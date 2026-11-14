mod features;
pub(crate) mod ipc;
mod protocol;
mod remote;
mod tabs;

use crate::adblock::AdBlocker;
use crate::favicon::FaviconCache;
use crate::history_db::HistoryDb;
use crate::i18n::{self, tr};
use crate::storage::{
    Bookmark, BrowserSettings, DownloadItem, HistoryItem, SessionTab, StorageManager, now_secs,
};
use crate::tab::Tab;
use crate::updater::{self, UiUpdateStatus, UpdateInfo};
use serde::Serialize;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tao::dpi::PhysicalSize;
use tao::event_loop::EventLoopProxy;
use tao::window::Window;
use wry::{
    PageLoadEvent, Rect, Theme, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows,
    WebViewExtWindows,
    dpi::{LogicalPosition, LogicalSize},
};

const TOP_BAR_HEIGHT: f64 = 84.0;
const BOOKMARKS_BAR_HEIGHT: f64 = 30.0;

// wry's default also disables msSmartScreenProtection (phishing/malware warnings); we keep it ON.
// All webviews sharing a profile must use identical args.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI";

const FAVICON_SCRIPT: &str = include_str!("../../assets/inject/favicon.js");
const FIND_SCRIPT: &str = include_str!("../../assets/inject/find.js");

// Read by the protocol handler (a plain fn) to stamp data-theme into served pages, so there is
// no wrong-theme flash on load.
pub(crate) static DARK_MODE: AtomicBool = AtomicBool::new(true);
pub(crate) static PRIVATE_MODE: AtomicBool = AtomicBool::new(false);

/// Background tabs unused this long are unloaded (when sleeping tabs are on).
const SLEEP_AFTER: Duration = Duration::from_secs(30 * 60);
const MAX_CLOSED_TABS: usize = 25;

fn theme_name(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

fn web_theme(dark: bool) -> Theme {
    if dark { Theme::Dark } else { Theme::Light }
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    // IPC commands from the chrome UI and internal pages
    IpcNewTab(Option<String>),
    IpcCloseTab(usize),
    IpcSwitchTab(usize),
    IpcNavigate(String),
    IpcBack,
    IpcForward,
    IpcReload,
    IpcToggleBookmark,
    IpcRemoveBookmark(String),
    IpcSetBookmarkFolder(String, String),
    /// (title, url, folder)
    IpcImportBookmarks(Vec<(String, String, String)>),
    IpcBookmarkFolderMenu(String),
    IpcClearHistory,
    IpcDeleteHistoryItem(String),
    IpcSearchHistory(String),
    IpcSetSearchEngine(String),
    IpcToggleAdblock,
    IpcToggleTheme,
    IpcSetSetting(String, serde_json::Value),
    IpcCheckUpdates,
    IpcDownloadUpdate,
    IpcInstallUpdate,
    IpcToggleAutoUpdate,
    IpcSetUpdateUrl(String),
    IpcRequestInitialState,
    IpcCloseActiveTab,
    IpcReopenTab,
    /// true = next tab, false = previous
    IpcCycleTab(bool),
    /// Ctrl+1..9: 0-based index, usize::MAX = last tab
    IpcSelectTab(usize),
    IpcAutocomplete(String),
    IpcFocusOmnibox,
    IpcFind,
    /// -1 = zoom out, 0 = reset, 1 = zoom in
    IpcZoom(i8),
    IpcPrint,
    IpcDevtools,
    IpcNewPrivateWindow,
    IpcTabMenu(usize),
    /// (tab id, new index)
    IpcReorderTab(usize, usize),
    IpcToggleMute(usize),
    IpcShieldMenu,
    /// ("open" | "show" | "cancel" | "remove" | "retry", download id)
    IpcDownloadAction(String, u64),
    IpcClearDownloads,
    IpcSetDefaultBrowser,
    IpcListSiteData,
    IpcClearSiteData(String),
    IpcUpdateFilterLists,
    IpcAllowHttp(String),
    IpcSnapLayouts,
    IpcMinimize,
    IpcMaximize,
    IpcClose,
    IpcDragWindow,
    /// Hosts whose site icons the page wants.
    IpcRequestFavicons(Vec<String>),
    /// (kind, request key, values): start-page widget data, see `widgets`.
    IpcWidgetFetch(String, String, serde_json::Value),

    /// URLs handed over by a second `Viima.exe <url>` launch.
    OpenUrls(Vec<String>),

    // Events from tab webviews
    TabTitleChanged(usize, String),
    TabPageLoadStarted(usize, String),
    TabPageLoadFinished(usize, String),
    TabFavicon(usize, String),
    TabAudio(usize, bool),
    TabZoom(usize, f64),
    TabNavigateTo(usize, String),
    HttpsFailed(usize, String),
    DownloadUpdate(DownloadItem),
    SiteData(Vec<(String, u32)>),
    SiteDataCleared(String),

    // Background work
    /// (host, icon as a data: URL or None when the site has none)
    FaviconsReady(Vec<(String, Option<String>)>),
    WidgetData {
        kind: String,
        key: String,
        url: String,
        result: Result<String, String>,
    },
    FilterListsUpdated(Result<usize, String>),
    Tick,
    UpdateCheckFinished {
        manual: bool,
        result: Result<Option<UpdateInfo>, String>,
    },
    UpdateDownloadFinished(Result<PathBuf, String>),
}

#[derive(Serialize)]
struct UiTabState {
    id: usize,
    title: String,
    url: String,
    favicon: Option<String>,
    blocked: u32,
    is_loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    pinned: bool,
    muted: bool,
    audible: bool,
    sleeping: bool,
    zoom: f64,
}

#[derive(Serialize)]
struct DownloadSummary {
    active: usize,
    /// Combined progress of running downloads, -1 when unknown.
    percent: i32,
    total: usize,
}

#[derive(Serialize, Default)]
struct FilterStatus {
    updating: bool,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InternalPage {
    NewTab,
    History,
    Bookmarks,
    Settings,
    Downloads,
    Insecure,
}

// Light updates (tab events) omit bookmarks/settings; the UI merges them into its state.
#[derive(Serialize)]
struct UiBrowserState<'a> {
    tabs: Vec<UiTabState>,
    active_tab_id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    bookmarks: Option<&'a [Bookmark]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<&'a BrowserSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<&'a UiUpdateStatus>,
    downloads: DownloadSummary,
    storage_error: Option<String>,
    private: bool,
}

#[derive(Serialize, Default)]
struct InternalPageState<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    bookmarks: Option<&'a [Bookmark]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    history: Option<Vec<HistoryItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    history_query: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<&'a BrowserSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<&'a UiUpdateStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    downloads: Option<&'a [DownloadItem]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    site_data: Option<&'a [(String, u32)]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter_lists: Option<&'a FilterStatus>,
    private: bool,
}

impl InternalPage {
    fn from_url(url: &str) -> Option<Self> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?;
        let name = match parsed.scheme() {
            "browser" => host,
            "http" => host.strip_prefix("browser.")?,
            _ => return None,
        };
        Some(match name {
            "newtab" => Self::NewTab,
            "history" => Self::History,
            "bookmarks" => Self::Bookmarks,
            "settings" => Self::Settings,
            "downloads" => Self::Downloads,
            "insecure" => Self::Insecure,
            _ => return None,
        })
    }

    fn browser_url(self) -> &'static str {
        match self {
            Self::NewTab => "browser://newtab",
            Self::History => "browser://history",
            Self::Bookmarks => "browser://bookmarks",
            Self::Settings => "browser://settings",
            Self::Downloads => "browser://downloads",
            Self::Insecure => "browser://insecure",
        }
    }
}

pub struct BrowserApp {
    window: Rc<Window>,
    web_context: WebContext,
    ui_webview: WebView,
    tabs: Vec<Tab>,
    active_tab_id: usize,
    next_tab_id: usize,
    storage: StorageManager,
    history: HistoryDb,
    adblock_enabled: Rc<Cell<bool>>,
    https_only: Rc<Cell<bool>>,
    /// Hosts the user chose to open over plain http this session (HTTPS-only mode).
    http_allowed: Rc<RefCell<HashSet<String>>>,
    #[cfg(windows)]
    download_ops: crate::webview_hooks::DownloadOps,
    proxy: EventLoopProxy<AppEvent>,
    window_size: PhysicalSize<u32>,
    update_status: UiUpdateStatus,
    pending_update: Option<UpdateInfo>,
    staged_update_path: Option<PathBuf>,
    /// Recently closed tabs for Ctrl+Shift+T (newest last).
    closed_tabs: Vec<SessionTab>,
    /// Private window: throwaway profile, nothing written to disk.
    private: bool,
    private_dir: Option<PathBuf>,
    history_query: String,
    site_data: Vec<(String, u32)>,
    filter_status: FilterStatus,
    last_filter_attempt: Option<Instant>,
    last_download_push: Instant,
    last_private_spawn: Option<Instant>,
    favicons: FaviconCache,
    widget_cache: remote::WidgetCache,
}

impl BrowserApp {
    pub fn new(
        window: Rc<Window>,
        proxy: EventLoopProxy<AppEvent>,
        private: bool,
        urls: Vec<String>,
    ) -> Self {
        let mut storage = StorageManager::load_or_default(private);
        i18n::set_language(&storage.data.settings.language);
        let dark = storage.data.settings.dark_mode;
        DARK_MODE.store(dark, Ordering::Relaxed);
        PRIVATE_MODE.store(private, Ordering::Relaxed);

        // History lives in SQLite; old JSON history is imported once.
        let mut history = HistoryDb::open(&crate::storage::config_dir().join("history.db"), private);
        if !private && !storage.data.history.is_empty() && history.is_available() {
            if history.is_empty() {
                history.import(&storage.data.history);
            }
            storage.data.history.clear();
            storage.save();
        }

        let adblock_enabled = Rc::new(Cell::new(storage.data.settings.adblock_enabled));
        let https_only = Rc::new(Cell::new(storage.data.settings.https_only));
        if !private {
            Self::restore_window_state(&window, &storage.data.session);
        }
        let window_size = window.inner_size();

        // One shared profile in AppData keeps logins/cookies; a private window gets a temp one.
        let private_dir = private.then(crate::storage::private_data_dir);
        let mut web_context = WebContext::new(match &private_dir {
            Some(dir) => Some(dir.clone()),
            None => crate::storage::webview_data_dir(),
        });

        let ui_proxy = proxy.clone();
        let ui_proxy_load = proxy.clone();
        let ui_webview = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_theme(web_theme(dark))
            .with_additional_browser_args(BROWSER_ARGS)
            .with_incognito(private)
            .with_devtools(cfg!(debug_assertions))
            .with_bounds(Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: LogicalSize::new(800.0, TOP_BAR_HEIGHT).into(),
            })
            .with_custom_protocol("browser".into(), protocol::handle_protocol)
            .with_ipc_handler(move |req| {
                if matches!(req.uri().host(), Some("browser.ui"))
                    && let Some(event) = ipc::parse(req.body(), true, None)
                {
                    let _ = ui_proxy.send_event(event);
                }
            })
            .with_on_page_load_handler(move |event, _| {
                if matches!(event, PageLoadEvent::Finished) {
                    let _ = ui_proxy_load.send_event(AppEvent::IpcRequestInitialState);
                }
            })
            .with_url("browser://ui/index.html")
            .build_as_child(&*window)
            .expect("Failed to initialize Chrome UI webview");

        let mut app = Self {
            window,
            web_context,
            ui_webview,
            tabs: Vec::new(),
            active_tab_id: 0,
            next_tab_id: 1,
            storage,
            history,
            adblock_enabled,
            https_only,
            http_allowed: Rc::default(),
            #[cfg(windows)]
            download_ops: Rc::default(),
            proxy,
            window_size,
            update_status: UiUpdateStatus::default(),
            pending_update: None,
            staged_update_path: None,
            closed_tabs: Vec::new(),
            private,
            private_dir,
            history_query: String::new(),
            site_data: Vec::new(),
            filter_status: FilterStatus::default(),
            last_filter_attempt: None,
            last_download_push: Instant::now(),
            last_private_spawn: None,
            favicons: FaviconCache::load(private),
            widget_cache: Default::default(),
        };
        app.layout();

        // Restore last session's tabs (normal window), then open any URLs from the command line.
        let session = app.storage.data.session.clone();
        if !private && !session.tabs.is_empty() {
            app.restore_tabs(session);
        }
        if urls.is_empty() {
            if app.tabs.is_empty() {
                let home = app.storage.data.settings.home_page.clone();
                app.new_tab(Some(home));
            }
        } else {
            for url in urls {
                app.new_tab(Some(url));
            }
        }

        app.start_background_tasks();
        app
    }

    fn start_background_tasks(&mut self) {
        let settings = &self.storage.data.settings;
        // Silent background update check after startup settles
        if settings.auto_update_enabled && !self.private {
            let url = settings.update_url.clone();
            let proxy = self.proxy.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(3));
                let result = updater::check_for_update(&url);
                let _ = proxy.send_event(AppEvent::UpdateCheckFinished {
                    manual: false,
                    result,
                });
            });
        }

        // Filter lists: load the cached copy, refresh it when older than a week.
        if settings.adblock_lists {
            let stale = now_secs().saturating_sub(settings.adblock_lists_updated)
                > crate::adblock::LIST_MAX_AGE_SECS;
            thread::spawn(AdBlocker::load_cached_lists);
            if stale && !self.private {
                self.update_filter_lists();
            }
        }

        // Once a minute: sleeping tabs, weekly filter refresh.
        let proxy = self.proxy.clone();
        thread::spawn(move || {
            while proxy.send_event(AppEvent::Tick).is_ok() {
                thread::sleep(Duration::from_secs(60));
            }
        });

        #[cfg(windows)]
        if !self.private {
            self.refresh_jump_list();
            // An installed copy keeps its "Default apps" registration pointing at itself.
            if updater::can_self_update()
                && let Ok(exe) = std::env::current_exe()
            {
                thread::spawn(move || {
                    let _ = crate::win_shell::register_browser_capabilities(&exe);
                });
            }
        }
    }

    #[cfg(windows)]
    fn refresh_jump_list(&self) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let tasks = vec![
            (
                tr("Uusi välilehti", "New tab").to_string(),
                "browser://newtab".to_string(),
            ),
            (
                tr("Uusi yksityinen ikkuna", "New private window").to_string(),
                "--private".to_string(),
            ),
        ];
        let bookmarks: Vec<(String, String)> = self
            .storage
            .data
            .bookmarks
            .iter()
            .rev()
            .take(6)
            .map(|b| (b.title.clone(), format!("\"{}\"", b.url.replace('"', "%22"))))
            .collect();
        let category = tr("Kirjanmerkit", "Bookmarks").to_string();
        thread::spawn(move || {
            let _ = crate::win_shell::set_jump_list(&exe, &tasks, &category, &bookmarks);
        });
    }

    #[cfg(windows)]
    fn hwnd(&self) -> isize {
        use tao::platform::windows::WindowExtWindows;
        self.window.hwnd()
    }

    /// Handles one event; returns true when the app should exit.
    pub fn handle_event(&mut self, event: AppEvent) -> bool {
        use AppEvent::*;
        match event {
            IpcNewTab(url) => self.new_tab(url),
            IpcCloseTab(id) => self.close_tab(id),
            IpcSwitchTab(id) => self.switch_tab(id),
            IpcNavigate(url) => self.navigate_active(&url),
            IpcBack => self.with_active(|t| t.go_back()),
            IpcForward => self.with_active(|t| t.go_forward()),
            IpcReload => self.with_active(|t| t.reload()),
            IpcToggleBookmark => self.toggle_bookmark(),
            IpcRemoveBookmark(url) => {
                self.storage.remove_bookmark(&url);
                self.bookmarks_changed();
            }
            IpcSetBookmarkFolder(url, folder) => {
                self.storage.set_bookmark_folder(&url, &folder);
                self.bookmarks_changed();
            }
            IpcImportBookmarks(items) => {
                self.storage.import_bookmarks(&items);
                self.bookmarks_changed();
            }
            IpcBookmarkFolderMenu(folder) => self.bookmark_folder_menu(&folder),
            IpcClearHistory => {
                self.history.clear();
                self.update_data();
            }
            IpcDeleteHistoryItem(url) => {
                self.history.delete_url(&url);
                self.update_data();
            }
            IpcSearchHistory(q) => {
                self.history_query = q.chars().take(200).collect();
                self.push_history_to_open_tabs();
            }
            IpcSetSearchEngine(engine) => {
                if self.storage.set_search_engine(engine) {
                    self.update_data();
                }
            }
            IpcToggleAdblock => self.toggle_adblock(),
            IpcToggleTheme => self.toggle_theme(),
            IpcSetSetting(key, value) => self.set_setting(&key, value),
            IpcCheckUpdates => self.check_for_updates(true),
            IpcDownloadUpdate => self.download_update(),
            IpcInstallUpdate => {
                if self.install_staged_update() {
                    self.flush();
                    return true;
                }
            }
            IpcToggleAutoUpdate => {
                self.storage.toggle_auto_update();
                self.update_data();
            }
            IpcSetUpdateUrl(url) => self.set_update_url(&url),
            IpcRequestInitialState => self.update_data(),
            IpcCloseActiveTab => self.close_tab(self.active_tab_id),
            IpcReopenTab => {
                if let Some(entry) = self.closed_tabs.pop() {
                    self.new_tab(Some(entry.url));
                }
            }
            IpcCycleTab(forward) => self.cycle_tab(forward),
            IpcSelectTab(index) => self.select_tab_index(index),
            IpcAutocomplete(typed) => self.autocomplete(&typed),
            IpcFocusOmnibox => {
                let _ = self.ui_webview.focus();
                let _ = self
                    .ui_webview
                    .evaluate_script("window.focusOmnibox && window.focusOmnibox();");
            }
            IpcFind => self.with_active(|t| {
                t.focus();
                t.run_script("window.__viimaFind && window.__viimaFind.open();");
            }),
            IpcZoom(dir) => self.zoom_active(dir),
            IpcPrint => self.with_active(|t| {
                if let Some(w) = &t.webview {
                    let _ = w.print();
                }
            }),
            IpcDevtools => {
                if self.storage.data.settings.devtools_enabled {
                    self.with_active(|t| {
                        if let Some(w) = &t.webview {
                            w.open_devtools();
                        }
                    });
                }
            }
            IpcNewPrivateWindow => self.open_private_window(),
            IpcTabMenu(id) => self.tab_menu(id),
            IpcReorderTab(id, index) => self.reorder_tab(id, index),
            IpcToggleMute(id) => self.toggle_mute(id),
            IpcShieldMenu => self.shield_menu(),
            IpcDownloadAction(op, id) => self.download_action(&op, id),
            IpcClearDownloads => {
                self.storage.clear_downloads();
                self.push_downloads(true);
            }
            IpcSetDefaultBrowser => self.set_default_browser(),
            IpcListSiteData => {
                #[cfg(windows)]
                crate::webview_hooks::list_cookie_sites(&self.ui_webview, self.proxy.clone());
            }
            IpcClearSiteData(site) => self.clear_site_data(&site),
            IpcUpdateFilterLists => self.update_filter_lists(),
            IpcAllowHttp(url) => self.allow_http(&url),
            IpcSnapLayouts => {
                #[cfg(windows)]
                crate::win_shell::show_snap_layouts(self.hwnd());
            }
            IpcMinimize => self.window.set_minimized(true),
            IpcMaximize => self.window.set_maximized(!self.window.is_maximized()),
            IpcClose => {
                self.flush();
                return true;
            }
            IpcDragWindow => {
                let _ = self.window.drag_window();
            }
            IpcRequestFavicons(hosts) => self.request_favicons(hosts),
            IpcWidgetFetch(kind, key, params) => self.widget_fetch(kind, key, params),
            FaviconsReady(results) => self.on_favicons_ready(results),
            WidgetData { kind, key, url, result } => self.on_widget_data(kind, key, url, result),
            OpenUrls(urls) => {
                if urls.is_empty() {
                    let home = self.storage.data.settings.home_page.clone();
                    self.new_tab(Some(home));
                }
                for url in urls {
                    self.new_tab(Some(url));
                }
                #[cfg(windows)]
                crate::win_shell::bring_to_front(self.hwnd());
            }
            TabTitleChanged(id, title) => self.on_title_changed(id, title),
            TabPageLoadStarted(id, url) => self.on_page_started(id, url),
            TabPageLoadFinished(id, url) => self.on_page_finished(id, url),
            TabFavicon(id, url) => {
                if let Some(tab) = self.tab_mut(id)
                    && tab.favicon.as_deref() != Some(url.as_str())
                {
                    tab.favicon = Some(url.clone());
                    self.update_ui();
                    self.learn_tab_favicon(id, &url);
                }
            }
            TabAudio(id, playing) => {
                if let Some(tab) = self.tab_mut(id)
                    && tab.playing_audio != playing
                {
                    tab.playing_audio = playing;
                    self.update_ui();
                }
            }
            TabZoom(id, factor) => self.on_zoom_changed(id, factor),
            TabNavigateTo(id, url) => {
                if let Some(tab) = self.tab_mut(id) {
                    tab.load_url(&url);
                }
            }
            HttpsFailed(id, http_url) => {
                let target = format!(
                    "browser://insecure?url={}",
                    url::form_urlencoded::byte_serialize(http_url.as_bytes()).collect::<String>()
                );
                if let Some(tab) = self.tab_mut(id) {
                    tab.load_url(&target);
                }
            }
            DownloadUpdate(item) => self.on_download_update(item),
            SiteData(list) => {
                self.site_data = list;
                self.update_data();
            }
            SiteDataCleared(site) => {
                self.site_data.retain(|(s, _)| *s != site);
                let active_site = self
                    .active_tab()
                    .and_then(|t| crate::storage::site_key(&t.url));
                if active_site.is_some_and(|a| a == site || a.ends_with(&format!(".{site}"))) {
                    self.with_active(|t| t.reload());
                }
                self.update_data();
            }
            FilterListsUpdated(result) => self.on_filter_lists_updated(result),
            Tick => self.on_tick(),
            UpdateCheckFinished { manual, result } => self.on_update_check_finished(manual, result),
            UpdateDownloadFinished(result) => self.on_update_download_finished(result),
        }
        false
    }

    fn top_height(&self) -> f64 {
        if self.storage.data.settings.show_bookmarks_bar {
            TOP_BAR_HEIGHT + BOOKMARKS_BAR_HEIGHT
        } else {
            TOP_BAR_HEIGHT
        }
    }

    fn content_rect(&self) -> Rect {
        let logical_size = self
            .window_size
            .to_logical::<f64>(self.window.scale_factor());
        let top = self.top_height();
        Rect {
            position: LogicalPosition::new(0.0, top).into(),
            size: LogicalSize::new(logical_size.width, (logical_size.height - top).max(1.0)).into(),
        }
    }

    /// Positions the chrome strip and the active tab for the current window size.
    fn layout(&self) {
        let logical_size = self
            .window_size
            .to_logical::<f64>(self.window.scale_factor());
        let _ = self.ui_webview.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(logical_size.width, self.top_height()).into(),
        });
        if let Some(tab) = self.active_tab() {
            tab.set_bounds(self.content_rect());
        }
    }

    pub fn on_resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.window_size = new_size;
        self.layout();
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == self.active_tab_id)
    }

    fn tab_mut(&mut self, id: usize) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    fn with_active(&mut self, f: impl FnOnce(&mut Tab)) {
        let id = self.active_tab_id;
        if let Some(tab) = self.tab_mut(id) {
            f(tab);
        }
    }

    fn autocomplete(&self, typed: &str) {
        let candidates = self.history.completion_candidates(typed);
        let suffix = self.storage.autocomplete(typed, &candidates);
        if let (Ok(q), Ok(s)) = (serde_json::to_string(typed), serde_json::to_string(&suffix)) {
            let _ = self.ui_webview.evaluate_script(&format!(
                "window.onAutocomplete && window.onAutocomplete({q}, {s});"
            ));
        }
    }

    fn toggle_bookmark(&mut self) {
        let Some(tab) = self.active_tab() else { return };
        let (title, url) = (tab.title.clone(), tab.url.clone());
        self.storage.toggle_bookmark(title, url);
        self.bookmarks_changed();
    }

    fn bookmarks_changed(&mut self) {
        self.update_data();
        #[cfg(windows)]
        if !self.private {
            self.refresh_jump_list();
        }
    }

    fn toggle_adblock(&mut self) {
        let enabled = self.storage.toggle_adblock();
        self.adblock_enabled.set(enabled);
        for tab in &self.tabs {
            let effective = enabled && !tab.site_allowed.get();
            tab.run_script(&format!("window.__viimaAdblockSetEnabled({effective});"));
        }
        self.update_data();
    }

    fn toggle_theme(&mut self) {
        let dark = self.storage.toggle_dark_mode();
        DARK_MODE.store(dark, Ordering::Relaxed);
        let script = format!(
            "document.documentElement.dataset.theme = '{}';",
            theme_name(dark)
        );
        // Chrome + internal pages follow data-theme; websites follow prefers-color-scheme.
        let _ = self.ui_webview.evaluate_script(&script);
        let _ = self.ui_webview.set_theme(web_theme(dark));
        for tab in &self.tabs {
            tab.set_theme(web_theme(dark));
            if InternalPage::from_url(&tab.url).is_some() {
                tab.run_script(&script);
            }
        }
        self.update_data();
    }

    fn on_page_started(&mut self, id: usize, url: String) {
        let site = crate::storage::site_key(&url);
        let allowed = site
            .as_deref()
            .is_some_and(|s| self.storage.is_adblock_allowed(s));
        let zoom = site.as_deref().map_or(1.0, |s| self.storage.site_zoom(s));
        if let Some(tab) = self.tab_mut(id) {
            tab.on_page_started(&url);
            tab.site_allowed.set(allowed);
            tab.set_zoom(zoom);
            self.update_ui();
        }
    }

    fn on_page_finished(&mut self, id: usize, url: String) {
        let enabled = self.adblock_enabled.get();
        let hide_cookies = self.storage.data.settings.hide_cookie_banners;
        let Some(tab) = self.tab_mut(id) else { return };
        tab.on_page_finished(&url);
        // The init script ran with the settings of when the webview was created.
        let effective = enabled && !tab.site_allowed.get();
        tab.run_script(&format!(
            "window.__viimaAdblockSetEnabled({effective}); window.__viimaCookieHide && window.__viimaCookieHide({hide_cookies});"
        ));
        let title = tab.title.clone();
        self.history.add_visit(&title, &url);
        self.update_ui();
        self.push_history_to_open_tabs();
        self.sync_session();
    }

    fn on_title_changed(&mut self, id: usize, title: String) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.on_title_changed(&title);
        let (url, title) = (tab.url.clone(), tab.title.clone());
        if id == self.active_tab_id {
            self.sync_window_title();
        }
        self.history.update_title(&url, &title);
        self.update_ui();
        self.sync_session();
    }

    fn window_title(&self, page_title: &str) -> String {
        // Internal pages already end their titles with "Viima"; don't append it twice.
        let base = if page_title.ends_with("Viima") {
            page_title.to_string()
        } else {
            format!("{page_title} - Viima")
        };
        if self.private {
            format!("{base} ({})", tr("yksityinen", "private"))
        } else {
            base
        }
    }

    fn sync_window_title(&self) {
        if let Some(tab) = self.active_tab() {
            self.window.set_title(&self.window_title(&tab.title));
        }
    }

    /// Light push (tab events): tab list only, to the chrome webview.
    pub fn update_ui(&self) {
        self.push_state(false);
    }

    /// Full push (bookmarks/history/settings changed): chrome + open internal pages.
    pub fn update_data(&self) {
        self.push_state(true);
    }

    /// Persist pending writes; call before exiting.
    pub fn flush(&mut self) {
        if !self.private {
            self.sync_session();
            self.save_window_state();
        }
        self.storage.flush();
        self.favicons.save();
        if let Some(dir) = self.private_dir.take() {
            // WebView2 still holds the profile for a moment after exit: delete it from a
            // detached helper. Leftovers are also swept on the next private window.
            self.tabs.clear();
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                let dir = dir.to_string_lossy().replace('"', "");
                let _ = std::process::Command::new("cmd.exe")
                    .raw_arg(format!(
                        "/S /C \"ping 127.0.0.1 -n 4 > nul & rmdir /s /q \"{dir}\"\""
                    ))
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn();
            }
        }
    }

    fn set_update_url(&mut self, url: &str) {
        if self.storage.set_update_url(url.to_string()) {
            self.update_status.error = None;
        } else {
            self.update_status.state = "error";
            self.update_status.error = Some(
                tr(
                    "Päivitys-URL:n täytyy alkaa https://",
                    "The update URL must start with https://",
                )
                .into(),
            );
        }
        self.update_data();
    }

    fn check_for_updates(&mut self, manual: bool) {
        if matches!(self.update_status.state, "checking" | "downloading") {
            return;
        }
        self.update_status.state = "checking";
        self.update_status.error = None;
        self.update_data();

        let url = self.storage.data.settings.update_url.clone();
        let proxy = self.proxy.clone();
        thread::spawn(move || {
            let result = updater::check_for_update(&url);
            let _ = proxy.send_event(AppEvent::UpdateCheckFinished { manual, result });
        });
    }

    fn on_update_check_finished(&mut self, manual: bool, result: Result<Option<UpdateInfo>, String>) {
        match result {
            Ok(Some(info)) => {
                self.update_status.latest_version = Some(info.version.clone());
                self.update_status.release_notes = info.release_notes.clone();
                self.update_status.error = None;
                self.pending_update = Some(info);

                if self.storage.data.settings.auto_update_enabled
                    && self.update_status.can_self_update
                {
                    self.download_update();
                    return;
                }
                self.update_status.state = "available";
                self.update_data();
            }
            Ok(None) => {
                self.pending_update = None;
                self.update_status.latest_version = None;
                self.update_status.release_notes = None;
                self.update_status.error = None;
                self.update_status.state = if manual { "up_to_date" } else { "idle" };
                self.update_data();
            }
            Err(err) => {
                if manual {
                    self.update_status.state = "error";
                    self.update_status.error = Some(err);
                    self.update_data();
                } else {
                    self.update_status.state = "idle";
                }
            }
        }
    }

    fn download_update(&mut self) {
        if matches!(self.update_status.state, "downloading" | "ready") {
            return;
        }
        let Some(info) = self.pending_update.clone() else {
            return;
        };
        if !self.update_status.can_self_update {
            self.update_status.state = "error";
            self.update_status.error = Some(
                tr(
                    "Automaattinen päivitys on käytettävissä vain asennetussa tai julkaistussa versiossa.",
                    "Automatic updates only work in the installed or released version.",
                )
                .into(),
            );
            self.update_data();
            return;
        }
        self.update_status.state = "downloading";
        self.update_status.error = None;
        self.update_data();

        let proxy = self.proxy.clone();
        thread::spawn(move || {
            let result = updater::download_and_stage_update(&info);
            let _ = proxy.send_event(AppEvent::UpdateDownloadFinished(result));
        });
    }

    fn on_update_download_finished(&mut self, result: Result<PathBuf, String>) {
        match result {
            Ok(path) => {
                self.staged_update_path = Some(path);
                self.update_status.state = "ready";
                self.update_status.error = None;
            }
            Err(err) => {
                self.staged_update_path = None;
                self.update_status.state = "error";
                self.update_status.error = Some(err);
            }
        }
        self.update_data();
    }

    fn install_staged_update(&mut self) -> bool {
        let Some(path) = &self.staged_update_path else {
            return false;
        };
        #[cfg(windows)]
        match updater::launch_staged_update(path) {
            Ok(()) => true,
            Err(err) => {
                self.update_status.state = "error";
                self.update_status.error = Some(err);
                self.update_data();
                false
            }
        }
        #[cfg(not(windows))]
        false
    }

    /// `page` = the internal page name the data is meant for. The origin is checked inside the
    /// page when the script runs: our cached tab URL can lag a navigation, and history or
    /// bookmarks must never reach a web page that just loaded in that tab.
    fn state_script(page: &str, json: &str) -> String {
        format!(
            "if (location.origin === 'http://browser.{page}' && window.onBrowserStateUpdate) {{ window.onBrowserStateUpdate({json}); }}"
        )
    }

    fn internal_state_script(kind: InternalPage, state: &InternalPageState<'_>) -> Option<String> {
        let page = kind.browser_url().strip_prefix("browser://")?;
        serde_json::to_string(state)
            .ok()
            .map(|json| Self::state_script(page, &json))
    }

    fn push_to_pages(&self, kind: InternalPage, state: &InternalPageState<'_>) {
        if !self
            .tabs
            .iter()
            .any(|t| InternalPage::from_url(&t.url) == Some(kind))
        {
            return;
        }
        if let Some(script) = Self::internal_state_script(kind, state) {
            for tab in &self.tabs {
                if InternalPage::from_url(&tab.url) == Some(kind) {
                    tab.run_script(&script);
                }
            }
        }
    }

    fn history_page_state(&self) -> InternalPageState<'_> {
        InternalPageState {
            history: Some(self.history.search(&self.history_query, 300)),
            history_query: Some(&self.history_query),
            settings: Some(&self.storage.data.settings),
            private: self.private,
            ..Default::default()
        }
    }

    fn push_history_to_open_tabs(&self) {
        if self
            .tabs
            .iter()
            .any(|t| InternalPage::from_url(&t.url) == Some(InternalPage::History))
        {
            self.push_to_pages(InternalPage::History, &self.history_page_state());
        }
    }

    /// Download list to open browser://downloads pages (+ the toolbar summary).
    fn push_downloads(&self, chrome_too: bool) {
        if chrome_too {
            self.update_ui();
        }
        self.push_to_pages(
            InternalPage::Downloads,
            &InternalPageState {
                downloads: Some(&self.storage.data.downloads),
                settings: Some(&self.storage.data.settings),
                private: self.private,
                ..Default::default()
            },
        );
    }

    fn download_summary(&self) -> DownloadSummary {
        let running: Vec<&DownloadItem> = self
            .storage
            .data
            .downloads
            .iter()
            .filter(|d| d.state == "in_progress")
            .collect();
        let (got, total) = running.iter().fold((0i64, 0i64), |(g, t), d| {
            if d.total > 0 { (g + d.received, t + d.total) } else { (g, t) }
        });
        DownloadSummary {
            active: running.len(),
            percent: if total > 0 {
                ((got * 100) / total) as i32
            } else {
                -1
            },
            total: self.storage.data.downloads.len(),
        }
    }

    fn push_state(&self, full: bool) {
        let ui_tabs: Vec<UiTabState> = self
            .tabs
            .iter()
            .map(|t| {
                let is_active = t.id == self.active_tab_id;
                UiTabState {
                    id: t.id,
                    title: t.title.clone(),
                    url: t.url.clone(),
                    favicon: t.favicon.clone(),
                    blocked: t.blocked.get().total(),
                    is_loading: t.is_loading,
                    can_go_back: is_active && t.can_go_back(),
                    can_go_forward: is_active && t.can_go_forward(),
                    pinned: t.pinned,
                    muted: t.muted,
                    audible: t.playing_audio,
                    sleeping: t.webview.is_none(),
                    zoom: t.zoom,
                }
            })
            .collect();

        let data = &self.storage.data;
        let state = UiBrowserState {
            tabs: ui_tabs,
            active_tab_id: self.active_tab_id,
            bookmarks: full.then_some(&data.bookmarks[..]),
            settings: full.then_some(&data.settings),
            update: full.then_some(&self.update_status),
            downloads: self.download_summary(),
            storage_error: self.storage.error(),
            private: self.private,
        };
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = self.ui_webview.evaluate_script(&Self::state_script("ui", &json));
        }

        if !full {
            return;
        }
        let base = || InternalPageState {
            settings: Some(&data.settings),
            private: self.private,
            ..Default::default()
        };
        self.push_to_pages(
            InternalPage::NewTab,
            &InternalPageState {
                bookmarks: Some(&data.bookmarks),
                ..base()
            },
        );
        self.push_history_to_open_tabs();
        self.push_to_pages(
            InternalPage::Bookmarks,
            &InternalPageState {
                bookmarks: Some(&data.bookmarks),
                ..base()
            },
        );
        self.push_to_pages(
            InternalPage::Settings,
            &InternalPageState {
                update: Some(&self.update_status),
                site_data: Some(&self.site_data),
                filter_lists: Some(&self.filter_status),
                ..base()
            },
        );
        self.push_downloads(false);
        self.push_to_pages(InternalPage::Insecure, &base());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_internal_page_kind() {
        assert!(InternalPage::from_url("browser://downloads") == Some(InternalPage::Downloads));
        assert!(
            InternalPage::from_url("http://browser.insecure/?url=x") == Some(InternalPage::Insecure)
        );
        assert!(InternalPage::from_url("https://browser.settings/").is_none());
        assert!(InternalPage::from_url("http://browser.evil/").is_none());
    }
}
