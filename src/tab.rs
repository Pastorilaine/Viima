use crate::adblock::BlockedCounts;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;
use wry::{MemoryUsageLevel, Rect, WebView, WebViewExtWindows};

pub struct Tab {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub favicon: Option<String>,
    /// Requests blocked on the current page; bumped by the network filter, reset on navigation.
    pub blocked: Rc<Cell<BlockedCounts>>,
    /// Ad blocking is switched off for the site currently shown (per-site allowlist).
    pub site_allowed: Rc<Cell<bool>>,
    /// http:// URL that HTTPS-only mode rewrote to https:// (for the failure fallback).
    pub https_upgraded: Rc<RefCell<Option<String>>>,
    pub is_loading: bool,
    pub pinned: bool,
    pub muted: bool,
    pub playing_audio: bool,
    pub zoom: f64,
    /// When the tab was last shown; idle background tabs are put to sleep.
    pub last_active: Instant,
    /// `None` = not loaded (restored lazily or put to sleep); created on activation.
    pub webview: Option<WebView>,
}

/// WebView2 can't navigate to custom schemes directly; wry serves `browser://x` as
/// `http://browser.x`. `with_url` converts at build time but `load_url` does not.
pub fn to_webview_url(url: &str) -> String {
    match url.strip_prefix("browser://") {
        Some(rest) => format!("http://browser.{rest}"),
        None => url.to_string(),
    }
}

impl Tab {
    pub fn new(id: usize, url: String, title: String) -> Self {
        Self {
            id,
            title,
            url,
            favicon: None,
            blocked: Rc::default(),
            site_allowed: Rc::default(),
            https_upgraded: Rc::default(),
            is_loading: false,
            pinned: false,
            muted: false,
            playing_audio: false,
            zoom: 1.0,
            last_active: Instant::now(),
            webview: None,
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
        self.blocked.set(BlockedCounts::default());
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

    pub fn set_zoom(&mut self, factor: f64) {
        if (self.zoom - factor).abs() < 0.001 {
            return;
        }
        self.zoom = factor;
        if let Some(w) = &self.webview {
            let _ = w.zoom(factor);
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

    /// Unloads the page to free memory; title, URL and favicon stay for the tab strip.
    pub fn discard(&mut self) {
        if self.webview.take().is_some() {
            self.is_loading = false;
            self.playing_audio = false;
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
        assert_eq!(
            to_webview_url("browser://insecure?url=http%3A%2F%2Fx.fi"),
            "http://browser.insecure?url=http%3A%2F%2Fx.fi"
        );
        assert_eq!(to_webview_url("https://yle.fi"), "https://yle.fi");
    }
}
