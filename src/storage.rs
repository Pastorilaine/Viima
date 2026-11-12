use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
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

/// True for any `browser://` page or its WebView2 `http://browser.<name>/` mapping.
pub fn is_internal_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return url.starts_with("browser://");
    };
    let Some(host) = parsed.host_str() else {
        return parsed.scheme() == "browser";
    };
    matches!(
        (parsed.scheme(), host),
        ("browser", _)
            | (
                "http",
                "browser.newtab"
                    | "browser.history"
                    | "browser.bookmarks"
                    | "browser.settings"
                    | "browser.ui"
            )
    )
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub title: String,
    pub url: String,
    pub added_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub title: String,
    pub url: String,
    pub visited_at: u64,
    pub visit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub search_engine: String, // "duckduckgo", "google", "bing", "wikipedia"
    pub adblock_enabled: bool,
    pub home_page: String,
    pub dark_mode: bool,
    pub auto_update_enabled: bool,
    pub update_url: String,
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
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
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
    pub history: Vec<HistoryItem>,
    pub settings: BrowserSettings,
    pub session: Session,
}

impl Default for BrowserData {
    fn default() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let default_bookmarks = vec![
            Bookmark {
                id: "bm-1".to_string(),
                title: "DuckDuckGo".to_string(),
                url: "https://duckduckgo.com".to_string(),
                added_at: now,
            },
            Bookmark {
                id: "bm-2".to_string(),
                title: "Yle Uutiset".to_string(),
                url: "https://yle.fi/uutiset".to_string(),
                added_at: now,
            },
            Bookmark {
                id: "bm-3".to_string(),
                title: "GitHub".to_string(),
                url: "https://github.com".to_string(),
                added_at: now,
            },
            Bookmark {
                id: "bm-4".to_string(),
                title: "YouTube".to_string(),
                url: "https://www.youtube.com".to_string(),
                added_at: now,
            },
            Bookmark {
                id: "bm-5".to_string(),
                title: "Rust Lang".to_string(),
                url: "https://www.rust-lang.org".to_string(),
                added_at: now,
            },
            Bookmark {
                id: "bm-6".to_string(),
                title: "Sää (Ilmatieteen laitos)".to_string(),
                url: "https://www.ilmatieteenlaitos.fi/saa".to_string(),
                added_at: now,
            },
        ];

        Self {
            bookmarks: default_bookmarks,
            history: Vec::new(),
            settings: BrowserSettings::default(),
            session: Session::default(),
        }
    }
}

/// WebView2 profile folder (cookies, cache, logins) under %LOCALAPPDATA%.
pub fn webview_data_dir() -> Option<PathBuf> {
    ProjectDirs::from("fi", "IT-Veljekset", "Viima").map(|d| {
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

impl StorageManager {
    pub fn load_or_default() -> Self {
        let file_path = Self::resolve_storage_path();
        let mut last_error = None;
        let mut data = if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(data) => data,
                    Err(error) => {
                        let mut backup_path = file_path.with_extension("json.corrupt");
                        if backup_path.exists() {
                            let stamp = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            backup_path = file_path
                                .with_file_name(format!("browser_data-{stamp}.json.corrupt"));
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
            let initial = BrowserData::default();
            if let Err(error) = Self::save_to_path(&file_path, &initial) {
                last_error = Some(format!(
                    "Selaimen oletustietojen tallennus epäonnistui: {error}"
                ));
            }
            initial
        };

        if !matches!(
            data.settings.search_engine.as_str(),
            "google" | "duckduckgo" | "bing" | "wikipedia"
        ) {
            data.settings.search_engine = "google".to_string();
        }

        let write_error = Arc::new(Mutex::new(None));
        let (tx, handle) = spawn_writer(file_path, write_error.clone());
        Self {
            data,
            last_error,
            tx: Some(tx),
            writer: Some(handle),
            write_error,
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
        if let Some(proj_dirs) = ProjectDirs::from("fi", "IT-Veljekset", "Viima") {
            let dir = proj_dirs.config_dir();
            let _ = fs::create_dir_all(dir);
            let target = dir.join("browser_data.json");
            if !target.exists()
                && let Some(old) = ProjectDirs::from("fi", "IT-Veljekset", "OmaSelain")
            {
                let old_target = old.config_dir().join("browser_data.json");
                if old_target.exists() {
                    let _ = fs::copy(&old_target, &target);
                }
            }
            target
        } else {
            PathBuf::from("browser_data.json")
        }
    }

    fn save_to_path(path: &PathBuf, data: &BrowserData) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let serialized = serde_json::to_string_pretty(data).map_err(std::io::Error::other)?;
        fs::write(path, serialized)
    }

    pub fn save(&mut self) {
        let Some(tx) = &self.tx else { return };
        let _ = tx.send(self.data.clone());
    }

    pub fn add_history_entry(&mut self, title: String, url: String) {
        if url.is_empty() || is_internal_url(&url) || url == "about:blank" {
            return;
        }

        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let now = elapsed.as_secs();

        // Check if last visited is the exact same URL within 5 seconds to avoid duplicate spam
        if let Some(first) = self.data.history.first_mut()
            && urls_match(&first.url, &url)
            && (now.saturating_sub(first.visited_at)) < 5
        {
            if !title.is_empty() && first.title != title {
                first.title = title;
            }
            first.visited_at = now;
            first.visit_count += 1;
            self.save();
            return;
        }

        let item = HistoryItem {
            id: unique_id("hist", elapsed.as_millis()),
            title: if title.trim().is_empty() {
                url.clone()
            } else {
                title
            },
            url,
            visited_at: now,
            visit_count: 1,
        };

        self.data.history.insert(0, item);
        // Limit history to 500 items to keep fast
        if self.data.history.len() > 500 {
            self.data.history.truncate(500);
        }
        self.save();
    }

    /// Inline omnibox completion: returns the text to append to `typed`, taken from the most
    /// visited matching bookmark/history URL (shown without scheme and `www.`).
    /// Typing just a host completes to the host; once a `/` is typed, full paths are offered.
    pub fn autocomplete(&self, typed: &str) -> Option<String> {
        if typed.len() < 2 || typed.chars().any(char::is_whitespace) {
            return None;
        }
        let want_path = typed.contains('/');
        let mut scores: std::collections::HashMap<String, u32> = Default::default();
        let candidates = self
            .data
            .bookmarks
            .iter()
            .map(|b| (b.url.as_str(), 5))
            .chain(
                self.data
                    .history
                    .iter()
                    .map(|h| (h.url.as_str(), h.visit_count)),
            );
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

    /// Page titles usually arrive after the load-finished event that created the history entry.
    pub fn update_history_title(&mut self, url: &str, title: &str) {
        let title = title.trim();
        if let Some(first) = self.data.history.first_mut()
            && !title.is_empty()
            && first.title != title
            && urls_match(&first.url, url)
        {
            first.title = title.to_string();
            self.save();
        }
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
            let elapsed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            let now = elapsed.as_secs();

            self.data.bookmarks.push(Bookmark {
                id: unique_id("bm", elapsed.as_millis()),
                title: if title.trim().is_empty() {
                    url.clone()
                } else {
                    title
                },
                url,
                added_at: now,
            });
            self.save();
            true
        }
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        let before = self.data.bookmarks.len();
        self.data.bookmarks.retain(|b| !urls_match(&b.url, url));
        if self.data.bookmarks.len() != before {
            self.save();
        }
    }

    #[allow(dead_code)]
    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.data.bookmarks.iter().any(|b| urls_match(&b.url, url))
    }

    pub fn clear_history(&mut self) {
        if !self.data.history.is_empty() {
            self.data.history.clear();
            self.save();
        }
    }

    pub fn delete_history_item(&mut self, url: &str) {
        let before = self.data.history.len();
        self.data.history.retain(|h| !urls_match(&h.url, url));
        if self.data.history.len() != before {
            self.save();
        }
    }

    pub fn set_search_engine(&mut self, engine: String) -> bool {
        if !matches!(
            engine.as_str(),
            "google" | "duckduckgo" | "bing" | "wikipedia"
        ) {
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
        // tx: None -> save() is a no-op, no disk access in tests
        let mut storage = StorageManager {
            data: BrowserData::default(),
            last_error: None,
            tx: None,
            writer: None,
            write_error: Default::default(),
        };

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
    fn test_session_roundtrip_and_old_files_load() {
        // Files written before sessions existed must still load.
        let old: BrowserData = serde_json::from_str(r#"{"bookmarks":[],"history":[]}"#).unwrap();
        assert!(old.session.tabs.is_empty());

        let data = BrowserData {
            session: Session {
                tabs: vec![SessionTab {
                    url: "https://yle.fi/".into(),
                    title: "Yle".into(),
                }],
                active_index: 0,
                maximized: true,
                window_size: Some((1280.0, 820.0)),
            },
            ..BrowserData::default()
        };
        let back: BrowserData =
            serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
        assert_eq!(back.session, data.session);
    }

    #[test]
    fn test_autocomplete() {
        let mut storage = StorageManager {
            data: BrowserData::default(),
            last_error: None,
            tx: None,
            writer: None,
            write_error: Default::default(),
        };
        // Default bookmarks include https://yle.fi/uutiset and https://www.youtube.com
        assert_eq!(storage.autocomplete("yl").as_deref(), Some("e.fi"));
        assert_eq!(storage.autocomplete("YO").as_deref(), Some("utube.com"));
        assert_eq!(storage.autocomplete("yle.fi/").as_deref(), Some("uutiset"));
        assert_eq!(storage.autocomplete("y"), None); // too short
        assert_eq!(storage.autocomplete("yle.fi"), None); // already complete
        assert_eq!(storage.autocomplete("rust lang"), None); // search query

        // More visits outrank a bookmark on the same prefix.
        for _ in 0..3 {
            storage.data.history.push(HistoryItem {
                id: "h".into(),
                title: "Ylioppilastutkinto".into(),
                url: "https://ylioppilastutkinto.fi/".into(),
                visited_at: 0,
                visit_count: 4,
            });
        }
        assert_eq!(
            storage.autocomplete("yl").as_deref(),
            Some("ioppilastutkinto.fi")
        );
        assert_eq!(storage.autocomplete("yle").as_deref(), Some(".fi"));
    }

    #[test]
    fn test_history_title_updates_latest_entry() {
        let mut storage = StorageManager {
            data: BrowserData::default(),
            last_error: None,
            tx: None,
            writer: None,
            write_error: Default::default(),
        };
        storage.add_history_entry("Vanha otsikko".into(), "https://yle.fi/".into());
        storage.update_history_title("https://yle.fi", "Yle");
        assert_eq!(storage.data.history[0].title, "Yle");
        storage.update_history_title("https://other.fi/", "Muu");
        assert_eq!(storage.data.history[0].title, "Yle");
    }

    #[test]
    fn test_internal_urls_excluded_from_history_and_bookmarks() {
        let mut storage = StorageManager {
            data: BrowserData::default(),
            last_error: None,
            tx: None,
            writer: None,
            write_error: Default::default(),
        };

        storage.add_history_entry("Uusi välilehti".into(), "browser://newtab".into());
        storage.add_history_entry("Uusi välilehti".into(), "http://browser.newtab/".into());
        storage.add_history_entry("Asetukset".into(), "http://browser.settings/".into());
        assert!(storage.data.history.is_empty());

        assert!(!storage.toggle_bookmark("Asetukset".into(), "http://browser.settings/".into()));
        assert!(!storage.is_bookmarked("http://browser.settings/"));
    }
}
