use crate::adblock::AdBlocker;
use crate::storage::{StorageManager, is_internal_url};
use crate::tab::Tab;
use crate::updater::{self, UiUpdateStatus, UpdateInfo};
use crate::url_resolver::UrlResolver;
use serde::Serialize;
use std::borrow::Cow;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tao::dpi::PhysicalSize;
use tao::event_loop::EventLoopProxy;
use tao::window::Window;
use wry::{
    NewWindowResponse, PageLoadEvent, Rect, Theme, WebContext, WebView, WebViewBuilder,
    WebViewBuilderExtWindows, WebViewExtWindows,
    dpi::{LogicalPosition, LogicalSize},
    http::{Request, Response},
};

const TOP_BAR_LOGICAL_HEIGHT: f64 = 84.0;

// wry's default also disables msSmartScreenProtection (phishing/malware warnings); we keep it ON.
// All webviews sharing a profile must use identical args.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI";

// Read by the protocol handler (a plain fn) to stamp data-theme into served pages, so there is
// no wrong-theme flash on load.
static DARK_MODE: AtomicBool = AtomicBool::new(true);

fn theme_name(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

fn web_theme(dark: bool) -> Theme {
    if dark { Theme::Dark } else { Theme::Light }
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    // IPC commands from Top Bar UI
    IpcNewTab(Option<String>),
    IpcCloseTab(usize),
    IpcSwitchTab(usize),
    IpcNavigate(String),
    IpcBack,
    IpcForward,
    IpcReload,
    IpcToggleBookmark,
    IpcRemoveBookmark(String),
    IpcClearHistory,
    IpcDeleteHistoryItem(String),
    IpcSetSearchEngine(String),
    IpcToggleAdblock,
    IpcToggleTheme,
    IpcCheckUpdates,
    IpcDownloadUpdate,
    IpcInstallUpdate,
    IpcToggleAutoUpdate,
    IpcSetUpdateUrl(String),
    IpcRequestInitialState,
    IpcCloseActiveTab,
    IpcFocusOmnibox,
    IpcMinimize,
    IpcMaximize,
    IpcClose,
    IpcDragWindow,

    // Events from Tab WebViews
    TabTitleChanged(usize, String),
    TabPageLoadStarted(usize, String),
    TabPageLoadFinished(usize, String),

    // Background Auto-Updater Events
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
    is_loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InternalPage {
    NewTab,
    History,
    Bookmarks,
    Settings,
}

// Light updates (tab events) omit bookmarks/history/settings; the UI merges them into its state.
#[derive(Serialize)]
struct UiBrowserState<'a> {
    tabs: Vec<UiTabState>,
    active_tab_id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    bookmarks: Option<&'a [crate::storage::Bookmark]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<&'a crate::storage::BrowserSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<&'a UiUpdateStatus>,
    storage_error: Option<String>,
}

#[derive(Serialize)]
struct InternalPageState<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    bookmarks: Option<&'a [crate::storage::Bookmark]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    history: Option<&'a [crate::storage::HistoryItem]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<&'a crate::storage::BrowserSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<&'a UiUpdateStatus>,
}

// Keyboard shortcuts for focused web pages (the chrome webview never sees those key events).
// Only Ctrl+T/W/L/H: reload and history navigation are already handled natively by WebView2.
const SHORTCUT_SCRIPT: &str = r#"
window.addEventListener('keydown', function (e) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey || !window.ipc) return;
    const key = { t: 'new_tab', w: 'close_tab', l: 'focus_omnibox', h: 'history' }[(e.key || '').toLowerCase()];
    if (!key) return;
    e.preventDefault();
    window.ipc.postMessage(JSON.stringify({ action: 'shortcut', key }));
}, true);
"#;

pub struct BrowserApp {
    window: Rc<Window>,
    web_context: WebContext,
    ui_webview: WebView,
    tabs: Vec<Tab>,
    active_tab_id: usize,
    next_tab_id: usize,
    storage: StorageManager,
    adblock_enabled: Rc<Cell<bool>>,
    proxy: EventLoopProxy<AppEvent>,
    window_size: PhysicalSize<u32>,
    update_status: UiUpdateStatus,
    pending_update: Option<UpdateInfo>,
    staged_update_path: Option<PathBuf>,
}

impl BrowserApp {
    pub fn new(window: Rc<Window>, proxy: EventLoopProxy<AppEvent>) -> Self {
        let storage = StorageManager::load_or_default();
        let dark = storage.data.settings.dark_mode;
        DARK_MODE.store(dark, Ordering::Relaxed);
        let adblock_enabled = Rc::new(Cell::new(storage.data.settings.adblock_enabled));
        let scale_factor = window.scale_factor();
        let window_size = window.inner_size();
        let logical_size = window_size.to_logical::<f64>(scale_factor);

        let top_rect = Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(logical_size.width, TOP_BAR_LOGICAL_HEIGHT).into(),
        };

        // One shared profile in AppData: keeps logins/cookies and keeps the exe folder clean.
        let mut web_context = WebContext::new(crate::storage::webview_data_dir());

        // Create UI Webview (Tabs strip + Omnibox + Custom Window Controls)
        let ui_proxy = proxy.clone();
        let ui_proxy_load = proxy.clone();
        let ui_webview = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_theme(web_theme(dark))
            .with_additional_browser_args(BROWSER_ARGS)
            .with_bounds(top_rect)
            .with_custom_protocol("browser".into(), Self::handle_protocol)
            .with_ipc_handler(move |req| {
                if matches!(req.uri().host(), Some("browser.ui")) {
                    Self::handle_ipc_message(req.body(), &ui_proxy, true);
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
            adblock_enabled,
            proxy,
            window_size,
            update_status: UiUpdateStatus::default(),
            pending_update: None,
            staged_update_path: None,
        };

        // Open initial tab (defaults to Google search & start page)
        let home_page = app.storage.data.settings.home_page.clone();
        app.new_tab(Some(home_page));

        // Silent background update check after startup settles
        if app.storage.data.settings.auto_update_enabled {
            let url = app.storage.data.settings.update_url.clone();
            let update_proxy = app.proxy.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(3));
                let result = updater::check_for_update(&url);
                let _ = update_proxy.send_event(AppEvent::UpdateCheckFinished {
                    manual: false,
                    result,
                });
            });
        }

        app
    }

    fn content_rect(&self) -> Rect {
        let scale_factor = self.window.scale_factor();
        let logical_size = self.window_size.to_logical::<f64>(scale_factor);
        let content_height = (logical_size.height - TOP_BAR_LOGICAL_HEIGHT).max(1.0);
        Rect {
            position: LogicalPosition::new(0.0, TOP_BAR_LOGICAL_HEIGHT).into(),
            size: LogicalSize::new(logical_size.width, content_height).into(),
        }
    }

    fn handle_protocol(_id: &str, req: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
        let uri = req.uri();
        let host = uri.host().unwrap_or("");
        let path = uri.path();

        let is_ui = matches!(host, "ui" | "browser.ui");
        let is_newtab = matches!(host, "newtab" | "browser.newtab");
        let is_history = matches!(host, "history" | "browser.history");
        let is_bookmarks = matches!(host, "bookmarks" | "browser.bookmarks");
        let is_settings = matches!(host, "settings" | "browser.settings");

        let (content_type, raw_bytes): (&'static str, &'static [u8]) = if path == "/viima-logo.png"
        {
            (
                "image/png",
                include_bytes!("../assets/branding/viima-logo.png"),
            )
        } else if is_ui {
            match path {
                "/style.css" => (
                    "text/css; charset=utf-8",
                    include_bytes!("../assets/ui/style.css"),
                ),
                "/app.js" => (
                    "application/javascript; charset=utf-8",
                    include_bytes!("../assets/ui/app.js"),
                ),
                _ => (
                    "text/html; charset=utf-8",
                    include_bytes!("../assets/ui/index.html"),
                ),
            }
        } else if is_newtab {
            match path {
                "/wallpapers/aurora-dark.jpg" => (
                    "image/jpeg",
                    include_bytes!("../assets/internal/wallpapers/aurora-dark.jpg"),
                ),
                "/wallpapers/glass-waves.jpg" => (
                    "image/jpeg",
                    include_bytes!("../assets/internal/wallpapers/glass-waves.jpg"),
                ),
                "/wallpapers/nordic-light.jpg" => (
                    "image/jpeg",
                    include_bytes!("../assets/internal/wallpapers/nordic-light.jpg"),
                ),
                _ => (
                    "text/html; charset=utf-8",
                    include_bytes!("../assets/internal/newtab.html"),
                ),
            }
        } else if is_history {
            (
                "text/html; charset=utf-8",
                include_bytes!("../assets/internal/history.html"),
            )
        } else if is_bookmarks {
            (
                "text/html; charset=utf-8",
                include_bytes!("../assets/internal/bookmarks.html"),
            )
        } else if is_settings {
            (
                "text/html; charset=utf-8",
                include_bytes!("../assets/internal/settings.html"),
            )
        } else {
            (
                "text/html; charset=utf-8",
                b"<!DOCTYPE html><html><body><h1>404 Sivua ei l\xc3\xb6ytynyt</h1></body></html>",
            )
        };

        let body: Cow<'static, [u8]> = if content_type.starts_with("text/html") {
            let html = std::str::from_utf8(raw_bytes).unwrap_or("");
            let attr = format!(
                "<html lang=\"fi\" data-theme=\"{}\">",
                theme_name(DARK_MODE.load(Ordering::Relaxed))
            );
            let themed = html
                .replacen("<html lang=\"fi\">", &attr, 1)
                .replace("{{VERSION}}", updater::CURRENT_VERSION);
            Cow::Owned(themed.into_bytes())
        } else {
            Cow::Borrowed(raw_bytes)
        };

        // Internal pages hold privileged IPC: never let a web page frame them (clickjacking).
        let mut builder = Response::builder()
            .header("Content-Type", content_type)
            .header("X-Frame-Options", "DENY")
            .header("Content-Security-Policy", "frame-ancestors 'none'")
            .header("X-Content-Type-Options", "nosniff");
        if content_type.starts_with("text/html") {
            builder = builder.header("Cache-Control", "no-store");
        } else if content_type.starts_with("image/") {
            builder = builder.header("Cache-Control", "public, max-age=86400");
        }
        builder.body(body).unwrap()
    }

    // Untrusted (web page) senders may only trigger the harmless "shortcut" action.
    fn handle_ipc_message(body: &str, proxy: &EventLoopProxy<AppEvent>, trusted: bool) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body)
            && let Some(action) = v.get("action").and_then(|a| a.as_str())
        {
            if !trusted && action != "shortcut" {
                return;
            }
            match action {
                "shortcut" => {
                    let event = match v.get("key").and_then(|k| k.as_str()) {
                        Some("new_tab") => AppEvent::IpcNewTab(None),
                        Some("close_tab") => AppEvent::IpcCloseActiveTab,
                        Some("focus_omnibox") => AppEvent::IpcFocusOmnibox,
                        Some("history") => AppEvent::IpcNavigate("browser://history".into()),
                        _ => return,
                    };
                    let _ = proxy.send_event(event);
                }
                "new_tab" => {
                    let url = v.get("url").and_then(|u| u.as_str()).map(|s| s.to_string());
                    let _ = proxy.send_event(AppEvent::IpcNewTab(url));
                }
                "close_tab" => {
                    if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                        let _ = proxy.send_event(AppEvent::IpcCloseTab(id as usize));
                    }
                }
                "switch_tab" => {
                    if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                        let _ = proxy.send_event(AppEvent::IpcSwitchTab(id as usize));
                    }
                }
                "navigate" => {
                    if let Some(url) = v.get("url").and_then(|u| u.as_str()) {
                        let _ = proxy.send_event(AppEvent::IpcNavigate(url.to_string()));
                    }
                }
                "back" => {
                    let _ = proxy.send_event(AppEvent::IpcBack);
                }
                "forward" => {
                    let _ = proxy.send_event(AppEvent::IpcForward);
                }
                "reload" => {
                    let _ = proxy.send_event(AppEvent::IpcReload);
                }
                "toggle_bookmark" => {
                    let _ = proxy.send_event(AppEvent::IpcToggleBookmark);
                }
                "remove_bookmark" => {
                    if let Some(url) = v.get("url").and_then(|u| u.as_str()) {
                        let _ = proxy.send_event(AppEvent::IpcRemoveBookmark(url.to_string()));
                    }
                }
                "clear_history" => {
                    let _ = proxy.send_event(AppEvent::IpcClearHistory);
                }
                "delete_history_item" => {
                    if let Some(url) = v.get("url").and_then(|u| u.as_str()) {
                        let _ = proxy.send_event(AppEvent::IpcDeleteHistoryItem(url.to_string()));
                    }
                }
                "set_search_engine" => {
                    if let Some(engine) = v.get("engine").and_then(|e| e.as_str()) {
                        let _ = proxy.send_event(AppEvent::IpcSetSearchEngine(engine.to_string()));
                    }
                }
                "toggle_adblock" => {
                    let _ = proxy.send_event(AppEvent::IpcToggleAdblock);
                }
                "toggle_theme" => {
                    let _ = proxy.send_event(AppEvent::IpcToggleTheme);
                }
                "check_updates" => {
                    let _ = proxy.send_event(AppEvent::IpcCheckUpdates);
                }
                "download_update" => {
                    let _ = proxy.send_event(AppEvent::IpcDownloadUpdate);
                }
                "install_update" => {
                    let _ = proxy.send_event(AppEvent::IpcInstallUpdate);
                }
                "toggle_auto_update" => {
                    let _ = proxy.send_event(AppEvent::IpcToggleAutoUpdate);
                }
                "set_update_url" => {
                    if let Some(url) = v.get("url").and_then(|u| u.as_str()) {
                        let _ = proxy.send_event(AppEvent::IpcSetUpdateUrl(url.to_string()));
                    }
                }
                "request_initial_state" => {
                    let _ = proxy.send_event(AppEvent::IpcRequestInitialState);
                }
                "minimize" => {
                    let _ = proxy.send_event(AppEvent::IpcMinimize);
                }
                "maximize" => {
                    let _ = proxy.send_event(AppEvent::IpcMaximize);
                }
                "close" => {
                    let _ = proxy.send_event(AppEvent::IpcClose);
                }
                "drag_window" => {
                    let _ = proxy.send_event(AppEvent::IpcDragWindow);
                }
                _ => {}
            }
        }
    }

    fn create_tab_webview(&mut self, id: usize, initial_url: &str) -> WebView {
        let rect = self.content_rect();
        let proxy_page_load = self.proxy.clone();
        let proxy_title = self.proxy.clone();
        let proxy_new_tab = self.proxy.clone();
        let proxy_ipc = self.proxy.clone();
        let adblock_enabled = self.storage.data.settings.adblock_enabled;
        let adblock_policy = self.adblock_enabled.clone();

        let mut builder = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_theme(web_theme(DARK_MODE.load(Ordering::Relaxed)))
            .with_additional_browser_args(BROWSER_ARGS)
            .with_bounds(rect)
            .with_custom_protocol("browser".into(), Self::handle_protocol)
            .with_ipc_handler(move |req| {
                let trusted = Self::is_trusted_ipc_source(req.uri());
                Self::handle_ipc_message(req.body(), &proxy_ipc, trusted);
            })
            .with_on_page_load_handler(move |event, url| match event {
                PageLoadEvent::Started => {
                    let _ = proxy_page_load
                        .send_event(AppEvent::TabPageLoadStarted(id, url.to_string()));
                }
                PageLoadEvent::Finished => {
                    let _ = proxy_page_load
                        .send_event(AppEvent::TabPageLoadFinished(id, url.to_string()));
                }
            })
            .with_document_title_changed_handler(move |title| {
                let _ = proxy_title.send_event(AppEvent::TabTitleChanged(id, title));
            })
            .with_navigation_handler(move |url| {
                // If navigation matches ad blocker domain, block it
                if adblock_policy.get() && AdBlocker::should_block_url(&url) {
                    return false;
                }
                true
            })
            .with_new_window_req_handler(move |url, _| {
                // Open new window requests (e.g. target="_blank") cleanly as tabs.
                // Web pages may only open http(s): never file://, browser:// or about: pages.
                if url.starts_with("http://") || url.starts_with("https://") {
                    let _ = proxy_new_tab.send_event(AppEvent::IpcNewTab(Some(url)));
                }
                NewWindowResponse::Deny
            });

        builder = builder.with_initialization_script(format!(
            "{} window.__viimaAdblockSetEnabled({}); {}",
            AdBlocker::get_injection_script(),
            adblock_enabled,
            SHORTCUT_SCRIPT
        ));

        let webview = builder
            .with_url(initial_url)
            .build_as_child(&*self.window)
            .expect("Failed to create tab webview");
        #[cfg(windows)]
        if let Err(e) = AdBlocker::install_network_filter(&webview, self.adblock_enabled.clone()) {
            eprintln!("adblock network filter unavailable: {e}");
        }
        webview
    }

    pub fn new_tab(&mut self, url: Option<String>) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;

        let initial_url = url.unwrap_or_else(|| "browser://newtab".to_string());
        let resolved =
            UrlResolver::resolve(&initial_url, &self.storage.data.settings.search_engine);

        // Hide all existing tabs before creating and showing new one
        for tab in &self.tabs {
            tab.set_visible(false);
        }

        let init_adblock = self.adblock_enabled.get();
        let webview = self.create_tab_webview(id, &resolved);
        let tab = Tab::new(id, webview, resolved, init_adblock);
        tab.focus();
        self.tabs.push(tab);
        self.active_tab_id = id;

        self.sync_window_title();
        self.update_ui();
    }

    pub fn close_tab(&mut self, id: usize) {
        if self.tabs.len() <= 1 {
            // If only one tab left, reset it to newtab instead of closing window
            if let Some(tab) = self.tabs.first_mut()
                && Self::internal_page_kind(&tab.url) != Some(InternalPage::NewTab)
            {
                tab.load_url("browser://newtab");
                tab.url = "browser://newtab".to_string();
                tab.title = "Uusi välilehti".to_string();
                tab.is_loading = true;
                self.sync_window_title();
                self.update_ui();
            }
            return;
        }

        let index = self.tabs.iter().position(|t| t.id == id);
        if let Some(idx) = index {
            let was_active = self.active_tab_id == id;
            let removed = self.tabs.remove(idx);
            removed.set_visible(false);

            if was_active {
                // Activate adjacent tab
                let new_idx = if idx >= self.tabs.len() {
                    self.tabs.len() - 1
                } else {
                    idx
                };
                self.active_tab_id = self.tabs[new_idx].id;
                let content_rect = self.content_rect();
                for tab in &self.tabs {
                    if tab.id == self.active_tab_id {
                        tab.set_bounds(content_rect);
                        tab.set_visible(true);
                        tab.focus();
                    } else {
                        tab.set_visible(false);
                    }
                }
                self.sync_window_title();
            }

            self.update_ui();
        }
    }

    pub fn switch_tab(&mut self, id: usize) {
        if self.active_tab_id == id || !self.tabs.iter().any(|tab| tab.id == id) {
            return;
        }

        let content_rect = self.content_rect();
        for tab in &self.tabs {
            if tab.id == id {
                tab.set_bounds(content_rect);
                tab.set_visible(true);
                tab.focus();
            } else {
                tab.set_visible(false);
            }
        }

        self.active_tab_id = id;
        self.sync_window_title();
        self.update_ui();
    }

    pub fn navigate_active(&mut self, input: &str) {
        let engine = &self.storage.data.settings.search_engine;
        let resolved = UrlResolver::resolve(input, engine);
        if self.adblock_enabled.get() && AdBlocker::should_block_url(&resolved) {
            return;
        }

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
            tab.load_url(&resolved);
            tab.url = resolved;
            tab.is_loading = true;
            tab.focus();
            self.update_ui();
        }
    }

    pub fn go_back(&mut self) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
            tab.go_back();
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == self.active_tab_id) {
            tab.go_forward();
        }
    }

    pub fn reload(&mut self) {
        if let Some(tab) = self.tabs.iter().find(|t| t.id == self.active_tab_id) {
            tab.reload();
        }
    }

    pub fn toggle_bookmark(&mut self) {
        let Some(tab) = self.tabs.iter().find(|t| t.id == self.active_tab_id) else {
            return;
        };
        if tab.url.is_empty() || tab.url == "about:blank" || is_internal_url(&tab.url) {
            return;
        }
        let url = tab.url.clone();
        let title = tab.title.clone();
        self.storage.toggle_bookmark(title, url);
        self.update_data();
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        self.storage.remove_bookmark(url);
        self.update_data();
    }

    pub fn clear_history(&mut self) {
        self.storage.clear_history();
        self.update_data();
    }

    pub fn delete_history_item(&mut self, url: &str) {
        self.storage.delete_history_item(url);
        self.update_data();
    }

    pub fn set_search_engine(&mut self, engine: &str) {
        if self.storage.set_search_engine(engine.to_string()) {
            self.update_data();
        }
    }

    pub fn toggle_adblock(&mut self) {
        let enabled = self.storage.toggle_adblock();
        self.adblock_enabled.set(enabled);
        let script = format!("window.__viimaAdblockSetEnabled({enabled});");
        for tab in &self.tabs {
            let _ = tab.webview.evaluate_script(&script);
        }
        self.update_data();
    }

    pub fn toggle_theme(&mut self) {
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
            let _ = tab.webview.set_theme(web_theme(dark));
            if Self::is_internal_page_url(&tab.url) {
                let _ = tab.webview.evaluate_script(&script);
            }
        }
        self.update_data();
    }

    pub fn on_page_started(&mut self, id: usize, url: String) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.on_page_started(&url);
            self.update_ui();
        }
    }

    pub fn on_page_finished(&mut self, id: usize, url: String) {
        let enabled = self.adblock_enabled.get();
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.on_page_finished(&url);
            if tab.init_adblock_enabled != enabled {
                let script = format!("window.__viimaAdblockSetEnabled({enabled});");
                let _ = tab.webview.evaluate_script(&script);
            }
            let title = tab.title.clone();
            self.storage.add_history_entry(title, url);
        } else {
            return;
        }
        self.update_ui();
        self.push_history_to_open_tabs();
    }

    pub fn on_title_changed(&mut self, id: usize, title: String) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.on_title_changed(&title);
            if id == self.active_tab_id {
                self.window.set_title(&format!("{} - Viima", tab.title));
            }
            self.update_ui();
        }
    }

    pub fn on_resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.window_size = new_size;

        let scale_factor = self.window.scale_factor();
        let logical_size = new_size.to_logical::<f64>(scale_factor);

        // Resize top bar to exactly 84px logical height
        let top_rect = Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(logical_size.width, TOP_BAR_LOGICAL_HEIGHT).into(),
        };
        let _ = self.ui_webview.set_bounds(top_rect);

        // Resize active tab webview to occupy the rest of the window
        let content_rect = self.content_rect();
        for tab in &self.tabs {
            if tab.id == self.active_tab_id {
                tab.set_bounds(content_rect);
            }
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

    pub fn close_active_tab(&mut self) {
        self.close_tab(self.active_tab_id);
    }

    pub fn focus_omnibox(&self) {
        let _ = self.ui_webview.focus();
        let _ = self
            .ui_webview
            .evaluate_script("window.focusOmnibox && window.focusOmnibox();");
    }

    /// Persist pending writes; call before exiting.
    pub fn flush(&mut self) {
        self.storage.flush();
    }

    pub fn toggle_auto_update(&mut self) {
        self.storage.toggle_auto_update();
        self.update_data();
    }

    pub fn set_update_url(&mut self, url: &str) {
        if self.storage.set_update_url(url.to_string()) {
            self.update_status.error = None;
            self.update_data();
        } else {
            self.update_status.state = "error";
            self.update_status.error = Some("Päivitys-URL:n täytyy alkaa https://".into());
            self.update_data();
        }
    }

    pub fn check_for_updates(&mut self, manual: bool) {
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

    pub fn on_update_check_finished(
        &mut self,
        manual: bool,
        result: Result<Option<UpdateInfo>, String>,
    ) {
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

    pub fn download_update(&mut self) {
        if matches!(self.update_status.state, "downloading" | "ready") {
            return;
        }
        let Some(info) = self.pending_update.clone() else {
            return;
        };
        if !self.update_status.can_self_update {
            self.update_status.state = "error";
            self.update_status.error = Some(
                "Automaattinen päivitys on käytettävissä vain asennetussa tai julkaistussa versiossa."
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

    pub fn on_update_download_finished(&mut self, result: Result<PathBuf, String>) {
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

    pub fn install_staged_update(&mut self) -> bool {
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

    fn sync_window_title(&self) {
        if let Some(tab) = self.tabs.iter().find(|t| t.id == self.active_tab_id) {
            self.window.set_title(&format!("{} - Viima", tab.title));
        }
    }

    fn internal_state_script(state: &InternalPageState<'_>) -> Option<String> {
        serde_json::to_string(state).ok().map(|json| {
            format!("if (window.onBrowserStateUpdate) {{ window.onBrowserStateUpdate({json}); }}")
        })
    }

    fn push_history_to_open_tabs(&self) {
        let has_history_tab = self
            .tabs
            .iter()
            .any(|t| Self::internal_page_kind(&t.url) == Some(InternalPage::History));
        if !has_history_tab {
            return;
        }
        let payload = InternalPageState {
            bookmarks: None,
            history: Some(&self.storage.data.history[..]),
            settings: Some(&self.storage.data.settings),
            update: None,
        };
        if let Some(script) = Self::internal_state_script(&payload) {
            for tab in &self.tabs {
                if Self::internal_page_kind(&tab.url) == Some(InternalPage::History) {
                    let _ = tab.webview.evaluate_script(&script);
                }
            }
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
                    is_loading: t.is_loading,
                    can_go_back: is_active && t.can_go_back(),
                    can_go_forward: is_active && t.can_go_forward(),
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
            storage_error: self.storage.error(),
        };

        if let Ok(json) = serde_json::to_string(&state) {
            let script = format!(
                "if (window.onBrowserStateUpdate) {{ window.onBrowserStateUpdate({json}); }}"
            );
            let _ = self.ui_webview.evaluate_script(&script);
        }

        if full {
            let mut history_script: Option<Option<String>> = None;
            let mut bookmarks_script: Option<Option<String>> = None;
            let mut settings_script: Option<Option<String>> = None;
            let mut newtab_script: Option<Option<String>> = None;

            for tab in &self.tabs {
                let script = match Self::internal_page_kind(&tab.url) {
                    Some(InternalPage::NewTab) => newtab_script.get_or_insert_with(|| {
                        Self::internal_state_script(&InternalPageState {
                            bookmarks: Some(&data.bookmarks[..]),
                            history: None,
                            settings: Some(&data.settings),
                            update: None,
                        })
                    }),
                    Some(InternalPage::History) => history_script.get_or_insert_with(|| {
                        Self::internal_state_script(&InternalPageState {
                            bookmarks: None,
                            history: Some(&data.history[..]),
                            settings: Some(&data.settings),
                            update: None,
                        })
                    }),
                    Some(InternalPage::Bookmarks) => bookmarks_script.get_or_insert_with(|| {
                        Self::internal_state_script(&InternalPageState {
                            bookmarks: Some(&data.bookmarks[..]),
                            history: None,
                            settings: Some(&data.settings),
                            update: None,
                        })
                    }),
                    Some(InternalPage::Settings) => settings_script.get_or_insert_with(|| {
                        Self::internal_state_script(&InternalPageState {
                            bookmarks: None,
                            history: None,
                            settings: Some(&data.settings),
                            update: Some(&self.update_status),
                        })
                    }),
                    None => continue,
                };
                if let Some(script) = script {
                    let _ = tab.webview.evaluate_script(script);
                }
            }
        }
    }

    // Windows serves browser:// pages as http://browser.<name>/ (intercepted by wry). Bare hosts
    // like "settings" are NOT trusted: they would go to real DNS and could be served by anyone.
    fn is_trusted_ipc_source(uri: &wry::http::Uri) -> bool {
        matches!(
            uri.host(),
            Some("browser.newtab" | "browser.history" | "browser.bookmarks" | "browser.settings")
        )
    }

    fn internal_page_kind(url: &str) -> Option<InternalPage> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?;
        match (parsed.scheme(), host) {
            ("browser", "newtab") | ("http", "browser.newtab") => Some(InternalPage::NewTab),
            ("browser", "history") | ("http", "browser.history") => Some(InternalPage::History),
            ("browser", "bookmarks") | ("http", "browser.bookmarks") => {
                Some(InternalPage::Bookmarks)
            }
            ("browser", "settings") | ("http", "browser.settings") => Some(InternalPage::Settings),
            _ => None,
        }
    }

    fn is_internal_page_url(url: &str) -> bool {
        Self::internal_page_kind(url).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_trust_is_exact() {
        let uri = |s: &str| s.parse::<wry::http::Uri>().unwrap();
        assert!(BrowserApp::is_trusted_ipc_source(&uri(
            "http://browser.settings/"
        )));
        // bare hosts would resolve via real DNS; lookalikes must not pass
        assert!(!BrowserApp::is_trusted_ipc_source(&uri("http://settings/")));
        assert!(!BrowserApp::is_trusted_ipc_source(&uri(
            "http://browser.settings.evil.com/"
        )));
        assert!(!BrowserApp::is_trusted_ipc_source(&uri(
            "https://evil.com/browser.settings"
        )));
    }
}
