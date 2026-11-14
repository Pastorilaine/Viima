//! Floating video player. The tab that holds the playing video lends its WebView2 to a small
//! frameless always-on-top window (`reparent`); `assets/inject/pip.js` hides the rest of the page
//! and draws the controls over the video. Back to the tab = the webview moves home.
//!
//! One player at a time. The tab never sleeps while it is popped out, and activating, closing
//! or navigating it ends the player first.
//! ponytail: only a top-level `<video>` is found (not shadow DOM, not cross-origin iframes).
use super::*;
use tao::dpi::PhysicalPosition;
use tao::event::WindowEvent;
use tao::event_loop::EventLoopWindowTarget;
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::window::{Icon, ResizeDirection, WindowBuilder, WindowId};
use wry::WebViewExtWindows;

const DEFAULT_W: f64 = 480.0;
const DEFAULT_H_MAX: f64 = 440.0;
const MIN_W: f64 = 320.0;
const MIN_H: f64 = 128.0;
/// Width (CSS px) the page believes it has: sites switch to cramped mobile layouts in a small
/// window, so the page is zoomed out instead and keeps its normal desktop layout.
const VIEW_W: f64 = 1280.0;

/// WebView2 zoom that makes a window `width` (logical px) wide show `VIEW_W` CSS px.
fn page_zoom(width: f64) -> f64 {
    (width / VIEW_W).clamp(0.25, 5.0)
}

/// Runs in the tab: defines `window.__viimaPip` (find / start / stop).
const SCRIPT: &str = include_str!("../../assets/inject/pip.js");

/// Commands the player page may send (see `pip.js`).
pub(super) const UI_COMMANDS: &[&str] = &["back", "close", "gone", "drag", "resize", "ratio"];

pub(super) struct Pip {
    tab_id: usize,
    window: Window,
}

/// "1280x720" from the page's answer (a JSON string), None when it found no video.
fn parse_size(raw: &str) -> Option<(u32, u32)> {
    let s: String = serde_json::from_str(raw).ok()?;
    let (w, h) = s.split_once('x')?;
    let (w, h) = (w.parse().ok()?, h.parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

fn resize_direction(dir: &str) -> Option<ResizeDirection> {
    Some(match dir {
        "n" => ResizeDirection::North,
        "s" => ResizeDirection::South,
        "e" => ResizeDirection::East,
        "w" => ResizeDirection::West,
        "ne" => ResizeDirection::NorthEast,
        "nw" => ResizeDirection::NorthWest,
        "se" => ResizeDirection::SouthEast,
        "sw" => ResizeDirection::SouthWest,
        _ => return None,
    })
}

/// Width:height the player window keeps while it is resized. Extreme shapes are limited so the
/// window stays usable (the picture is letterboxed inside).
fn window_ratio(video: (u32, u32)) -> f64 {
    (video.0 as f64 / video.1 as f64).clamp(0.75, 2.5)
}

/// Window size (logical) that fits the video's shape at a comfortable size.
fn window_size(video: (u32, u32)) -> (f64, f64) {
    let ratio = window_ratio(video);
    if ratio >= 1.0 {
        (DEFAULT_W, (DEFAULT_W / ratio).round())
    } else {
        ((DEFAULT_H_MAX * ratio).round(), DEFAULT_H_MAX)
    }
}

impl BrowserApp {
    pub fn pip_window_id(&self) -> Option<WindowId> {
        self.pip.as_ref().map(|p| p.window.id())
    }

    pub(super) fn pip_tab_id(&self) -> Option<usize> {
        self.pip.as_ref().map(|p| p.tab_id)
    }

    /// Asks tab `id` whether it has a playing video; `AppEvent::PipFound` brings the answer.
    pub(super) fn find_pip_video(&self, id: usize, manual: bool) {
        let Some(tab) = self.tabs.iter().find(|t| t.id == id) else {
            return;
        };
        let Some(webview) = tab.webview.as_ref() else {
            return;
        };
        let proxy = self.proxy.clone();
        let _ = webview.evaluate_script_with_callback(
            &format!("{SCRIPT}\nwindow.__viimaPip.find()"),
            move |res| {
                let _ = proxy.send_event(AppEvent::PipFound {
                    tab: id,
                    manual,
                    size: parse_size(&res),
                });
            },
        );
    }

    /// Ctrl+Shift+P / tab menu: pop the video of tab `id` out, or bring it back.
    pub(super) fn toggle_pip(&mut self, id: usize) {
        match self.pip_tab_id() {
            Some(open) if open == id => self.close_pip(true),
            Some(_) => {
                self.close_pip(false);
                self.find_pip_video(id, true);
            }
            None => self.find_pip_video(id, true),
        }
    }

    pub(super) fn on_pip_found(
        &mut self,
        target: &EventLoopWindowTarget<AppEvent>,
        tab: usize,
        manual: bool,
        size: Option<(u32, u32)>,
    ) {
        let Some(size) = size else { return };
        // Automatic: the user may have come back to the tab while the answer was on its way.
        if self.pip.is_some() || (!manual && tab == self.active_tab_id) {
            return;
        }
        if !self.open_pip(target, tab, size) || tab != self.active_tab_id {
            return;
        }
        // The popped-out tab was the one on screen: show its neighbour instead.
        let idx = self.tabs.iter().position(|t| t.id == tab).unwrap_or(0);
        let other = self
            .tabs
            .get(idx + 1)
            .or_else(|| idx.checked_sub(1).and_then(|i| self.tabs.get(i)))
            .map(|t| t.id);
        match other {
            Some(id) => {
                self.activate(id);
                self.update_ui();
                self.sync_session();
            }
            None => self.open_tab_at(None, None),
        }
    }

    fn open_pip(
        &mut self,
        target: &EventLoopWindowTarget<AppEvent>,
        tab_id: usize,
        video: (u32, u32),
    ) -> bool {
        let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) else {
            return false;
        };
        let Some(webview) = tab.webview.as_ref() else {
            return false;
        };
        let scale = self.window.scale_factor();
        let (w, h) = window_size(video);
        let mut builder = WindowBuilder::new()
            .with_title(format!("{} — Viima", tab.title))
            .with_window_icon(
                Icon::from_rgba(include_bytes!("../../assets/branding/viima_icon_64.rgba").to_vec(), 64, 64).ok(),
            )
            .with_decorations(false)
            .with_undecorated_shadow(true)
            .with_always_on_top(true)
            .with_inner_size(LogicalSize::new(w, h))
            .with_min_inner_size(LogicalSize::new(MIN_W, MIN_H));
        // Bottom-right corner of the screen, clear of the taskbar.
        if let Some(m) = self.window.current_monitor() {
            let (pos, size) = (m.position(), m.size());
            let margin = (24.0 * scale) as i32;
            builder = builder.with_position(PhysicalPosition::new(
                pos.x + size.width as i32 - (w * scale) as i32 - margin,
                pos.y + size.height as i32 - (h * scale) as i32 - margin - (48.0 * scale) as i32,
            ));
        }
        let Ok(window) = builder.build(target) else {
            return false;
        };
        // Move the tab's webview into the new window; the page script then fills it with the video.
        if webview.reparent(window.hwnd()).is_err() {
            return false;
        }
        crate::win_shell::lock_aspect_ratio(window.hwnd(), window_ratio(video));
        self.pip = Some(Pip { tab_id, window });
        self.layout_pip();
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            tab.set_visible(true);
            let labels = serde_json::json!({
                "back": tr("Takaisin välilehteen", "Back to tab"),
                "close": tr("Sulje ja pysäytä", "Close and pause"),
                "play": tr("Toista / tauko", "Play / pause"),
                "mute": tr("Mykistä", "Mute"),
            });
            let zoom = page_zoom(w);
            if let Some(wv) = &tab.webview {
                let _ = wv.zoom(zoom);
            }
            tab.run_script(&format!("window.__viimaPip.start({labels}, {zoom})"));
        }
        true
    }

    /// Ends the player: the webview goes back to the main window (hidden). With `restore` the
    /// tab is shown as well; without it the video keeps playing in the background.
    pub(super) fn close_pip(&mut self, restore: bool) {
        let Some(pip) = self.pip.take() else {
            return;
        };
        let tab_id = pip.tab_id;
        let rect = self.content_rect();
        let home = self.hwnd();
        if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
            tab.run_script("window.__viimaPip&&window.__viimaPip.stop()");
            if let Some(w) = &tab.webview {
                let _ = w.reparent(home);
            }
            tab.set_bounds(rect);
            tab.set_visible(false);
            if let Some(w) = &tab.webview {
                let _ = w.zoom(tab.zoom);
            }
        }
        drop(pip); // only now: the popout window must not take the webview down with it
        if restore {
            self.activate(tab_id);
            self.update_ui();
            self.sync_session();
        }
    }

    fn layout_pip(&self) {
        let Some(pip) = &self.pip else { return };
        let size = pip.window.inner_size().to_logical::<f64>(pip.window.scale_factor());
        if size.width < 1.0 || size.height < 1.0 {
            return;
        }
        if let Some(tab) = self.tabs.iter().find(|t| t.id == pip.tab_id) {
            tab.set_bounds(Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: LogicalSize::new(size.width, size.height).into(),
            });
            let z = page_zoom(size.width);
            if let Some(w) = &tab.webview {
                let _ = w.zoom(z);
            }
            tab.run_script(&format!("window.__viimaPip&&window.__viimaPip.setZoom({z})"));
        }
    }

    pub fn on_pip_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::Resized(_) => self.layout_pip(),
            // The window's own close (Alt+F4): same as the X on the video.
            WindowEvent::CloseRequested => self.pip_ui(self.pip_tab_id().unwrap_or(0), "close", ""),
            _ => {}
        }
    }

    /// A command from the controls drawn in the player (`pip.js`). The sender is a web page, so
    /// it only counts for the tab that is really popped out.
    pub(super) fn pip_ui(&mut self, tab_id: usize, cmd: &str, dir: &str) {
        if self.pip_tab_id() != Some(tab_id) {
            return;
        }
        match cmd {
            "back" => self.close_pip(true),
            "close" => {
                if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                    tab.run_script("(function(){var v=window.__viimaPip&&window.__viimaPip.video();v&&v.pause()})()");
                }
                self.close_pip(false);
            }
            // The page took its video away (navigated, removed it): nothing left to show.
            "gone" => self.close_pip(false),
            // The video changed shape (next video, ad): the window follows, keeping its width.
            "ratio" => {
                let Some((w, h)) = dir.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))) else {
                    return;
                };
                if w == 0 || h == 0 {
                    return;
                }
                let ratio = window_ratio((w, h));
                crate::win_shell::set_aspect_ratio(ratio);
                if let Some(pip) = &self.pip {
                    let width = pip.window.inner_size().to_logical::<f64>(pip.window.scale_factor()).width;
                    pip.window.set_inner_size(LogicalSize::new(width, (width / ratio).round()));
                }
            }
            "drag" => {
                if let Some(pip) = &self.pip {
                    let _ = pip.window.drag_window();
                }
            }
            "resize" => {
                if let (Some(pip), Some(d)) = (&self.pip, resize_direction(dir)) {
                    let _ = pip.window.drag_resize_window(d);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_answer_and_window_shape() {
        assert_eq!(parse_size(r#""1280x720""#), Some((1280, 720)));
        assert_eq!(parse_size(r#""""#), None);
        assert_eq!(parse_size("null"), None);
        assert_eq!(parse_size(r#""0x720""#), None);
        assert_eq!(page_zoom(480.0), 0.375);
        assert_eq!(page_zoom(100.0), 0.25); // WebView2 does not zoom out further
        assert_eq!(window_size((1920, 1080)), (480.0, 270.0));
        // very tall / very wide videos stay inside sane window sizes
        assert_eq!(window_size((100, 1000)), (330.0, 440.0));
        assert_eq!(window_size((4000, 100)), (480.0, 192.0));
        assert!(window_size((100, 1000)).0 >= MIN_W && window_size((4000, 100)).1 >= MIN_H);
    }

    #[test]
    fn test_resize_directions_are_a_fixed_set() {
        for d in ["n", "s", "e", "w", "ne", "nw", "se", "sw"] {
            assert!(resize_direction(d).is_some(), "{d}");
        }
        assert!(resize_direction("x").is_none());
        assert!(UI_COMMANDS.contains(&"resize"));
    }
}
