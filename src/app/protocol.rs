//! `browser://` scheme: serves the chrome UI and internal pages from the binary.
use super::{DARK_MODE, PRIVATE_MODE, theme_name};
use crate::updater;
use std::borrow::Cow;
use std::sync::atomic::Ordering;
use wry::http::{Request, Response};

fn asset(host: &str, path: &str) -> Option<(&'static str, &'static [u8])> {
    const HTML: &str = "text/html; charset=utf-8";
    const JS: &str = "application/javascript; charset=utf-8";
    // Shared by every page
    match path {
        "/viima-logo.png" => {
            return Some((
                "image/png",
                include_bytes!("../../assets/branding/viima-logo.png"),
            ));
        }
        "/i18n.js" => return Some((JS, include_bytes!("../../assets/i18n.js"))),
        _ => {}
    }
    let page = host.strip_prefix("browser.").unwrap_or(host);
    Some(match (page, path) {
        ("ui", "/style.css") => (
            "text/css; charset=utf-8",
            include_bytes!("../../assets/ui/style.css"),
        ),
        ("ui", "/app.js") => (JS, include_bytes!("../../assets/ui/app.js")),
        ("ui", _) => (HTML, include_bytes!("../../assets/ui/index.html")),
        ("newtab", "/wallpapers/aurora-dark.jpg") => (
            "image/jpeg",
            include_bytes!("../../assets/internal/wallpapers/aurora-dark.jpg"),
        ),
        ("newtab", "/wallpapers/glass-waves.jpg") => (
            "image/jpeg",
            include_bytes!("../../assets/internal/wallpapers/glass-waves.jpg"),
        ),
        ("newtab", "/wallpapers/nordic-light.jpg") => (
            "image/jpeg",
            include_bytes!("../../assets/internal/wallpapers/nordic-light.jpg"),
        ),
        ("newtab", _) => (HTML, include_bytes!("../../assets/internal/newtab.html")),
        ("history", _) => (HTML, include_bytes!("../../assets/internal/history.html")),
        ("bookmarks", _) => (
            HTML,
            include_bytes!("../../assets/internal/bookmarks.html"),
        ),
        ("settings", _) => (HTML, include_bytes!("../../assets/internal/settings.html")),
        ("downloads", _) => (
            HTML,
            include_bytes!("../../assets/internal/downloads.html"),
        ),
        ("insecure", _) => (HTML, include_bytes!("../../assets/internal/insecure.html")),
        _ => return None,
    })
}

pub fn handle_protocol(_id: &str, req: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let uri = req.uri();
    let (content_type, raw_bytes) = asset(uri.host().unwrap_or(""), uri.path()).unwrap_or((
        "text/html; charset=utf-8",
        b"<!DOCTYPE html><html><body><h1>404</h1></body></html>",
    ));

    let body: Cow<'static, [u8]> = if content_type.starts_with("text/html") {
        let html = std::str::from_utf8(raw_bytes).unwrap_or("");
        // Theme, language and private mode are stamped in so pages render right on first paint.
        let attr = format!(
            "<html lang=\"{}\" data-theme=\"{}\"{}>",
            crate::i18n::lang(),
            theme_name(DARK_MODE.load(Ordering::Relaxed)),
            if PRIVATE_MODE.load(Ordering::Relaxed) {
                " data-private=\"1\""
            } else {
                ""
            }
        );
        let themed = html
            .replacen("<html lang=\"fi\">", &attr, 1)
            .replace("{{VERSION}}", updater::CURRENT_VERSION);
        Cow::Owned(themed.into_bytes())
    } else {
        Cow::Borrowed(raw_bytes)
    };

    // Internal pages hold privileged IPC: never let a web page frame them (clickjacking).
    let mut builder = Response::builder()
        .header("Content-Type", content_type)
        .header("X-Frame-Options", "DENY")
        .header("Content-Security-Policy", "frame-ancestors 'none'")
        .header("X-Content-Type-Options", "nosniff");
    if content_type.starts_with("text/html") || content_type.starts_with("application/javascript")
    {
        builder = builder.header("Cache-Control", "no-store");
    } else if content_type.starts_with("image/") {
        builder = builder.header("Cache-Control", "public, max-age=86400");
    }
    builder.body(body).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_internal_page_is_served() {
        for page in crate::storage::INTERNAL_PAGES {
            let (ct, _) = asset(&format!("browser.{page}"), "/").unwrap();
            assert!(ct.starts_with("text/html"), "{page}");
        }
        assert!(asset("browser.settings", "/i18n.js").is_some());
        assert!(asset("browser.evil", "/").is_none());
    }
}
