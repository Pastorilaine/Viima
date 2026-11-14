//! IPC from the chrome UI, internal pages and (restricted) web pages -> `AppEvent`.
use super::AppEvent;
use serde_json::Value;

/// Windows serves browser:// pages as http://browser.<name>/ (intercepted by wry). Bare hosts
/// like "settings" are NOT trusted: they would go to real DNS and could be served by anyone.
pub fn is_trusted_ipc_source(uri: &wry::http::Uri) -> bool {
    uri.scheme_str() == Some("http")
        && uri
            .host()
            .and_then(|h| h.strip_prefix("browser."))
            .is_some_and(|name| crate::storage::INTERNAL_PAGES.contains(&name))
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn n(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

/// Keyboard shortcuts forwarded from focused pages (the chrome webview never sees them).
fn shortcut(key: &str) -> Option<AppEvent> {
    Some(match key {
        "new_tab" => AppEvent::IpcNewTab(None),
        "close_tab" => AppEvent::IpcCloseActiveTab,
        "focus_omnibox" => AppEvent::IpcFocusOmnibox,
        "history" => AppEvent::IpcNewTab(Some("browser://history".into())),
        "downloads" => AppEvent::IpcNewTab(Some("browser://downloads".into())),
        "reopen_tab" => AppEvent::IpcReopenTab,
        "next_tab" => AppEvent::IpcCycleTab(true),
        "prev_tab" => AppEvent::IpcCycleTab(false),
        "private_window" => AppEvent::IpcNewPrivateWindow,
        "bookmarks_bar" => AppEvent::IpcSetSetting("toggle_bookmarks_bar".into(), Value::Null),
        "bookmark" => AppEvent::IpcToggleBookmark,
        k if k.starts_with("tab_") => match k[4..].parse::<usize>() {
            Ok(9) => AppEvent::IpcSelectTab(usize::MAX),
            Ok(i @ 1..=8) => AppEvent::IpcSelectTab(i - 1),
            _ => return None,
        },
        _ => return None,
    })
}

/// Browser shortcut for a real key press inside a tab (WebView2 accelerator key event).
/// Done natively, not via page JS: a page could otherwise post shortcuts in a loop
/// (endless private windows, closing the tab the user is looking at...).
pub fn shortcut_for_key(vk: u32, ctrl: bool, shift: bool, alt: bool) -> Option<AppEvent> {
    // Ctrl+Alt is AltGr on Finnish keyboards (@, {, [ ...): never a shortcut.
    if !ctrl || alt {
        return None;
    }
    let key = match (u8::try_from(vk).ok()?, shift) {
        (0x09, true) => "prev_tab", // VK_TAB
        (0x09, false) => "next_tab",
        (b'T', true) => "reopen_tab",
        (b'N', true) => "private_window",
        (b'B', true) => "bookmarks_bar",
        (d @ b'1'..=b'9', false) => return shortcut(&format!("tab_{}", d as char)),
        (b'T', false) => "new_tab",
        (b'W', false) => "close_tab",
        (b'L', false) => "focus_omnibox",
        (b'H', false) => "history",
        (b'J', false) => "downloads",
        (b'D', false) => "bookmark",
        _ => return None,
    };
    shortcut(key)
}

/// Untrusted (web page) senders may only report their favicon.
pub fn parse(body: &str, trusted: bool, tab_id: Option<usize>) -> Option<AppEvent> {
    let v: Value = serde_json::from_str(body).ok()?;
    let action = v.get("action")?.as_str()?;
    if !trusted && action != "favicon" {
        return None;
    }
    Some(match action {
        "shortcut" => shortcut(v.get("key")?.as_str()?)?,
        // Sent by any page: only ever shown as an <img> in the tab strip.
        "favicon" => {
            let url = s(&v, "url")?;
            let ok = url.len() <= 2048 && (url.starts_with("https://") || url.starts_with("http://"));
            if !ok {
                return None;
            }
            AppEvent::TabFavicon(tab_id?, url)
        }
        "new_tab" => AppEvent::IpcNewTab(s(&v, "url")),
        "close_tab" => AppEvent::IpcCloseTab(n(&v, "id")? as usize),
        "switch_tab" => AppEvent::IpcSwitchTab(n(&v, "id")? as usize),
        "navigate" => AppEvent::IpcNavigate(s(&v, "url")?),
        "back" => AppEvent::IpcBack,
        "forward" => AppEvent::IpcForward,
        "reload" => AppEvent::IpcReload,
        "toggle_bookmark" => AppEvent::IpcToggleBookmark,
        "remove_bookmark" => AppEvent::IpcRemoveBookmark(s(&v, "url")?),
        "set_bookmark_folder" => AppEvent::IpcSetBookmarkFolder(s(&v, "url")?, s(&v, "folder")?),
        "import_bookmarks" => {
            let items = v
                .get("items")?
                .as_array()?
                .iter()
                .filter_map(|i| Some((s(i, "title").unwrap_or_default(), s(i, "url")?, s(i, "folder").unwrap_or_default())))
                .collect();
            AppEvent::IpcImportBookmarks(items)
        }
        "bookmark_folder_menu" => AppEvent::IpcBookmarkFolderMenu(s(&v, "folder")?),
        "clear_history" => AppEvent::IpcClearHistory,
        "delete_history_item" => AppEvent::IpcDeleteHistoryItem(s(&v, "url")?),
        "search_history" => AppEvent::IpcSearchHistory(s(&v, "q").unwrap_or_default()),
        "set_search_engine" => AppEvent::IpcSetSearchEngine(s(&v, "engine")?),
        "toggle_adblock" => AppEvent::IpcToggleAdblock,
        "toggle_theme" => AppEvent::IpcToggleTheme,
        "set_setting" => AppEvent::IpcSetSetting(s(&v, "key")?, v.get("value")?.clone()),
        "check_updates" => AppEvent::IpcCheckUpdates,
        "download_update" => AppEvent::IpcDownloadUpdate,
        "install_update" => AppEvent::IpcInstallUpdate,
        "toggle_auto_update" => AppEvent::IpcToggleAutoUpdate,
        "set_update_url" => AppEvent::IpcSetUpdateUrl(s(&v, "url")?),
        "request_favicons" => AppEvent::IpcRequestFavicons(
            v.get("hosts")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .filter(|h| h.len() <= 253)
                .take(48)
                .map(str::to_string)
                .collect(),
        ),
        "widget_fetch" => AppEvent::IpcWidgetFetch(
            s(&v, "kind")?,
            s(&v, "key")?.chars().take(64).collect(),
            v.get("params").cloned().unwrap_or(Value::Null),
        ),
        "request_initial_state" => AppEvent::IpcRequestInitialState,
        "autocomplete" => AppEvent::IpcAutocomplete(s(&v, "q")?),
        "find" => AppEvent::IpcFind,
        "zoom" => AppEvent::IpcZoom(match v.get("dir")?.as_i64()? {
            d if d < 0 => -1,
            0 => 0,
            _ => 1,
        }),
        "print" => AppEvent::IpcPrint,
        "devtools" => AppEvent::IpcDevtools,
        "private_window" => AppEvent::IpcNewPrivateWindow,
        "tab_menu" => AppEvent::IpcTabMenu(n(&v, "id")? as usize),
        "reorder_tab" => AppEvent::IpcReorderTab(n(&v, "id")? as usize, n(&v, "index")? as usize),
        "toggle_mute" => AppEvent::IpcToggleMute(n(&v, "id")? as usize),
        "shield_menu" => AppEvent::IpcShieldMenu,
        "download_action" => AppEvent::IpcDownloadAction(s(&v, "op")?, n(&v, "id")?),
        "clear_downloads" => AppEvent::IpcClearDownloads,
        "set_default_browser" => AppEvent::IpcSetDefaultBrowser,
        "list_site_data" => AppEvent::IpcListSiteData,
        "clear_site_data" => AppEvent::IpcClearSiteData(s(&v, "site")?),
        "update_filter_lists" => AppEvent::IpcUpdateFilterLists,
        "allow_http" => AppEvent::IpcAllowHttp(s(&v, "url")?),
        "snap_layouts" => AppEvent::IpcSnapLayouts,
        "minimize" => AppEvent::IpcMinimize,
        "maximize" => AppEvent::IpcMaximize,
        "close" => AppEvent::IpcClose,
        "drag_window" => AppEvent::IpcDragWindow,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_trust_is_exact() {
        let uri = |s: &str| s.parse::<wry::http::Uri>().unwrap();
        assert!(is_trusted_ipc_source(&uri("http://browser.settings/")));
        assert!(is_trusted_ipc_source(&uri("http://browser.downloads/")));
        // bare hosts would resolve via real DNS; lookalikes must not pass
        assert!(!is_trusted_ipc_source(&uri("http://settings/")));
        assert!(!is_trusted_ipc_source(&uri("http://browser.settings.evil.com/")));
        assert!(!is_trusted_ipc_source(&uri("https://evil.com/browser.settings")));
        assert!(!is_trusted_ipc_source(&uri("https://browser.settings/")));
    }

    #[test]
    fn test_icon_and_widget_requests_are_trusted_only() {
        let icons = r#"{"action":"request_favicons","hosts":["yle.fi",7,"a.fi"]}"#;
        assert!(parse(icons, false, Some(1)).is_none());
        assert!(matches!(
            parse(icons, true, None),
            Some(AppEvent::IpcRequestFavicons(h)) if h == ["yle.fi", "a.fi"]
        ));
        let widget = r#"{"action":"widget_fetch","kind":"quote","key":"k1","params":{"symbol":"AAPL"}}"#;
        assert!(parse(widget, false, Some(1)).is_none());
        assert!(matches!(parse(widget, true, None), Some(AppEvent::IpcWidgetFetch(..))));
    }

    #[test]
    fn test_untrusted_pages_are_limited() {
        assert!(parse(r#"{"action":"clear_history"}"#, false, Some(1)).is_none());
        assert!(parse(r#"{"action":"set_default_browser"}"#, false, Some(1)).is_none());
        // Pages can't drive the browser with fake shortcuts; real keys come natively.
        assert!(parse(r#"{"action":"shortcut","key":"private_window"}"#, false, Some(1)).is_none());
        assert!(parse(r#"{"action":"shortcut","key":"close_tab"}"#, false, Some(1)).is_none());
        assert!(matches!(
            parse(r#"{"action":"shortcut","key":"tab_9"}"#, true, None),
            Some(AppEvent::IpcSelectTab(usize::MAX))
        ));
        assert!(parse(r#"{"action":"favicon","url":"javascript:x"}"#, false, Some(1)).is_none());
        assert!(matches!(
            parse(r#"{"action":"zoom","dir":-3}"#, true, None),
            Some(AppEvent::IpcZoom(-1))
        ));
    }

    #[test]
    fn test_shortcut_for_key() {
        let k = |vk: u8, shift| shortcut_for_key(vk as u32, true, shift, false);
        assert!(matches!(k(b'T', false), Some(AppEvent::IpcNewTab(None))));
        assert!(matches!(k(b'T', true), Some(AppEvent::IpcReopenTab)));
        assert!(matches!(k(0x09, true), Some(AppEvent::IpcCycleTab(false))));
        assert!(matches!(k(b'3', false), Some(AppEvent::IpcSelectTab(2))));
        assert!(matches!(k(b'9', false), Some(AppEvent::IpcSelectTab(usize::MAX))));
        assert!(k(b'X', false).is_none());
        // no Ctrl, or AltGr (Ctrl+Alt)
        assert!(shortcut_for_key(b'T' as u32, false, false, false).is_none());
        assert!(shortcut_for_key(b'2' as u32, true, false, true).is_none());
    }
}
