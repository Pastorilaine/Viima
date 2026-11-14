//! Menus, zoom, downloads, settings side effects, filter lists, private windows.
use super::*;
use crate::storage::site_key;
#[cfg(windows)]
use crate::win_shell::{MenuEntry, popup_menu};

/// Chrome's zoom steps.
const ZOOM_LEVELS: &[f64] = &[
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0,
];

pub(super) fn next_zoom(current: f64, dir: i8) -> f64 {
    match dir {
        0 => 1.0,
        d if d > 0 => ZOOM_LEVELS
            .iter()
            .copied()
            .find(|z| *z > current + 0.001)
            .unwrap_or(5.0),
        _ => ZOOM_LEVELS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < current - 0.001)
            .unwrap_or(0.25),
    }
}

impl BrowserApp {
    pub(super) fn zoom_active(&mut self, dir: i8) {
        self.with_active(|t| {
            let target = next_zoom(t.zoom, dir);
            t.set_zoom(target);
        });
        let (id, zoom) = match self.active_tab() {
            Some(t) => (t.id, t.zoom),
            None => return,
        };
        self.on_zoom_changed(id, zoom);
    }

    /// Remembers the zoom level per site (WebView2 zooms per tab, not per site).
    pub(super) fn on_zoom_changed(&mut self, id: usize, factor: f64) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.zoom = factor;
        if let Some(site) = site_key(&tab.url) {
            self.storage.set_site_zoom(&site, factor);
        }
        self.update_ui();
    }

    pub(super) fn open_private_window(&mut self) {
        // Pages can send this shortcut too: never let one spawn a flood of processes.
        if self
            .last_private_spawn
            .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        self.last_private_spawn = Some(Instant::now());
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).arg("--private").spawn();
        }
    }

    #[cfg(windows)]
    pub(super) fn tab_menu(&mut self, id: usize) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let tab = &self.tabs[idx];
        let has_right = self.tabs[idx + 1..].iter().any(|t| !t.pinned);
        let has_others = self.tabs.iter().any(|t| t.id != id && !t.pinned);
        let entries = [
            MenuEntry::item(1, tr("Päivitä välilehti", "Reload tab")),
            MenuEntry::item(2, tr("Monista välilehti", "Duplicate tab")),
            MenuEntry::item(
                3,
                if tab.pinned {
                    tr("Irrota välilehti", "Unpin tab")
                } else {
                    tr("Kiinnitä välilehti", "Pin tab")
                },
            ),
            MenuEntry::item(
                4,
                if tab.muted {
                    tr("Poista mykistys", "Unmute tab")
                } else {
                    tr("Mykistä välilehti", "Mute tab")
                },
            ),
            MenuEntry::Separator,
            MenuEntry::item(5, tr("Sulje välilehti\tCtrl+W", "Close tab\tCtrl+W")),
            MenuEntry::Item {
                id: 6,
                label: tr("Sulje muut välilehdet", "Close other tabs").into(),
                checked: false,
                enabled: has_others,
            },
            MenuEntry::Item {
                id: 7,
                label: tr("Sulje oikealla olevat", "Close tabs to the right").into(),
                checked: false,
                enabled: has_right,
            },
            MenuEntry::Separator,
            MenuEntry::item(
                8,
                tr("Avaa suljettu välilehti\tCtrl+Shift+T", "Reopen closed tab\tCtrl+Shift+T"),
            ),
        ];
        match popup_menu(self.hwnd(), &entries) {
            // A sleeping tab has nothing to reload: opening it loads the page.
            Some(1) => match self.tab_mut(id) {
                Some(t) if t.webview.is_some() => t.reload(),
                _ => self.switch_tab(id),
            },
            Some(2) => self.duplicate_tab(id),
            Some(3) => self.toggle_pin(id),
            Some(4) => self.toggle_mute(id),
            Some(5) => self.close_tab(id),
            Some(6) => self.close_other_tabs(id, false),
            Some(7) => self.close_other_tabs(id, true),
            Some(8) => {
                let _ = self.proxy.send_event(AppEvent::IpcReopenTab);
            }
            _ => {}
        }
    }

    #[cfg(windows)]
    pub(super) fn shield_menu(&mut self) {
        let Some(tab) = self.active_tab() else { return };
        let site = site_key(&tab.url);
        let counts = tab.blocked.get();
        let global = self.adblock_enabled.get();
        let mut entries = Vec::new();
        if let Some(site) = &site {
            entries.push(MenuEntry::info(site.clone()));
            entries.push(MenuEntry::info(format!(
                "{} {} — {} {} · {} {} · {} {}",
                tr("Estetty", "Blocked"),
                counts.total(),
                tr("mainokset", "ads"),
                counts.ads,
                tr("seuranta", "trackers"),
                counts.trackers,
                tr("skriptit", "scripts"),
                counts.scripts
            )));
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Item {
                id: 1,
                label: tr("Estä mainokset tällä sivustolla", "Block ads on this site").into(),
                checked: global && !self.storage.is_adblock_allowed(site),
                enabled: global,
            });
        }
        entries.push(MenuEntry::check(
            2,
            tr("Mainosesto käytössä (kaikki sivustot)", "Ad blocking on (all sites)"),
            global,
        ));
        if site.is_some() {
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::item(
                3,
                tr(
                    "Tyhjennä tämän sivuston evästeet ja tiedot",
                    "Clear cookies and data for this site",
                ),
            ));
        }
        match (popup_menu(self.hwnd(), &entries), site) {
            (Some(1), Some(site)) => {
                let allowed = self.storage.toggle_adblock_allow(&site);
                let id = self.active_tab_id;
                if let Some(tab) = self.tab_mut(id) {
                    tab.site_allowed.set(allowed);
                    tab.reload();
                }
                self.update_data();
            }
            (Some(2), _) => self.toggle_adblock(),
            (Some(3), Some(site)) => self.clear_site_data(&site),
            _ => {}
        }
    }

    #[cfg(windows)]
    pub(super) fn bookmark_folder_menu(&mut self, folder: &str) {
        let items: Vec<(String, String)> = self
            .storage
            .data
            .bookmarks
            .iter()
            .filter(|b| b.folder == folder)
            .map(|b| (b.title.clone(), b.url.clone()))
            .collect();
        let mut entries: Vec<MenuEntry> = items
            .iter()
            .enumerate()
            .take(60)
            .map(|(i, (title, _))| {
                let label: String = title.chars().take(60).collect();
                MenuEntry::item(i as u32 + 1, label)
            })
            .collect();
        if entries.is_empty() {
            entries.push(MenuEntry::info(tr("(tyhjä)", "(empty)")));
        }
        if let Some(choice) = popup_menu(self.hwnd(), &entries)
            && let Some((_, url)) = items.get(choice as usize - 1)
        {
            let url = url.clone();
            self.navigate_active(&url);
        }
    }

    #[cfg(not(windows))]
    pub(super) fn tab_menu(&mut self, _id: usize) {}
    #[cfg(not(windows))]
    pub(super) fn shield_menu(&mut self) {}
    #[cfg(not(windows))]
    pub(super) fn bookmark_folder_menu(&mut self, _folder: &str) {}

    pub(super) fn on_download_update(&mut self, item: DownloadItem) {
        let finished = item.state != "in_progress";
        let is_new = !self.storage.data.downloads.iter().any(|d| d.id == item.id);
        self.storage.upsert_download(item);
        // Progress ticks arrive many times a second: repaint at most ~4x per second.
        if finished || is_new || self.last_download_push.elapsed() >= Duration::from_millis(250) {
            self.last_download_push = Instant::now();
            self.push_downloads(true);
        }
    }

    pub(super) fn download_action(&mut self, op: &str, id: u64) {
        let Some(item) = self
            .storage
            .data
            .downloads
            .iter()
            .find(|d| d.id == id)
            .cloned()
        else {
            return;
        };
        match op {
            #[cfg(windows)]
            "open" if item.state == "completed" => crate::win_shell::open_path(&item.path),
            #[cfg(windows)]
            "show" if !item.path.is_empty() => crate::win_shell::show_in_folder(&item.path),
            #[cfg(windows)]
            "cancel" => {
                if let Some(op) = self.download_ops.borrow().get(&id) {
                    unsafe {
                        let _ = op.Cancel();
                    }
                }
            }
            "remove" => {
                self.storage.remove_download(id);
                self.push_downloads(true);
            }
            "retry" if item.url.starts_with("https://") || item.url.starts_with("http://") => {
                self.storage.remove_download(id);
                self.navigate_active(&item.url);
            }
            _ => {}
        }
    }

    pub(super) fn set_default_browser(&mut self) {
        #[cfg(windows)]
        {
            let result = std::env::current_exe()
                .map_err(|e| e.to_string())
                .and_then(|exe| crate::win_shell::open_default_apps_settings(&exe));
            if let Err(e) = result {
                self.storage.last_error = Some(e);
                self.update_ui();
            }
        }
    }

    pub(super) fn clear_site_data(&mut self, site: &str) {
        #[cfg(windows)]
        crate::webview_hooks::clear_site_data(&self.ui_webview, site, self.proxy.clone());
        #[cfg(not(windows))]
        let _ = site;
    }

    pub(super) fn update_filter_lists(&mut self) {
        if self.filter_status.updating {
            return;
        }
        self.filter_status.updating = true;
        self.filter_status.error = None;
        self.last_filter_attempt = Some(Instant::now());
        self.update_data();
        let proxy = self.proxy.clone();
        thread::spawn(move || {
            let _ = proxy.send_event(AppEvent::FilterListsUpdated(AdBlocker::update_lists()));
        });
    }

    pub(super) fn on_filter_lists_updated(&mut self, result: Result<usize, String>) {
        self.filter_status.updating = false;
        match result {
            Ok(count) if self.storage.data.settings.adblock_lists => {
                let s = &mut self.storage.data.settings;
                s.adblock_lists_updated = now_secs();
                s.adblock_rule_count = count;
                self.storage.save();
            }
            // Turned off while downloading: drop what arrived.
            Ok(_) => AdBlocker::clear_lists(),
            Err(e) => self.filter_status.error = Some(e),
        }
        self.update_data();
    }

    pub(super) fn allow_http(&mut self, url: &str) {
        let Ok(parsed) = url::Url::parse(url) else {
            return;
        };
        if parsed.scheme() != "http" {
            return;
        }
        if let Some(host) = parsed.host_str() {
            self.http_allowed.borrow_mut().insert(host.to_string());
        }
        let id = self.active_tab_id;
        if let Some(tab) = self.tab_mut(id) {
            tab.load_url(url);
        }
    }

    /// Extension list for open Settings pages. `started` = what this run actually loaded, so the
    /// page can tell when a restart is needed.
    pub(super) fn push_extensions(&self) {
        let payload = serde_json::json!({
            "list": crate::extensions::list(),
            "started": self.extensions_started,
            "private": self.private,
            "folder": crate::extensions::enabled_dir().to_string_lossy(),
        });
        let script = format!(
            "if (location.origin === 'http://browser.settings' && window.onExtensions) {{ window.onExtensions({payload}); }}"
        );
        for tab in &self.tabs {
            if InternalPage::from_url(&tab.url) == Some(InternalPage::Settings) {
                tab.run_script(&script);
            }
        }
    }

    pub(super) fn on_tick(&mut self) {
        self.sleep_idle_tabs();
        // Window size too, so a crash or Windows shutdown doesn't lose it.
        if !self.private {
            self.save_window_state();
        }
        self.favicons.save();
        let s = &self.storage.data.settings;
        let stale =
            now_secs().saturating_sub(s.adblock_lists_updated) > crate::adblock::LIST_MAX_AGE_SECS;
        let retry_ok = self
            .last_filter_attempt
            .is_none_or(|t| t.elapsed() > Duration::from_secs(3600));
        if s.adblock_lists && stale && retry_ok && !self.private {
            self.update_filter_lists();
        }
    }

    pub(super) fn set_setting(&mut self, key: &str, value: serde_json::Value) {
        if key == "toggle_bookmarks_bar" {
            let v = !self.storage.data.settings.show_bookmarks_bar;
            return self.set_setting("show_bookmarks_bar", serde_json::Value::Bool(v));
        }
        if !self.storage.set_setting(key, &value) {
            return;
        }
        let s = &self.storage.data.settings;
        match key {
            "show_bookmarks_bar" => self.layout(),
            "https_only" => self.https_only.set(s.https_only),
            "hide_cookie_banners" => {
                let script = format!(
                    "window.__viimaCookieHide && window.__viimaCookieHide({});",
                    s.hide_cookie_banners
                );
                for tab in &self.tabs {
                    tab.run_script(&script);
                }
            }
            "devtools_enabled" => {
                #[cfg(windows)]
                for tab in &self.tabs {
                    if let Some(w) = &tab.webview {
                        crate::webview_hooks::set_devtools_enabled(w, s.devtools_enabled);
                    }
                }
            }
            "adblock_lists" => {
                if s.adblock_lists {
                    self.update_filter_lists();
                } else {
                    AdBlocker::clear_lists();
                    self.storage.data.settings.adblock_rule_count = 0;
                    self.storage.data.settings.adblock_lists_updated = 0;
                    self.storage.save();
                }
            }
            "language" => {
                i18n::set_language(&s.language);
                // Pages translate themselves on load; reload the chrome and internal pages.
                let _ = self.ui_webview.reload();
                for tab in &self.tabs {
                    if InternalPage::from_url(&tab.url).is_some() {
                        tab.reload();
                    }
                }
                self.sync_window_title();
                #[cfg(windows)]
                if !self.private {
                    self.refresh_jump_list();
                }
            }
            _ => {}
        }
        self.update_data();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_steps() {
        assert_eq!(next_zoom(1.0, 1), 1.1);
        assert_eq!(next_zoom(1.0, -1), 0.9);
        assert_eq!(next_zoom(1.13, 1), 1.25);
        assert_eq!(next_zoom(5.0, 1), 5.0);
        assert_eq!(next_zoom(0.25, -1), 0.25);
        assert_eq!(next_zoom(2.0, 0), 1.0);
    }
}
