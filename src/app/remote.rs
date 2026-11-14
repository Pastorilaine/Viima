//! Network-backed extras for the chrome and internal pages: site icons and widget data.
use super::*;
use crate::{favicon, widgets};
use std::collections::HashMap;

const WIDGET_CACHE_TTL: Duration = Duration::from_secs(300);
const FETCH_THREADS: usize = 4;

/// Lowercase host of an http(s) address: the key icons are cached under.
pub(super) fn icon_host(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    matches!(u.scheme(), "http" | "https")
        .then(|| u.host_str().map(str::to_ascii_lowercase))
        .flatten()
}

pub(super) type WidgetCache = HashMap<String, (Instant, String)>;

impl BrowserApp {
    /// Pages ask for the icons of the hosts they show: cached ones are pushed straight away,
    /// the rest are fetched and pushed when they arrive.
    pub(super) fn request_favicons(&mut self, hosts: Vec<String>) {
        let known: Vec<(String, String)> = hosts
            .iter()
            .filter_map(|h| self.favicons.get(h).map(|d| (h.clone(), d.clone())))
            .collect();
        self.push_favicons(&known);
        let jobs = self
            .favicons
            .claim_missing(&hosts)
            .into_iter()
            .map(|h| (h, None))
            .collect();
        self.spawn_favicon_fetch(jobs);
    }

    /// A tab reported its icon address: remember the icon for its site if we have none yet.
    pub(super) fn learn_tab_favicon(&mut self, id: usize, icon_url: &str) {
        let Some(host) = self.tabs.iter().find(|t| t.id == id).and_then(|t| icon_host(&t.url)) else {
            return;
        };
        // The page's own address is the best source: try it even if a guess for this host failed.
        if crate::favicon::valid_host(&host) && self.favicons.get(&host).is_none() {
            self.spawn_favicon_fetch(vec![(host, Some(icon_url.to_string()))]);
        }
    }

    /// (host, exact icon address if the page gave one). Falls back to looking at the site itself.
    fn spawn_favicon_fetch(&self, jobs: Vec<(String, Option<String>)>) {
        if jobs.is_empty() {
            return;
        }
        let per_thread = jobs.len().div_ceil(FETCH_THREADS);
        for chunk in jobs.chunks(per_thread) {
            let chunk = chunk.to_vec();
            let proxy = self.proxy.clone();
            thread::spawn(move || {
                for (host, url) in chunk {
                    let icon = url
                        .as_deref()
                        .and_then(favicon::fetch_icon)
                        .or_else(|| favicon::fetch_for_host(&host));
                    let _ = proxy.send_event(AppEvent::FaviconsReady(vec![(host, icon)]));
                }
            });
        }
    }

    pub(super) fn on_favicons_ready(&mut self, results: Vec<(String, Option<String>)>) {
        let mut fresh = Vec::new();
        for (host, icon) in results {
            if let Some(data) = &icon {
                fresh.push((host.clone(), data.clone()));
            }
            self.favicons.finish(&host, icon);
        }
        self.push_favicons(&fresh);
    }

    /// Icons go to the chrome and to open internal pages only, never to web pages.
    fn push_favicons(&self, icons: &[(String, String)]) {
        if icons.is_empty() {
            return;
        }
        let map: serde_json::Map<String, serde_json::Value> = icons
            .iter()
            .map(|(h, d)| (h.clone(), serde_json::Value::String(d.clone())))
            .collect();
        let Ok(json) = serde_json::to_string(&map) else { return };
        let script = format!(
            "if (location.origin.startsWith('http://browser.') && window.onFavicons) {{ window.onFavicons({json}); }}"
        );
        let _ = self.ui_webview.evaluate_script(&script);
        for tab in &self.tabs {
            if InternalPage::from_url(&tab.url).is_some() {
                tab.run_script(&script);
            }
        }
    }

    pub(super) fn widget_fetch(&mut self, kind: String, key: String, params: serde_json::Value) {
        let Some(url) = widgets::request_url(&kind, &params, i18n::lang()) else {
            self.push_widget(&kind, &key, Err("invalid request".into()));
            return;
        };
        if let Some((at, body)) = self.widget_cache.get(&url)
            && at.elapsed() < WIDGET_CACHE_TTL
        {
            let body = body.clone();
            self.push_widget(&kind, &key, Ok(body));
            return;
        }
        let proxy = self.proxy.clone();
        thread::spawn(move || {
            let result = widgets::fetch(&url);
            let _ = proxy.send_event(AppEvent::WidgetData { kind, key, url, result });
        });
    }

    pub(super) fn on_widget_data(
        &mut self,
        kind: String,
        key: String,
        url: String,
        result: Result<String, String>,
    ) {
        if let Ok(body) = &result {
            if self.widget_cache.len() >= 64 {
                self.widget_cache.clear();
            }
            self.widget_cache.insert(url, (Instant::now(), body.clone()));
        }
        self.push_widget(&kind, &key, result);
    }

    pub(super) fn set_page_pref(&mut self, key: &str, value: &str) {
        if !self.private {
            self.storage.set_page_pref(key, value);
        }
    }

    /// Sends the saved start-page preferences to every open start page (they merge them in).
    pub(super) fn push_page_prefs(&self) {
        let Ok(json) = serde_json::to_string(&self.storage.data.page_prefs) else {
            return;
        };
        let script = format!(
            "if (location.origin === 'http://browser.newtab' && window.onPagePrefs) {{ window.onPagePrefs({json}); }}"
        );
        for tab in &self.tabs {
            if InternalPage::from_url(&tab.url) == Some(InternalPage::NewTab) {
                tab.run_script(&script);
            }
        }
    }

    /// Answer goes to every open start page; each one only reacts to its own request key.
    fn push_widget(&self, kind: &str, key: &str, result: Result<String, String>) {
        let (ok, payload) = match result {
            Ok(body) => (true, body),
            Err(e) => (false, e),
        };
        let (Ok(kind), Ok(key), Ok(payload)) = (
            serde_json::to_string(kind),
            serde_json::to_string(key),
            serde_json::to_string(&payload),
        ) else {
            return;
        };
        let script = format!(
            "if (location.origin === 'http://browser.newtab' && window.onWidgetData) {{ window.onWidgetData({kind}, {key}, {ok}, {payload}); }}"
        );
        for tab in &self.tabs {
            if InternalPage::from_url(&tab.url) == Some(InternalPage::NewTab) {
                tab.run_script(&script);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icon_host() {
        assert_eq!(icon_host("https://WWW.Yle.fi/uutiset").as_deref(), Some("www.yle.fi"));
        assert_eq!(icon_host("browser://newtab"), None);
        assert_eq!(icon_host("http://browser.newtab/"), Some("browser.newtab".into()));
    }
}
