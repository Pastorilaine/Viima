use std::cell::Cell;
use std::rc::Rc;
use wry::{MemoryUsageLevel, Rect, WebView, WebViewExtWindows};

pub struct Tab {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub favicon: Option<String>,
    /// Requests blocked on the current page; bumped by the network filter, reset on navigation.
    pub blocked: Rc<Cell<u32>>,
    pub is_loading: bool,
    pub init_adblock_enabled: bool,
    /// `None` = restored tab not opened yet; created on first activation to keep startup light.
    pub webview: Option<WebView>,
}

/// WebView2 can't navigate to custom schemes directly; wry serves `browser://x` as
/// `http://browser.x`. `with_url` converts at build time but `load_url` does not.
fn to_webview_url(url: &str) -> String {
    match url.strip_prefix("browser://") {
        Some(rest) => format!("http://browser.{rest}"),
        None => url.to_string(),
    }
}

impl Tab {
    pub fn new(
        id: usize,
        webview: Option<WebView>,
        url: String,
        title: String,
        init_adblock_enabled: bool,
    ) -> Self {
        Self {
            id,
            title,
            url,
            favicon: None,
            blocked: Rc::default(),
            is_loading: false,
            init_adblock_enabled,
            webview,
        }
    }

    pub fn load_url(&mut self, url: &str) {
        if let Some(w) = &self.webview {
            let _ = w.load_url(&to_webview_url(url));
        }
    }

    /// Runs a script built by the app itself (state pushes, adblock toggle) in this tab.
    pub fn run_script(&self, script: &str) {
        if let Some(w) = &self.webview {
            let _ = w.evaluate_script(script);
        }
    }

    pub fn on_page_started(&mut self, url: &str) {
        self.is_loading = true;
        self.url = url.to_string();
        self.favicon = None;
        self.blocked.set(0);
    }

    pub fn on_page_finished(&mut self, url: &str) {
        self.is_loading = false;
        if !url.is_empty() {
            self.url = url.to_string();
        }
    }

    pub fn on_title_changed(&mut self, title: &str) {
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            self.title = trimmed.to_string();
        }
    }

    // Ask WebView2 itself: it knows about SPA (pushState) and redirect entries.
    pub fn can_go_back(&self) -> bool {
        self.webview
            .as_ref()
            .is_some_and(|w| w.can_go_back().unwrap_or(false))
    }

    pub fn can_go_forward(&self) -> bool {
        self.webview
            .as_ref()
            .is_some_and(|w| w.can_go_forward().unwrap_or(false))
    }

    pub fn go_back(&self) {
        if let Some(w) = &self.webview {
            let _ = w.go_back();
        }
    }

    pub fn go_forward(&self) {
        if let Some(w) = &self.webview {
            let _ = w.go_forward();
        }
    }

    pub fn reload(&self) {
        if let Some(w) = &self.webview {
            let _ = w.reload();
        }
    }

    pub fn set_bounds(&self, bounds: Rect) {
        if let Some(w) = &self.webview {
            let _ = w.set_bounds(bounds);
        }
    }

    /// Hidden tabs drop to low memory mode so WebView2 can trim their caches.
    pub fn set_visible(&self, visible: bool) {
        if let Some(w) = &self.webview {
            let _ = w.set_visible(visible);
            let _ = w.set_memory_usage_level(if visible {
                MemoryUsageLevel::Normal
            } else {
                MemoryUsageLevel::Low
            });
        }
    }

    pub fn set_theme(&self, theme: wry::Theme) {
        if let Some(w) = &self.webview {
            let _ = w.set_theme(theme);
        }
    }

    pub fn focus(&self) {
        if let Some(w) = &self.webview {
            let _ = w.focus();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_webview_url() {
        assert_eq!(
            to_webview_url("browser://history"),
            "http://browser.history"
        );
        assert_eq!(to_webview_url("https://yle.fi"), "https://yle.fi");
    }
}
