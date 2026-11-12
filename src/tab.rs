use wry::{Rect, WebView};

pub struct Tab {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub is_loading: bool,
    pub init_adblock_enabled: bool,
    pub webview: WebView,
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
        webview: WebView,
        initial_url: String,
        init_adblock_enabled: bool,
    ) -> Self {
        Self {
            id,
            title: "Uusi välilehti".to_string(),
            url: initial_url,
            is_loading: false,
            init_adblock_enabled,
            webview,
        }
    }

    pub fn load_url(&mut self, url: &str) {
        let _ = self.webview.load_url(&to_webview_url(url));
    }

    pub fn on_page_started(&mut self, url: &str) {
        self.is_loading = true;
        self.url = url.to_string();
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
        self.webview.can_go_back().unwrap_or(false)
    }

    pub fn can_go_forward(&self) -> bool {
        self.webview.can_go_forward().unwrap_or(false)
    }

    pub fn go_back(&self) {
        let _ = self.webview.go_back();
    }

    pub fn go_forward(&self) {
        let _ = self.webview.go_forward();
    }

    pub fn reload(&self) {
        let _ = self.webview.reload();
    }

    pub fn set_bounds(&self, bounds: Rect) {
        let _ = self.webview.set_bounds(bounds);
    }

    pub fn set_visible(&self, visible: bool) {
        let _ = self.webview.set_visible(visible);
    }

    pub fn focus(&self) {
        let _ = self.webview.focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_webview_url() {
        assert_eq!(to_webview_url("browser://history"), "http://browser.history");
        assert_eq!(to_webview_url("https://yle.fi"), "https://yle.fi");
    }
}
