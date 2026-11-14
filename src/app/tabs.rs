//! Tab lifecycle: create, restore, switch, close, reorder, pin, mute, sleep.
use super::*;
use crate::url_resolver::UrlResolver;
use wry::NewWindowResponse;

/// HTTPS-only mode: the https:// URL to load instead of `url`, if it should be upgraded.
/// Local hosts, IP addresses, internal pages and user-approved hosts stay on http.
pub(super) fn https_upgrade(url: &str, allowed: &HashSet<String>) -> Option<String> {
    let mut parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "http" {
        return None;
    }
    let host = match parsed.host()? {
        url::Host::Domain(d) => d.to_string(),
        _ => return None, // IPv4/IPv6 literal: usually a LAN device without a certificate
    };
    let local = !host.contains('.')
        || host.starts_with("browser.")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".lan");
    if local || allowed.contains(&host) {
        return None;
    }
    parsed.set_scheme("https").ok()?;
    if parsed.port() == Some(80) {
        let _ = parsed.set_port(None);
    }
    Some(parsed.into())
}

impl BrowserApp {
    fn create_tab_webview(&mut self, idx: usize) -> WebView {
        let rect = self.content_rect();
        let tab = &self.tabs[idx];
        let id = tab.id;
        let url = tab.url.clone();
        let site_allowed = tab.site_allowed.clone();
        let blocked = tab.blocked.clone();
        let upgraded = tab.https_upgraded.clone();
        let muted = tab.muted;

        let settings = &self.storage.data.settings;
        let adblock_now = self.adblock_enabled.get() && !site_allowed.get();
        let init_script = format!(
            "window.__viimaLang='{}';{} window.__viimaAdblockSetEnabled({}); {} window.__viimaCookieHide({}); {} {}",
            i18n::lang(),
            AdBlocker::get_injection_script(),
            adblock_now,
            AdBlocker::cookie_banner_script(),
            settings.hide_cookie_banners,
            FAVICON_SCRIPT,
            FIND_SCRIPT
        );

        let proxy_page_load = self.proxy.clone();
        let proxy_title = self.proxy.clone();
        let proxy_new_tab = self.proxy.clone();
        let proxy_ipc = self.proxy.clone();
        let proxy_nav = self.proxy.clone();
        let adblock_policy = self.adblock_enabled.clone();
        let https_only = self.https_only.clone();
        let http_allowed = self.http_allowed.clone();
        let nav_upgraded = upgraded.clone();

        let ext_enabled = self.extensions_path.is_some();
        let builder = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_browser_extensions_enabled(ext_enabled)
            .with_theme(web_theme(DARK_MODE.load(Ordering::Relaxed)))
            .with_additional_browser_args(BROWSER_ARGS)
            .with_incognito(self.private)
            .with_devtools(settings.devtools_enabled)
            .with_hotkeys_zoom(true)
            .with_bounds(rect)
            .with_custom_protocol("browser".into(), protocol::handle_protocol)
            .with_ipc_handler(move |req| {
                let trusted = ipc::is_trusted_ipc_source(req.uri());
                if let Some(event) = ipc::parse(req.body(), trusted, Some(id)) {
                    let _ = proxy_ipc.send_event(event);
                }
            })
            .with_on_page_load_handler(move |event, url| {
                let _ = proxy_page_load.send_event(match event {
                    PageLoadEvent::Started => AppEvent::TabPageLoadStarted(id, url),
                    PageLoadEvent::Finished => AppEvent::TabPageLoadFinished(id, url),
                });
            })
            .with_document_title_changed_handler(move |title| {
                let _ = proxy_title.send_event(AppEvent::TabTitleChanged(id, title));
            })
            .with_navigation_handler(move |url| {
                if adblock_policy.get() && AdBlocker::should_block_url(&url) {
                    return false;
                }
                if https_only.get()
                    && let Some(https) = https_upgrade(&url, &http_allowed.borrow())
                {
                    *nav_upgraded.borrow_mut() = Some(url);
                    let _ = proxy_nav.send_event(AppEvent::TabNavigateTo(id, https));
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
            })
            .with_initialization_script(init_script);

        let webview = builder
            .with_url(&url)
            .build_as_child(&*self.window)
            .expect("Failed to create tab webview");
        #[cfg(windows)]
        {
            if let Err(e) = AdBlocker::install_network_filter(
                &webview,
                self.adblock_enabled.clone(),
                site_allowed,
                blocked,
            ) {
                eprintln!("adblock network filter unavailable: {e}");
            }
            if let Err(e) = crate::webview_hooks::install(
                &webview,
                id,
                self.proxy.clone(),
                self.download_ops.clone(),
                upgraded,
            ) {
                eprintln!("webview hooks unavailable: {e}");
            }
            if muted {
                crate::webview_hooks::set_muted(&webview, true);
            }
        }
        webview
    }

    /// Opens a tab at the end of the strip (or at `index`) and shows it.
    pub(super) fn open_tab_at(&mut self, url: Option<String>, index: Option<usize>) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;

        let initial_url = url.unwrap_or_else(|| "browser://newtab".to_string());
        let resolved =
            UrlResolver::resolve(&initial_url, &self.storage.data.settings.search_engine);

        let tab = Tab::new(id, resolved, i18n::new_tab_title().to_string());
        if let Some(site) = crate::storage::site_key(&tab.url) {
            tab.site_allowed.set(self.storage.is_adblock_allowed(&site));
        }
        let idx = index
            .unwrap_or(self.tabs.len())
            .clamp(self.pinned_count(), self.tabs.len());
        self.tabs.insert(idx, tab);
        let webview = self.create_tab_webview(idx);
        self.tabs[idx].webview = Some(webview);
        self.tabs[idx].is_loading = true;
        self.activate(id);

        self.update_ui();
        self.sync_session();
    }

    pub(super) fn new_tab(&mut self, url: Option<String>) {
        // Settings, history, downloads and bookmarks open in a tab of their own, and one is
        // enough: asking for it again just goes back to it. (Start pages can be many.)
        let wanted = url.as_deref().and_then(InternalPage::from_url);
        if let Some(kind) = wanted
            && !matches!(kind, InternalPage::NewTab | InternalPage::Insecure)
            && let Some(id) = self
                .tabs
                .iter()
                .find(|t| InternalPage::from_url(&t.url) == Some(kind))
                .map(|t| t.id)
        {
            self.switch_tab(id);
            return;
        }
        self.open_tab_at(url, None);
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|t| t.pinned).count()
    }

    /// Recreates last session's tabs. Only the active one gets a webview now; the rest load
    /// when first opened, so a big session doesn't spawn dozens of renderers at startup.
    pub(super) fn restore_tabs(&mut self, session: crate::storage::Session) {
        let active = session
            .active_index
            .min(session.tabs.len().saturating_sub(1));
        for saved in session.tabs {
            let id = self.next_tab_id;
            self.next_tab_id += 1;
            let title = if saved.title.trim().is_empty() {
                saved.url.clone()
            } else {
                saved.title
            };
            let mut tab = Tab::new(id, saved.url, title);
            tab.pinned = saved.pinned;
            self.tabs.push(tab);
        }
        // Keep pinned tabs first even if the saved order was off.
        self.tabs.sort_by_key(|t| !t.pinned);
        let id = self.tabs[active.min(self.tabs.len() - 1)].id;
        self.activate(id);
        self.update_ui();
    }

    /// Shows tab `id` (creating its webview if it is not loaded) and hides the others.
    fn activate(&mut self, id: usize) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        if self.tabs[idx].webview.is_none() {
            let webview = self.create_tab_webview(idx);
            let tab = &mut self.tabs[idx];
            tab.webview = Some(webview);
            tab.is_loading = true;
            tab.zoom = 1.0;
        }

        let content_rect = self.content_rect();
        let now = Instant::now();
        for tab in &mut self.tabs {
            if tab.id == id || tab.id == self.active_tab_id {
                tab.last_active = now;
            }
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
    }

    fn remember_closed(&mut self, idx: usize) {
        let tab = &self.tabs[idx];
        if InternalPage::from_url(&tab.url) == Some(InternalPage::NewTab)
            || tab.url.is_empty()
            || tab.url == "about:blank"
        {
            return;
        }
        let entry = SessionTab {
            url: Self::session_url(&tab.url),
            title: tab.title.clone(),
            pinned: false,
        };
        if self.closed_tabs.len() >= MAX_CLOSED_TABS {
            self.closed_tabs.remove(0);
        }
        self.closed_tabs.push(entry);
    }

    pub(super) fn close_tab(&mut self, id: usize) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };

        if self.tabs.len() <= 1 {
            // If only one tab left, reset it to newtab instead of closing window
            if InternalPage::from_url(&self.tabs[0].url) != Some(InternalPage::NewTab) {
                self.remember_closed(0);
                let tab = &mut self.tabs[0];
                tab.pinned = false;
                tab.load_url("browser://newtab");
                tab.url = "browser://newtab".to_string();
                tab.title = i18n::new_tab_title().to_string();
                tab.is_loading = true;
                self.sync_window_title();
                self.update_ui();
                self.sync_session();
            }
            return;
        }

        self.remember_closed(idx);
        let was_active = self.active_tab_id == id;
        let removed = self.tabs.remove(idx);
        removed.set_visible(false);
        drop(removed);

        if was_active {
            let new_idx = idx.min(self.tabs.len() - 1);
            let next_id = self.tabs[new_idx].id;
            self.activate(next_id);
        }

        self.update_ui();
        self.sync_session();
    }

    pub(super) fn switch_tab(&mut self, id: usize) {
        if self.active_tab_id == id || !self.tabs.iter().any(|tab| tab.id == id) {
            return;
        }
        self.activate(id);
        self.update_ui();
        self.sync_session();
    }

    pub(super) fn select_tab_index(&mut self, index: usize) {
        let idx = index.min(self.tabs.len().saturating_sub(1));
        if let Some(tab) = self.tabs.get(idx) {
            let id = tab.id;
            self.switch_tab(id);
        }
    }

    pub(super) fn cycle_tab(&mut self, forward: bool) {
        let len = self.tabs.len();
        if len < 2 {
            return;
        }
        let Some(idx) = self.tabs.iter().position(|t| t.id == self.active_tab_id) else {
            return;
        };
        let next = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        let id = self.tabs[next].id;
        self.switch_tab(id);
    }

    /// Drag & drop: pinned tabs stay in the pinned block, others after it.
    pub(super) fn reorder_tab(&mut self, id: usize, to: usize) {
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let tab = self.tabs.remove(from);
        let pinned = self.pinned_count();
        let to = if tab.pinned {
            to.min(pinned)
        } else {
            to.clamp(pinned, self.tabs.len())
        };
        self.tabs.insert(to, tab);
        if from != to {
            self.update_ui();
            self.sync_session();
        }
    }

    pub(super) fn toggle_pin(&mut self, id: usize) {
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let mut tab = self.tabs.remove(from);
        tab.pinned = !tab.pinned;
        // Pinning moves the tab to the end of the pinned block, unpinning right after it.
        let to = self.pinned_count();
        self.tabs.insert(to, tab);
        self.update_ui();
        self.sync_session();
    }

    pub(super) fn toggle_mute(&mut self, id: usize) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.muted = !tab.muted;
        #[cfg(windows)]
        if let Some(w) = &tab.webview {
            crate::webview_hooks::set_muted(w, tab.muted);
        }
        self.update_ui();
    }

    pub(super) fn duplicate_tab(&mut self, id: usize) {
        if let Some(idx) = self.tabs.iter().position(|t| t.id == id) {
            let url = Self::session_url(&self.tabs[idx].url);
            self.open_tab_at(Some(url), Some(idx + 1));
        }
    }

    /// Closes the unpinned tabs other than `id` (`right_only`: only those to its right).
    pub(super) fn close_other_tabs(&mut self, id: usize, right_only: bool) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let ids: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(i, t)| !t.pinned && t.id != id && (!right_only || *i > idx))
            .map(|(_, t)| t.id)
            .collect();
        for other in ids {
            self.close_tab(other);
        }
    }

    /// Internal pages are stored as `browser://x` so they restore regardless of WebView2's mapping.
    pub(super) fn session_url(url: &str) -> String {
        match InternalPage::from_url(url) {
            // The warning page's target is only meaningful right now.
            Some(InternalPage::Insecure) => InternalPage::NewTab.browser_url().to_string(),
            Some(page) => {
                let parsed = url::Url::parse(url).ok();
                let query = parsed.as_ref().and_then(|u| u.query()).unwrap_or("");
                if query.is_empty() {
                    page.browser_url().to_string()
                } else {
                    format!("{}?{query}", page.browser_url())
                }
            }
            None => url.to_string(),
        }
    }

    /// Mirrors open tabs into storage; the background writer only runs when something changed.
    pub(super) fn sync_session(&mut self) {
        if self.private {
            return;
        }
        let keep = |t: &&Tab| !t.url.is_empty() && t.url != "about:blank";
        let tabs: Vec<SessionTab> = self
            .tabs
            .iter()
            .filter(keep)
            .map(|t| SessionTab {
                url: Self::session_url(&t.url),
                title: t.title.clone(),
                pinned: t.pinned,
            })
            .collect();
        let active_index = self
            .tabs
            .iter()
            .filter(keep)
            .position(|t| t.id == self.active_tab_id)
            .unwrap_or(0);
        let session = crate::storage::Session {
            tabs,
            active_index,
            ..self.storage.data.session.clone()
        };
        self.storage.set_session(session);
    }

    pub(super) fn restore_window_state(window: &Window, session: &crate::storage::Session) {
        if let Some((w, h)) = session.window_size {
            let scale = window.scale_factor();
            // Never restore bigger than the current monitor (it may have changed since).
            let (max_w, max_h) = window
                .current_monitor()
                .map(|m| {
                    let s = m.size().to_logical::<f64>(scale);
                    (s.width, s.height)
                })
                .unwrap_or((f64::MAX, f64::MAX));
            let w = w.clamp(640.0, max_w.max(640.0));
            let h = h.clamp(480.0, max_h.max(480.0));
            window.set_inner_size(LogicalSize::new(w, h));
        }
        if session.maximized {
            window.set_maximized(true);
        }
    }

    pub(super) fn save_window_state(&mut self) {
        let mut session = self.storage.data.session.clone();
        session.maximized = self.window.is_maximized();
        if !session.maximized && self.window_size.width > 0 && self.window_size.height > 0 {
            let size = self
                .window_size
                .to_logical::<f64>(self.window.scale_factor());
            session.window_size = Some((size.width, size.height));
        }
        self.storage.set_session(session);
    }

    pub(super) fn navigate_active(&mut self, input: &str) {
        let resolved = UrlResolver::resolve(input, &self.storage.data.settings.search_engine);
        if self.adblock_enabled.get() && AdBlocker::should_block_url(&resolved) {
            return;
        }
        let id = self.active_tab_id;
        if let Some(tab) = self.tab_mut(id) {
            tab.load_url(&resolved);
            tab.url = resolved;
            tab.is_loading = true;
            tab.focus();
            self.update_ui();
        }
    }

    /// Unloads background tabs that haven't been looked at for a while.
    pub(super) fn sleep_idle_tabs(&mut self) {
        if !self.storage.data.settings.sleeping_tabs {
            return;
        }
        // A destroyed webview could take its running download with it.
        let downloading = self
            .storage
            .data
            .downloads
            .iter()
            .any(|d| d.state == "in_progress");
        if downloading {
            return;
        }
        let active = self.active_tab_id;
        let mut changed = false;
        for tab in &mut self.tabs {
            if tab.id != active
                && tab.webview.is_some()
                && !tab.playing_audio
                && tab.last_active.elapsed() >= SLEEP_AFTER
            {
                tab.discard();
                changed = true;
            }
        }
        if changed {
            self.update_ui();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_https_upgrade() {
        let mut allowed = HashSet::new();
        assert_eq!(
            https_upgrade("http://example.com/a?b=1", &allowed).as_deref(),
            Some("https://example.com/a?b=1")
        );
        assert_eq!(
            https_upgrade("http://example.com:80/", &allowed).as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(https_upgrade("https://example.com/", &allowed), None);
        assert_eq!(https_upgrade("http://browser.settings/", &allowed), None);
        assert_eq!(https_upgrade("http://localhost:3000/", &allowed), None);
        assert_eq!(https_upgrade("http://192.168.1.1/", &allowed), None);
        assert_eq!(https_upgrade("http://printer.local/", &allowed), None);
        allowed.insert("old.example.com".to_string());
        assert_eq!(https_upgrade("http://old.example.com/", &allowed), None);
    }

    #[test]
    fn test_session_url() {
        assert_eq!(
            BrowserApp::session_url("http://browser.history/"),
            "browser://history"
        );
        assert_eq!(
            BrowserApp::session_url("http://browser.insecure/?url=http%3A%2F%2Fx"),
            "browser://newtab"
        );
        assert_eq!(BrowserApp::session_url("https://yle.fi/"), "https://yle.fi/");
    }
}
