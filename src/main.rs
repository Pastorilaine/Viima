#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod app;
mod history_db;
mod i18n;
mod storage;
mod tab;
mod updater;
mod url_resolver;
#[cfg(windows)]
mod webview_hooks;
#[cfg(windows)]
mod win_platform;
#[cfg(windows)]
mod win_shell;

use app::{AppEvent, BrowserApp};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::WindowBuilder,
};

/// Command line: `viima.exe [--private] [url | file path ...]`. Internal flags are skipped.
fn parse_cli(args: &[String]) -> (bool, Vec<String>) {
    let mut private = false;
    let mut urls = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
        } else if arg == "--private" || arg == "--incognito" {
            private = true;
        } else if arg == "--cleanup-update" {
            skip_next = true;
        } else if !arg.starts_with("--") && !arg.trim().is_empty() {
            urls.push(cli_target(arg));
        }
    }
    (private, urls)
}

/// A local file (e.g. a double-clicked .html or .pdf) becomes a file:// URL; anything else
/// goes through the omnibox resolver later (URL or search).
fn cli_target(arg: &str) -> String {
    let path = std::path::Path::new(arg);
    if path.is_file()
        && let Ok(abs) = path.canonicalize()
        && let Ok(url) = url::Url::from_file_path(abs.to_string_lossy().trim_start_matches(r"\\?\"))
    {
        return url.to_string();
    }
    arg.to_string()
}

fn main() {
    if updater::handle_cli_args() {
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (private, urls) = parse_cli(&args);

    // A second normal launch (e.g. a link clicked in another app) goes to the open window.
    #[cfg(windows)]
    if !private && win_shell::forward_to_running_instance(&urls.join("\n")) {
        return;
    }

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window_icon = tao::window::Icon::from_rgba(
        include_bytes!("../assets/branding/viima_icon_64.rgba").to_vec(),
        64,
        64,
    )
    .ok();

    let window = WindowBuilder::new()
        .with_title(if private { "Viima (yksityinen)" } else { "Viima" })
        .with_window_icon(window_icon)
        .with_window_classname(if private {
            "ViimaPrivateWindow"
        } else {
            win_shell::MAIN_WINDOW_CLASS
        })
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(640.0, 480.0))
        .with_decorations(false)
        .with_undecorated_shadow(true)
        .with_visible(true)
        .build(&event_loop)
        .expect("Ikkunan luominen epäonnistui");

    window.set_focus();

    #[cfg(target_os = "windows")]
    unsafe {
        use std::ffi::c_void;
        unsafe extern "system" {
            fn ShowWindow(hwnd: *mut c_void, nCmdShow: i32) -> i32;
            fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
            fn BringWindowToTop(hwnd: *mut c_void) -> i32;
        }
        let hwnd_ptr = window.hwnd() as *mut c_void;
        ShowWindow(hwnd_ptr, 1); // SW_SHOWNORMAL
        BringWindowToTop(hwnd_ptr);
        SetForegroundWindow(hwnd_ptr);
    }

    #[cfg(windows)]
    if !private {
        let forward = proxy.clone();
        win_shell::install_copydata_hook(
            window.hwnd(),
            Box::new(move |payload| {
                let urls = payload
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(str::to_string)
                    .collect();
                let _ = forward.send_event(AppEvent::OpenUrls(urls));
            }),
        );
    }

    let window = std::rc::Rc::new(window);
    let mut browser = BrowserApp::new(window, proxy, private, urls);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(app_event) => {
                if browser.handle_event(app_event) {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => browser.on_resize(size),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                browser.flush();
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_cli(&args(&["https://yle.fi", "--private"])),
            (true, vec!["https://yle.fi".to_string()])
        );
        assert_eq!(
            parse_cli(&args(&["--cleanup-update", r"C:\x\Viima-staged.exe"])),
            (false, vec![])
        );
        let (_, urls) = parse_cli(&args(&["Cargo.toml"]));
        assert!(urls[0].starts_with("file:///"), "{urls:?}");
    }
}
