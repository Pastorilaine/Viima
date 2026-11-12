#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod app;
mod storage;
mod tab;
mod updater;
mod url_resolver;
#[cfg(windows)]
mod win_platform;

use app::{AppEvent, BrowserApp};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::WindowBuilder,
};

fn main() {
    if updater::handle_cli_args() {
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
        .with_title("Viima")
        .with_window_icon(window_icon)
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

    let window = std::rc::Rc::new(window);
    let win = window.clone();
    let mut browser = BrowserApp::new(window.clone(), proxy);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(app_event) => match app_event {
                AppEvent::IpcNewTab(url) => {
                    browser.new_tab(url);
                }
                AppEvent::IpcCloseTab(id) => {
                    browser.close_tab(id);
                }
                AppEvent::IpcSwitchTab(id) => {
                    browser.switch_tab(id);
                }
                AppEvent::IpcNavigate(url) => {
                    browser.navigate_active(&url);
                }
                AppEvent::IpcBack => {
                    browser.go_back();
                }
                AppEvent::IpcForward => {
                    browser.go_forward();
                }
                AppEvent::IpcReload => {
                    browser.reload();
                }
                AppEvent::IpcToggleBookmark => {
                    browser.toggle_bookmark();
                }
                AppEvent::IpcRemoveBookmark(url) => {
                    browser.remove_bookmark(&url);
                }
                AppEvent::IpcClearHistory => {
                    browser.clear_history();
                }
                AppEvent::IpcDeleteHistoryItem(url) => {
                    browser.delete_history_item(&url);
                }
                AppEvent::IpcSetSearchEngine(engine) => {
                    browser.set_search_engine(&engine);
                }
                AppEvent::IpcToggleAdblock => {
                    browser.toggle_adblock();
                }
                AppEvent::IpcToggleTheme => {
                    browser.toggle_theme();
                }
                AppEvent::IpcCheckUpdates => {
                    browser.check_for_updates(true);
                }
                AppEvent::IpcDownloadUpdate => {
                    browser.download_update();
                }
                AppEvent::IpcInstallUpdate => {
                    if browser.install_staged_update() {
                        browser.flush();
                        *control_flow = ControlFlow::Exit;
                    }
                }
                AppEvent::IpcToggleAutoUpdate => {
                    browser.toggle_auto_update();
                }
                AppEvent::IpcSetUpdateUrl(url) => {
                    browser.set_update_url(&url);
                }
                AppEvent::IpcRequestInitialState => {
                    browser.update_data();
                }
                AppEvent::IpcCloseActiveTab => {
                    browser.close_active_tab();
                }
                AppEvent::IpcReopenTab => {
                    browser.reopen_closed_tab();
                }
                AppEvent::IpcCycleTab(forward) => {
                    browser.cycle_tab(forward);
                }
                AppEvent::IpcFocusOmnibox => {
                    browser.focus_omnibox();
                }
                AppEvent::IpcMinimize => {
                    win.set_minimized(true);
                }
                AppEvent::IpcMaximize => {
                    let is_max = win.is_maximized();
                    win.set_maximized(!is_max);
                }
                AppEvent::IpcClose => {
                    browser.flush();
                    *control_flow = ControlFlow::Exit;
                }
                AppEvent::IpcDragWindow => {
                    let _ = win.drag_window();
                }
                AppEvent::TabTitleChanged(id, title) => {
                    browser.on_title_changed(id, title);
                }
                AppEvent::TabPageLoadStarted(id, url) => {
                    browser.on_page_started(id, url);
                }
                AppEvent::TabPageLoadFinished(id, url) => {
                    browser.on_page_finished(id, url);
                }
                AppEvent::UpdateCheckFinished { manual, result } => {
                    browser.on_update_check_finished(manual, result);
                }
                AppEvent::UpdateDownloadFinished(result) => {
                    browser.on_update_download_finished(result);
                }
            },

            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                browser.on_resize(size);
            }

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
