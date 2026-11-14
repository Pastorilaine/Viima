use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID_SEQ: AtomicU64 = AtomicU64::new(1);

fn unique_id(prefix: &str, timestamp_ms: u128) -> String {
    let seq = NEXT_ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{timestamp_ms}-{seq}")
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Internal page names served as `browser://<name>` (WebView2: `http://browser.<name>/`).
pub const INTERNAL_PAGES: &[&str] = &[
    "newtab",
    "history",
    "bookmarks",
    "settings",
    "downloads",
    "insecure",
];

/// True for any `browser://` page or its WebView2 `http://browser.<name>/` mapping.
pub fn is_internal_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return url.starts_with("browser://");
    };
    let Some(host) = parsed.host_str() else {
        return parsed.scheme() == "browser";
    };
    match parsed.scheme() {
        "browser" => true,
        "http" => host
            .strip_prefix("browser.")
            .is_some_and(|name| name == "ui" || INTERNAL_PAGES.contains(&name)),
        _ => false,
    }
}

/// Compares two URLs canonically when parseable (e.g. `https://duckduckgo.com` == `https://duckduckgo.com/`).
pub fn urls_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (url::Url::parse(a), url::Url::parse(b)) {
        (Ok(ua), Ok(ub)) => ua == ub,
        _ => false,
    }
}

/// Lower-case host without `www.`, used as the key for per-site settings.
pub fn site_key(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || is_internal_url(url) {
        return None;
    }
    let host = parsed.host_str()?;
    Some(host.strip_prefix("www.").unwrap_or(host).to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub title: String,
    pub url: String,
    pub added_at: u64,
    /// "" = bookmarks bar root; otherwise a folder name (one level).
    #[serde(default)]
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub title: String,
    pub url: String,
    pub visited_at: u64,
    pub visit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DownloadItem {
    pub id: u64,
    pub url: String,
    pub path: String,
    pub file_name: String,
    pub received: i64,
    /// -1 when the server didn't say.
    pub total: i64,
    /// "in_progress" | "completed" | "interrupted"
    pub state: String,
    pub started_at: u64,
}

impl Default for DownloadItem {
    fn default() -> Self {
        Self {
            id: 0,
            url: String::new(),
            path: String::new(),
            file_name: String::new(),
            received: 0,
            total: -1,
            state: "in_progress".into(),
            started_at: 0,
        }
    }
}

pub const SEARCH_ENGINES: &[&str] = &["google", "duckduckgo", "bing", "wikipedia"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub search_engine: String, // "duckduckgo", "google", "bing", "wikipedia"
    pub adblock_enabled: bool,
    pub home_page: String,
    pub dark_mode: bool,
    pub auto_update_enabled: bool,
    pub update_url: String,
    /// "fi" | "en"
    pub language: String,
    pub show_bookmarks_bar: bool,
    pub https_only: bool,
    pub hide_cookie_banners: bool,
    pub devtools_enabled: bool,
    pub sleeping_tabs: bool,
    /// Download EasyList & co. weekly on top of the built-in block list.
    pub adblock_lists: bool,
    /// Unix time of the last successful filter list download (0 = never).
    pub adblock_lists_updated: u64,
    pub adblock_rule_count: usize,
    /// Sites (see `site_key`) where ad blocking is turned off.
    pub adblock_allowlist: Vec<String>,
    /// Missing in files written before 0.4.3 (= 0); see `migrate`.
    #[serde(default)]
    pub settings_version: u32,
}

const SETTINGS_VERSION: u32 = 1;

impl BrowserSettings {
    /// One-time fixes for settings files written by older versions.
    fn migrate(&mut self) {
        if self.settings_version < 1 {
            // DevTools used to be on by default: a scammer could talk a user into pasting
            // code into the console of a privileged internal page. Off unless re-enabled.
            self.devtools_enabled = false;
        }
        self.settings_version = SETTINGS_VERSION;
    }
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            search_engine: "google".to_string(),
            adblock_enabled: true,
            home_page: "browser://newtab".to_string(),
            dark_mode: true,
            auto_update_enabled: true,
            update_url: crate::updater::default_update_url().to_string(),
            language: "fi".to_string(),
            show_bookmarks_bar: false,
            https_only: false,
            hide_cookie_banners: true,
            devtools_enabled: false,
            sleeping_tabs: true,
            adblock_lists: true,
            adblock_lists_updated: 0,
            adblock_rule_count: 0,
            adblock_allowlist: Vec::new(),
            settings_version: SETTINGS_VERSION,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
}

/// Open tabs + window state, restored on next launch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    pub active_index: usize,
    pub maximized: bool,
    /// Logical (width, height) of the last non-maximized window.
    pub window_size: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserData {
    pub bookmarks: Vec<Bookmark>,
    /// Legacy (≤ 0.3.x) history; imported into SQLite on first start, then dropped.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryItem>,
    pub settings: BrowserSettings,
    pub session: Session,
    /// Per-site page zoom (site_key -> factor), only non-100% values.
    pub site_zoom: HashMap<String, f64>,
    pub downloads: Vec<DownloadItem>,
}

impl Default for BrowserData {
    fn default() -> Self {
        let now = now_secs();
        let bm = |id: &str, title: &str, url: &str| Bookmark {
            id: id.to_string(),
            title: title.to_string(),
            url: url.to_string(),
            added_at: now,
            folder: String::new(),
        };
        Self {
            bookmarks: vec![
                bm("bm-1", "DuckDuckGo", "https://duckduckgo.com"),
                bm("bm-2", "Yle Uutiset", "https://yle.fi/uutiset"),
                bm("bm-3", "GitHub", "https://github.com"),
                bm("bm-4", "YouTube", "https://www.youtube.com"),
                bm("bm-5", "Rust Lang", "https://www.rust-lang.org"),
                bm(
                    "bm-6",
                    "Sää (Ilmatieteen laitos)",
                    "https://www.ilmatieteenlaitos.fi/saa",
                ),
            ],
            history: Vec::new(),
            settings: BrowserSettings::default(),
            session: Session::default(),
            site_zoom: HashMap::new(),
            downloads: Vec::new(),
        }
    }
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("fi", "IT-Veljekset", "Viima")
}

/// WebView2 profile folder (cookies, cache, logins) under %LOCALAPPDATA%.
pub fn webview_data_dir() -> Option<PathBuf> {
    project_dirs().map(|d| {
        let new_dir = d.data_local_dir().join("WebView2");
        if !new_dir.exists()
            && let Some(old) = ProjectDirs::from("fi", "IT-Veljekset", "OmaSelain")
        {
            let old_dir = old.data_local_dir().join("WebView2");
            if old_dir.exists() {
                let _ = fs::create_dir_all(d.data_local_dir());
                let _ = fs::rename(&old_dir, &new_dir);
            }
        }
        new_dir
    })
}

/// Throwaway WebView2 profile for a private window; stale ones from crashed sessions are removed.
pub fn private_data_dir() -> PathBuf {
    let base = std::env::temp_dir().join("Viima-Private");
    if let Ok(entries) = fs::read_dir(&base) {
        for e in entries.flatten() {
            // A folder still used by another private window can't be renamed (open files).
            let stale = e.path().with_extension("stale");
            if fs::rename(e.path(), &stale).is_ok() {
                let _ = fs::remove_dir_all(stale);
            }
        }
    }
    base.join(format!("{}-{}", std::process::id(), now_secs()))
}

/// Folder for app data files (browser_data.json, history.db, filter lists).
pub fn config_dir() -> PathBuf {
    match project_dirs() {
        Some(d) => {
            let dir = d.config_dir().to_path_buf();
            let _ = fs::create_dir_all(&dir);
            dir
        }
        None => PathBuf::from("."),
    }
}

pub struct StorageManager {
    pub data: BrowserData,
    pub last_error: Option<String>,
    // Writes and JSON serialization go to a background thread so disk/CPU stalls never block the UI thread.
    tx: Option<Sender<BrowserData>>,
    writer: Option<JoinHandle<()>>,
    write_error: Arc<Mutex<Option<String>>>,
}

impl Drop for StorageManager {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Coalescing writer: only the newest snapshot is serialized and written; tmp + rename keeps the file intact.
fn spawn_writer(
    path: PathBuf,
    write_error: Arc<Mutex<Option<String>>>,
) -> (Sender<BrowserData>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<BrowserData>();
    let handle = thread::spawn(move || {
        while let Ok(mut snapshot) = rx.recv() {
            while let Ok(newer) = rx.try_recv() {
                snapshot = newer;
            }
            let tmp = path.with_extension("json.tmp");
            // Compact JSON: this runs on every navigation, pretty-printing roughly doubles the work.
            let result = serde_json::to_string(&snapshot)
                .map_err(std::io::Error::other)
                .and_then(|json| fs::write(&tmp, json).and_then(|_| fs::rename(&tmp, &path)));
            if let Ok(mut slot) = write_error.lock() {
                *slot = result
                    .err()
                    .map(|e| format!("Selaimen tietojen tallennus epäonnistui: {e}"));
            }
        }
    });
    (tx, handle)
}

const MAX_DOWNLOADS: usize = 100;

impl StorageManager {
    /// `read_only` (private window): everything stays in memory; the normal window owns the file.
    pub fn load_or_default(read_only: bool) -> Self {
        let file_path = Self::resolve_storage_path();
        let mut last_error = None;
        let mut data = if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(data) => data,
                    Err(error) if read_only => {
                        last_error = Some(format!("Asetustiedosto oli virheellinen ({error})."));
                        BrowserData::default()
                    }
                    Err(error) => {
                        let mut backup_path = file_path.with_extension("json.corrupt");
                        if backup_path.exists() {
                            backup_path = file_path.with_file_name(format!(
                                "browser_data-{}.json.corrupt",
                                now_secs()
                            ));
                        }
                        if let Err(rename_error) = fs::rename(&file_path, &backup_path) {
                            last_error = Some(format!(
                                "Asetustiedosto oli virheellinen, eikä sitä voitu varmuuskopioida: {rename_error}"
                            ));
                        } else {
                            last_error = Some(format!(
                                "Asetustiedosto oli virheellinen ({error}). Alkuperäinen säilytettiin tiedostossa {}",
                                backup_path.display()
                            ));
                        }
                        BrowserData::default()
                    }
                },
                Err(error) => {
                    last_error = Some(format!("Asetustiedoston lukeminen epäonnistui: {error}"));
                    BrowserData::default()
                }
            }
        } else {
            BrowserData::default()
        };

        if !SEARCH_ENGINES.contains(&data.settings.search_engine.as_str()) {
            data.settings.search_engine = "google".to_string();
        }
        if !matches!(data.settings.language.as_str(), "fi" | "en") {
            data.settings.language = "fi".to_string();
        }
        data.settings.migrate();
        // A download can't survive a restart: anything still running was interrupted.
        for d in &mut data.downloads {
            if d.state == "in_progress" {
                d.state = "interrupted".into();
            }
        }

        let write_error = Arc::new(Mutex::new(None));
        let (tx, writer) = if read_only {
            (None, None)
        } else {
            let (tx, handle) = spawn_writer(file_path, write_error.clone());
            (Some(tx), Some(handle))
        };
        let storage = Self {
            data,
            last_error,
            tx,
            writer,
            write_error,
        };
        storage.save();
        storage
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            data: BrowserData::default(),
            last_error: None,
            tx: None,
            writer: None,
            write_error: Default::default(),
        }
    }

    /// Current error to show in the UI: a failed background write wins over the load error.
    pub fn error(&self) -> Option<String> {
        self.write_error
            .lock()
            .ok()
            .and_then(|e| e.clone())
            .or_else(|| self.last_error.clone())
    }

    /// Wait for pending writes to hit the disk. Call before exiting.
    pub fn flush(&mut self) {
        drop(self.tx.take());
        if let Some(handle) = self.writer.take() {
            let _ = handle.join();
        }
    }

    fn resolve_storage_path() -> PathBuf {
        let target = config_dir().join("browser_data.json");
        if !target.exists()
            && let Some(old) = ProjectDirs::from("fi", "IT-Veljekset", "OmaSelain")
        {
            let old_target = old.config_dir().join("browser_data.json");
            if old_target.exists() {
                let _ = fs::copy(&old_target, &target);
            }
        }
        target
    }

    pub fn save(&self) {
        let Some(tx) = &self.tx else { return };
        let _ = tx.send(self.data.clone());
    }

    /// Inline omnibox completion: returns the text to append to `typed`, taken from the most
    /// visited matching bookmark/history URL (shown without scheme and `www.`).
    /// Typing just a host completes to the host; once a `/` is typed, full paths are offered.
    pub fn autocomplete(&self, typed: &str, history: &[(String, u32)]) -> Option<String> {
        if typed.len() < 2 || typed.chars().any(char::is_whitespace) {
            return None;
        }
        let want_path = typed.contains('/');
        let mut scores: HashMap<String, u32> = Default::default();
        let candidates = self
            .data
            .bookmarks
            .iter()
            .map(|b| (b.url.as_str(), 5))
            .chain(history.iter().map(|(u, c)| (u.as_str(), *c)));
        for (url, weight) in candidates {
            let Ok(parsed) = url::Url::parse(url) else {
                continue;
            };
            if !matches!(parsed.scheme(), "http" | "https") {
                continue;
            }
            let Some(host) = parsed.host_str() else {
                continue;
            };
            let host = host.strip_prefix("www.").unwrap_or(host);
            let cand = if want_path {
                let mut s = format!("{host}{}", parsed.path());
                if let Some(q) = parsed.query() {
                    s.push('?');
                    s.push_str(q);
                }
                s.trim_end_matches('/').to_string()
            } else {
                host.to_string()
            };
            if cand.len() > typed.len()
                && cand.is_char_boundary(typed.len())
                && cand[..typed.len()].eq_ignore_ascii_case(typed)
            {
                *scores.entry(cand).or_default() += weight;
            }
        }
        scores
            .into_iter()
            // Highest score wins; on a tie the shorter (more general) URL.
            .max_by(|(a, sa), (b, sb)| sa.cmp(sb).then(b.len().cmp(&a.len())))
            .map(|(cand, _)| cand[typed.len()..].to_string())
    }

    pub fn set_session(&mut self, session: Session) {
        if self.data.session != session {
            self.data.session = session;
            self.save();
        }
    }

    pub fn toggle_bookmark(&mut self, title: String, url: String) -> bool {
        if url.is_empty() || is_internal_url(&url) || url == "about:blank" {
            return false;
        }

        if let Some(pos) = self
            .data
            .bookmarks
            .iter()
            .position(|b| urls_match(&b.url, &url))
        {
            self.data.bookmarks.remove(pos);
            self.save();
            false
        } else {
            self.push_bookmark(title, url, String::new());
            self.save();
            true
        }
    }

    fn push_bookmark(&mut self, title: String, url: String, folder: String) {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        self.data.bookmarks.push(Bookmark {
            id: unique_id("bm", elapsed.as_millis()),
            title: if title.trim().is_empty() {
                url.clone()
            } else {
                title
            },
            url,
            added_at: elapsed.as_secs(),
            folder,
        });
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        let before = self.data.bookmarks.len();
        self.data.bookmarks.retain(|b| !urls_match(&b.url, url));
        if self.data.bookmarks.len() != before {
            self.save();
        }
    }

    pub fn set_bookmark_folder(&mut self, url: &str, folder: &str) {
        let folder: String = folder.trim().chars().take(60).collect();
        if let Some(b) = self
            .data
            .bookmarks
            .iter_mut()
            .find(|b| urls_match(&b.url, url))
            && b.folder != folder
        {
            b.folder = folder;
            self.save();
        }
    }

    /// Adds http(s) bookmarks that aren't saved yet; returns how many were added.
    pub fn import_bookmarks(&mut self, items: &[(String, String, String)]) -> usize {
        let mut added = 0;
        for (title, url, folder) in items.iter().take(5000) {
            let ok_scheme = url.starts_with("https://") || url.starts_with("http://");
            if !ok_scheme || url.len() > 4096 || self.is_bookmarked(url) {
                continue;
            }
            let title: String = title.trim().chars().take(300).collect();
            let folder: String = folder.trim().chars().take(60).collect();
            self.push_bookmark(title, url.clone(), folder);
            added += 1;
        }
        if added > 0 {
            self.save();
        }
        added
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.data.bookmarks.iter().any(|b| urls_match(&b.url, url))
    }

    pub fn set_search_engine(&mut self, engine: String) -> bool {
        if !SEARCH_ENGINES.contains(&engine.as_str()) {
            return false;
        }
        self.data.settings.search_engine = engine;
        self.save();
        true
    }

    pub fn toggle_dark_mode(&mut self) -> bool {
        self.data.settings.dark_mode = !self.data.settings.dark_mode;
        self.save();
        self.data.settings.dark_mode
    }

    pub fn toggle_adblock(&mut self) -> bool {
        self.data.settings.adblock_enabled = !self.data.settings.adblock_enabled;
        self.save();
        self.data.settings.adblock_enabled
    }

    pub fn toggle_auto_update(&mut self) -> bool {
        self.data.settings.auto_update_enabled = !self.data.settings.auto_update_enabled;
        self.save();
        self.data.settings.auto_update_enabled
    }

    pub fn set_update_url(&mut self, url: String) -> bool {
        let trimmed = url.trim();
        let next = if trimmed.is_empty() {
            crate::updater::default_update_url().to_string()
        } else if trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            return false;
        };
        self.data.settings.update_url = next;
        self.save();
        true
    }

    /// Generic setter for the simple settings the settings page can change. Unknown keys and
    /// wrong value types are ignored (the page is trusted, but keep the surface explicit).
    pub fn set_setting(&mut self, key: &str, value: &serde_json::Value) -> bool {
        let s = &mut self.data.settings;
        let flag = value.as_bool();
        let changed = match (key, flag) {
            ("show_bookmarks_bar", Some(v)) => std::mem::replace(&mut s.show_bookmarks_bar, v) != v,
            ("https_only", Some(v)) => std::mem::replace(&mut s.https_only, v) != v,
            ("hide_cookie_banners", Some(v)) => {
                std::mem::replace(&mut s.hide_cookie_banners, v) != v
            }
            ("devtools_enabled", Some(v)) => std::mem::replace(&mut s.devtools_enabled, v) != v,
            ("sleeping_tabs", Some(v)) => std::mem::replace(&mut s.sleeping_tabs, v) != v,
            ("adblock_lists", Some(v)) => std::mem::replace(&mut s.adblock_lists, v) != v,
            ("language", None) => match value.as_str() {
                Some(l @ ("fi" | "en")) if s.language != l => {
                    s.language = l.to_string();
                    true
                }
                _ => false,
            },
            _ => false,
        };
        if changed {
            self.save();
        }
        changed
    }

    /// Toggles ad blocking for one site; returns true if blocking is now OFF there.
    pub fn toggle_adblock_allow(&mut self, site: &str) -> bool {
        let list = &mut self.data.settings.adblock_allowlist;
        let allowed = if let Some(pos) = list.iter().position(|s| s == site) {
            list.remove(pos);
            false
        } else {
            list.push(site.to_string());
            true
        };
        self.save();
        allowed
    }

    pub fn is_adblock_allowed(&self, site: &str) -> bool {
        self.data
            .settings
            .adblock_allowlist
            .iter()
            .any(|s| s == site)
    }

    pub fn set_site_zoom(&mut self, site: &str, factor: f64) {
        let factor = (factor * 100.0).round() / 100.0;
        let changed = if (factor - 1.0).abs() < 0.001 {
            self.data.site_zoom.remove(site).is_some()
        } else {
            self.data.site_zoom.insert(site.to_string(), factor) != Some(factor)
        };
        if changed {
            self.save();
        }
    }

    pub fn site_zoom(&self, site: &str) -> f64 {
        self.data.site_zoom.get(site).copied().unwrap_or(1.0)
    }

    /// Progress ticks only update memory; the file is written when the state changes.
    pub fn upsert_download(&mut self, item: DownloadItem) {
        let state_changed = match self.data.downloads.iter_mut().find(|d| d.id == item.id) {
            Some(existing) => {
                let changed = existing.state != item.state;
                *existing = item;
                changed
            }
            None => {
                self.data.downloads.insert(0, item);
                self.data.downloads.truncate(MAX_DOWNLOADS);
                true
            }
        };
        if state_changed {
            self.save();
        }
    }

    pub fn remove_download(&mut self, id: u64) {
        self.data.downloads.retain(|d| d.id != id);
        self.save();
    }

    pub fn clear_downloads(&mut self) {
        self.data.downloads.retain(|d| d.state == "in_progress");
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_defaults() {
        let data = BrowserData::default();
        assert!(!data.bookmarks.is_empty());
        assert_eq!(data.settings.search_engine, "google");
        assert!(data.settings.adblock_enabled);
        assert_eq!(data.settings.language, "fi");
    }

    #[test]
    fn test_writer_persists_latest_on_flush() {
        let path = std::env::temp_dir().join("test_viima_writer.json");
        let (tx, handle) = spawn_writer(path.clone(), Default::default());
        let mut old = BrowserData::default();
        old.settings.search_engine = "bing".into();
        let mut latest = BrowserData::default();
        latest.settings.search_engine = "duckduckgo".into();
        tx.send(old).unwrap();
        tx.send(latest).unwrap();
        drop(tx);
        handle.join().unwrap();
        let written: BrowserData =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written.settings.search_engine, "duckduckgo");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_toggle_bookmark() {
        let mut storage = StorageManager::in_memory();
        let url = "https://test-site.fi";
        let added = storage.toggle_bookmark("Test Site".into(), url.into());
        assert!(added);
        assert!(storage.is_bookmarked(url));
        // Canonical trailing-slash match (WebView2 adds `/` to root URLs)
        assert!(storage.is_bookmarked("https://test-site.fi/"));

        let removed = storage.toggle_bookmark("Test Site".into(), "https://test-site.fi/".into());
        assert!(!removed);
        assert!(!storage.is_bookmarked(url));
    }

    #[test]
    fn test_import_bookmarks_and_folders() {
        let mut storage = StorageManager::in_memory();
        let items = vec![
            ("A".into(), "https://a.fi/".into(), "Työ".into()),
            ("Dup".into(), "https://github.com/".into(), "".into()),
            ("Bad".into(), "javascript:alert(1)".into(), "".into()),
        ];
        assert_eq!(storage.import_bookmarks(&items), 1);
        storage.set_bookmark_folder("https://a.fi", "Uutiset");
        let a = storage.data.bookmarks.iter().find(|b| b.url == "https://a.fi/").unwrap();
        assert_eq!(a.folder, "Uutiset");
    }

    #[test]
    fn test_session_roundtrip_and_old_files_load() {
        // Files written before sessions/pinning/folders existed must still load.
        let old: BrowserData = serde_json::from_str(
            r#"{"bookmarks":[{"id":"x","title":"t","url":"https://x.fi","added_at":1}],"history":[]}"#,
        )
        .unwrap();
        assert!(old.session.tabs.is_empty());
        assert_eq!(old.bookmarks[0].folder, "");

        let data = BrowserData {
            session: Session {
                tabs: vec![SessionTab {
                    url: "https://yle.fi/".into(),
                    title: "Yle".into(),
                    pinned: true,
                }],
                active_index: 0,
                maximized: true,
                window_size: Some((1280.0, 820.0)),
            },
            ..BrowserData::default()
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(!json.contains("\"history\""), "empty legacy history is not written");
        let back: BrowserData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session, data.session);
    }

    #[test]
    fn test_settings_migration_turns_devtools_off_once() {
        let mut old: BrowserSettings =
            serde_json::from_str(r#"{"devtools_enabled":true}"#).unwrap();
        assert_eq!(old.settings_version, 0);
        old.migrate();
        assert!(!old.devtools_enabled);
        // Re-enabled by the user after migrating: stays on.
        old.devtools_enabled = true;
        let mut again: BrowserSettings =
            serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
        again.migrate();
        assert!(again.devtools_enabled);
        assert!(!BrowserSettings::default().devtools_enabled);
    }

    #[test]
    fn test_autocomplete() {
        let storage = StorageManager::in_memory();
        // Default bookmarks include https://yle.fi/uutiset and https://www.youtube.com
        assert_eq!(storage.autocomplete("yl", &[]).as_deref(), Some("e.fi"));
        assert_eq!(storage.autocomplete("YO", &[]).as_deref(), Some("utube.com"));
        assert_eq!(storage.autocomplete("yle.fi/", &[]).as_deref(), Some("uutiset"));
        assert_eq!(storage.autocomplete("y", &[]), None); // too short
        assert_eq!(storage.autocomplete("yle.fi", &[]), None); // already complete
        assert_eq!(storage.autocomplete("rust lang", &[]), None); // search query

        // More visits outrank a bookmark on the same prefix.
        let history = vec![("https://ylioppilastutkinto.fi/".to_string(), 12)];
        assert_eq!(
            storage.autocomplete("yl", &history).as_deref(),
            Some("ioppilastutkinto.fi")
        );
        assert_eq!(storage.autocomplete("yle", &history).as_deref(), Some(".fi"));
    }

    #[test]
    fn test_settings_and_site_prefs() {
        let mut storage = StorageManager::in_memory();
        assert!(storage.set_setting("https_only", &serde_json::json!(true)));
        assert!(!storage.set_setting("https_only", &serde_json::json!(true)));
        assert!(!storage.set_setting("update_url", &serde_json::json!("http://evil")));
        assert!(storage.set_setting("language", &serde_json::json!("en")));
        assert!(!storage.set_setting("language", &serde_json::json!("sv")));

        assert_eq!(site_key("https://www.Yle.fi/uutiset").as_deref(), Some("yle.fi"));
        assert_eq!(site_key("http://browser.settings/"), None);
        assert!(storage.toggle_adblock_allow("yle.fi"));
        assert!(storage.is_adblock_allowed("yle.fi"));
        assert!(!storage.toggle_adblock_allow("yle.fi"));

        storage.set_site_zoom("yle.fi", 1.25);
        assert_eq!(storage.site_zoom("yle.fi"), 1.25);
        storage.set_site_zoom("yle.fi", 1.0);
        assert!(storage.data.site_zoom.is_empty());
    }

    #[test]
    fn test_internal_urls() {
        assert!(is_internal_url("browser://newtab"));
        assert!(is_internal_url("http://browser.downloads/"));
        assert!(is_internal_url("http://browser.ui/index.html"));
        assert!(!is_internal_url("http://browser.evil/"));
        assert!(!is_internal_url("https://browser.settings/"));
        let mut storage = StorageManager::in_memory();
        assert!(!storage.toggle_bookmark("Asetukset".into(), "http://browser.settings/".into()));
    }
}
