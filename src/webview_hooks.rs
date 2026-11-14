//! WebView2 features wry doesn't expose: downloads with progress, audio/mute, zoom tracking,
//! failed-navigation detection (HTTPS-only fallback) and DevTools protocol calls (site data).
#![cfg(windows)]

use crate::app::AppEvent;
use crate::storage::{DownloadItem, now_secs};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, BytesReceivedChangedEventHandler, CallDevToolsProtocolMethodCompletedHandler,
    DownloadStartingEventHandler, IsDocumentPlayingAudioChangedEventHandler,
    NavigationCompletedEventHandler, StateChangedEventHandler, ZoomFactorChangedEventHandler,
    take_pwstr,
};
use windows::core::{BOOL, HSTRING, Interface, PWSTR};
use wry::{WebView, WebViewExtWindows};

/// Live download operations (for cancel), keyed by `DownloadItem::id`.
pub type DownloadOps = Rc<RefCell<HashMap<u64, ICoreWebView2DownloadOperation>>>;

static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(0);

fn next_download_id() -> u64 {
    // Unique across restarts (the list is persisted): millisecond clock + sequence.
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    ms * 100 + NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed) % 100
}

fn download_snapshot(op: &ICoreWebView2DownloadOperation, base: &DownloadItem) -> DownloadItem {
    let mut item = base.clone();
    unsafe {
        let mut received = 0i64;
        let _ = op.BytesReceived(&mut received);
        item.received = received;
        let mut total = 0i64;
        if op.TotalBytesToReceive(&mut total).is_ok() && total > 0 {
            item.total = total;
        }
        let mut path = PWSTR::null();
        if op.ResultFilePath(&mut path).is_ok() {
            let p = take_pwstr(path);
            if !p.is_empty() {
                item.file_name = std::path::Path::new(&p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                item.path = p;
            }
        }
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
        let _ = op.State(&mut state);
        item.state = match state {
            COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => "completed",
            COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED => "interrupted",
            _ => "in_progress",
        }
        .into();
    }
    item
}

/// Hooks one tab's webview. `https_upgraded` holds the http:// URL the HTTPS-only mode
/// rewrote, so a failed https load can fall back to the warning page.
pub fn install(
    webview: &WebView,
    tab_id: usize,
    proxy: EventLoopProxy<AppEvent>,
    downloads: DownloadOps,
    https_upgraded: Rc<RefCell<Option<String>>>,
) -> windows::core::Result<()> {
    let core = webview.webview();
    let controller = webview.controller();
    let mut token = 0i64;
    unsafe {
        // Audio indicator
        if let Ok(wv8) = core.cast::<ICoreWebView2_8>() {
            let p = proxy.clone();
            wv8.add_IsDocumentPlayingAudioChanged(
                &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                    if let Some(wv) = sender.and_then(|s| s.cast::<ICoreWebView2_8>().ok()) {
                        let mut playing = BOOL::default();
                        wv.IsDocumentPlayingAudio(&mut playing)?;
                        let _ = p.send_event(AppEvent::TabAudio(tab_id, playing.as_bool()));
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }

        // Browser shortcuts (Ctrl+T, Ctrl+W, Ctrl+Tab...): real key presses only.
        let p = proxy.clone();
        controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                use windows::Win32::UI::Input::KeyboardAndMouse::{
                    GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
                };
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN {
                    return Ok(());
                }
                let mut vk = 0u32;
                args.VirtualKey(&mut vk)?;
                let down = |k: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| {
                    GetKeyState(k.0 as i32) < 0
                };
                if let Some(event) = crate::app::ipc::shortcut_for_key(
                    vk,
                    down(VK_CONTROL),
                    down(VK_SHIFT),
                    down(VK_MENU),
                ) {
                    args.SetHandled(true)?;
                    let _ = p.send_event(event);
                }
                Ok(())
            })),
            &mut token,
        )?;

        // Zoom (Ctrl +/-/0, Ctrl+wheel are handled natively; we only remember the level)
        let p = proxy.clone();
        controller.add_ZoomFactorChanged(
            &ZoomFactorChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(c) = sender {
                    let mut f = 1.0f64;
                    c.ZoomFactor(&mut f)?;
                    let _ = p.send_event(AppEvent::TabZoom(tab_id, f));
                }
                Ok(())
            })),
            &mut token,
        )?;

        // HTTPS-only fallback: the upgraded https:// load failed -> show the warning page.
        let p = proxy.clone();
        core.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut ok = BOOL::default();
                args.IsSuccess(&mut ok)?;
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                let _ = args.WebErrorStatus(&mut status);
                if ok.as_bool() {
                    https_upgraded.borrow_mut().take();
                } else if status != COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED
                    && let Some(http_url) = https_upgraded.borrow_mut().take()
                {
                    let _ = p.send_event(AppEvent::HttpsFailed(tab_id, http_url));
                }
                Ok(())
            })),
            &mut token,
        )?;

        // Downloads: our own list + progress instead of the Edge flyout.
        if let Ok(wv4) = core.cast::<ICoreWebView2_4>() {
            let p = proxy.clone();
            wv4.add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let op = args.DownloadOperation()?;
                    args.SetHandled(true)?;
                    let mut uri = PWSTR::null();
                    let _ = op.Uri(&mut uri);
                    let base = DownloadItem {
                        id: next_download_id(),
                        url: take_pwstr(uri),
                        started_at: now_secs(),
                        ..Default::default()
                    };
                    let id = base.id;
                    let _ = p.send_event(AppEvent::DownloadUpdate(download_snapshot(&op, &base)));
                    downloads.borrow_mut().insert(id, op.clone());

                    let (p2, base2) = (p.clone(), base.clone());
                    let mut t = 0i64;
                    op.add_BytesReceivedChanged(
                        &BytesReceivedChangedEventHandler::create(Box::new(move |sender, _| {
                            if let Some(op) = sender {
                                let _ = p2.send_event(AppEvent::DownloadUpdate(download_snapshot(
                                    &op, &base2,
                                )));
                            }
                            Ok(())
                        })),
                        &mut t,
                    )?;
                    let (p3, ops) = (p.clone(), downloads.clone());
                    op.add_StateChanged(
                        &StateChangedEventHandler::create(Box::new(move |sender, _| {
                            if let Some(op) = sender {
                                let item = download_snapshot(&op, &base);
                                if item.state != "in_progress" {
                                    ops.borrow_mut().remove(&item.id);
                                }
                                let _ = p3.send_event(AppEvent::DownloadUpdate(item));
                            }
                            Ok(())
                        })),
                        &mut t,
                    )?;
                    Ok(())
                })),
                &mut token,
            )?;
        }
    }
    Ok(())
}

pub fn set_muted(webview: &WebView, muted: bool) {
    if let Ok(wv8) = webview.webview().cast::<ICoreWebView2_8>() {
        unsafe {
            let _ = wv8.SetIsMuted(muted);
        }
    }
}

pub fn set_devtools_enabled(webview: &WebView, enabled: bool) {
    unsafe {
        if let Ok(settings) = webview.webview().Settings() {
            let _ = settings.SetAreDevToolsEnabled(enabled);
        }
    }
}

/// Calls a Chrome DevTools Protocol method; `done` gets the JSON result (UI thread).
fn devtools_call(
    core: &ICoreWebView2,
    method: &str,
    params: &serde_json::Value,
    done: impl FnOnce(Option<serde_json::Value>) + 'static,
) {
    unsafe {
        let _ = core.CallDevToolsProtocolMethod(
            &HSTRING::from(method),
            &HSTRING::from(params.to_string()),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, json| {
                done(res.ok().and_then(|_| serde_json::from_str(&json).ok()));
                Ok(())
            })),
        );
    }
}

fn cookie_site(domain: &str) -> String {
    let d = domain.trim_start_matches('.');
    d.strip_prefix("www.").unwrap_or(d).to_string()
}

/// Sends `AppEvent::SiteData` with (site, cookie count), most cookies first.
pub fn list_cookie_sites(webview: &WebView, proxy: EventLoopProxy<AppEvent>) {
    devtools_call(
        &webview.webview(),
        "Network.getAllCookies",
        &serde_json::json!({}),
        move |res| {
            let mut counts: HashMap<String, u32> = HashMap::new();
            if let Some(cookies) = res.as_ref().and_then(|v| v["cookies"].as_array()) {
                for c in cookies {
                    if let Some(domain) = c["domain"].as_str() {
                        *counts.entry(cookie_site(domain)).or_default() += 1;
                    }
                }
            }
            let mut list: Vec<(String, u32)> = counts.into_iter().collect();
            list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let _ = proxy.send_event(AppEvent::SiteData(list));
        },
    );
}

/// Deletes cookies and storage (localStorage, IndexedDB, cache, service workers) of one site
/// and its subdomains, then sends `AppEvent::SiteDataCleared`.
pub fn clear_site_data(webview: &WebView, site: &str, proxy: EventLoopProxy<AppEvent>) {
    let core = webview.webview();
    for origin in [
        format!("https://{site}"),
        format!("https://www.{site}"),
        format!("http://{site}"),
        format!("http://www.{site}"),
    ] {
        devtools_call(
            &core,
            "Storage.clearDataForOrigin",
            &serde_json::json!({ "origin": origin, "storageTypes": "all" }),
            |_| {},
        );
    }
    let site = site.to_string();
    let core2 = core.clone();
    devtools_call(&core, "Network.getAllCookies", &serde_json::json!({}), move |res| {
        if let Some(cookies) = res.as_ref().and_then(|v| v["cookies"].as_array()) {
            for c in cookies {
                let (Some(name), Some(domain)) = (c["name"].as_str(), c["domain"].as_str()) else {
                    continue;
                };
                let d = cookie_site(domain);
                if d == site || d.ends_with(&format!(".{site}")) {
                    devtools_call(
                        &core2,
                        "Network.deleteCookies",
                        &serde_json::json!({ "name": name, "domain": domain, "path": c["path"] }),
                        |_| {},
                    );
                }
            }
        }
        let _ = proxy.send_event(AppEvent::SiteDataCleared(site));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_site_and_ids() {
        assert_eq!(cookie_site(".www.yle.fi"), "yle.fi");
        assert_eq!(cookie_site("accounts.google.com"), "accounts.google.com");
        assert_ne!(next_download_id(), next_download_id());
    }
}
