//! Site icons for the tab strip, bookmarks bar and start page.
//!
//! Fetched here instead of by the pages: guessing `/favicon.ico` fails for many sites, and a
//! disk cache lets restored tabs and bookmarks show their icon right away. Icons travel to the
//! UI as `data:` URLs, so a page can never be made to load an arbitrary address through them.
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

const MAX_ICON_BYTES: usize = 64 * 1024;
const MAX_HTML_BYTES: usize = 256 * 1024;
const MAX_ENTRIES: usize = 400;

/// host -> `data:` URL. Persisted unless this is a private window.
pub struct FaviconCache {
    map: HashMap<String, String>,
    /// Hosts with no usable icon this session, and hosts being fetched right now.
    failed: HashSet<String>,
    pending: HashSet<String>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl FaviconCache {
    pub fn load(private: bool) -> Self {
        let path = (!private).then(|| crate::storage::config_dir().join("favicons.json"));
        let map = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<HashMap<String, String>>(&text).ok())
            .unwrap_or_default();
        Self { map, failed: HashSet::new(), pending: HashSet::new(), path, dirty: false }
    }

    pub fn get(&self, host: &str) -> Option<&String> {
        self.map.get(host)
    }

    /// Hosts that still need fetching; they are marked pending so nothing is fetched twice.
    pub fn claim_missing(&mut self, hosts: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for h in hosts {
            if valid_host(h)
                && !self.map.contains_key(h)
                && !self.failed.contains(h)
                && self.pending.insert(h.clone())
            {
                out.push(h.clone());
            }
        }
        out
    }

    pub fn finish(&mut self, host: &str, icon: Option<String>) {
        self.pending.remove(host);
        match icon {
            Some(data) => {
                self.failed.remove(host);
                if self.map.len() >= MAX_ENTRIES
                    && let Some(old) = self.map.keys().next().cloned()
                {
                    self.map.remove(&old);
                }
                self.map.insert(host.to_string(), data);
                self.dirty = true;
            }
            None => {
                self.failed.insert(host.to_string());
            }
        }
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        if let Some(path) = &self.path
            && let Ok(json) = serde_json::to_string(&self.map)
        {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
        self.dirty = false;
    }
}

/// A plausible public DNS name (no IP literals, no single-label hosts like `localhost`).
pub fn valid_host(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && !host.starts_with("browser.") // the browser's own pages (http://browser.newtab/ ...)
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && host.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'-')
        && host.bytes().any(|c| c.is_ascii_lowercase())
}

fn is_global(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || (o[0] == 100 && (o[1] & 0xC0) == 64)) // 100.64.0.0/10 (CGNAT)
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_global(IpAddr::V4(v4));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xFE00) == 0xFC00 // fc00::/7 unique local
                || (s[0] & 0xFFC0) == 0xFE80) // fe80::/10 link local
        }
    }
}

/// Icons are only fetched from public hosts, never from the user's own network.
fn is_public(host: &str) -> bool {
    match (host, 443).to_socket_addrs() {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            !addrs.is_empty() && addrs.iter().all(|a| is_global(a.ip()))
        }
        Err(_) => false,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Option<ureq::http::Response<ureq::Body>> {
    agent
        .get(url)
        .header(
            "User-Agent",
            &format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Viima/{}",
                crate::updater::CURRENT_VERSION
            ),
        )
        .call()
        .ok()
}

/// A whole file, or None when it is bigger than `limit` (half an image is no use).
fn get_bytes(agent: &ureq::Agent, url: &str, limit: u64) -> Option<Vec<u8>> {
    get(agent, url)?.body_mut().with_config().limit(limit).read_to_vec().ok()
}

/// Just the start of a page: the icon links live in `<head>`, and pages can be megabytes.
fn get_head(agent: &ureq::Agent, url: &str, max: u64) -> Option<String> {
    use std::io::Read;
    let mut resp = get(agent, url)?;
    let mut buf = Vec::new();
    resp.body_mut().as_reader().take(max).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Icon at an exact address (reported by the page itself), as a `data:` URL.
pub fn fetch_icon(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    if !valid_host(&host) || !is_public(&host) {
        return None;
    }
    let bytes = get_bytes(&agent(), url, MAX_ICON_BYTES as u64)?;
    to_data_url(&bytes)
}

/// Finds a site's icon: the page's own `<link rel=icon>`, then `/favicon.ico`.
pub fn fetch_for_host(host: &str) -> Option<String> {
    if !valid_host(host) || !is_public(host) {
        return None;
    }
    let agent = agent();
    let base = url::Url::parse(&format!("https://{host}/")).ok()?;
    let mut candidates: Vec<String> = Vec::new();
    if let Some(html) = get_head(&agent, base.as_str(), MAX_HTML_BYTES as u64) {
        candidates.extend(
            parse_icon_links(&html)
                .iter()
                .filter_map(|href| base.join(href).ok())
                .map(String::from),
        );
    }
    candidates.push(format!("https://{host}/favicon.ico"));
    for url in candidates.iter().take(5) {
        if let Some(icon) = fetch_icon(url) {
            return Some(icon);
        }
    }
    None
}

/// `href`s of the icon links in a page, best first. Naive on purpose: only `<link>` tags are read.
pub fn parse_icon_links(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut found: Vec<(u8, String)> = Vec::new();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<link") {
        let start = from + i + 5;
        let Some(len) = lower[start..].find('>') else { break };
        from = start + len;
        let attrs = parse_attrs(&html[start..start + len]);
        let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        let (Some(rel), Some(href)) = (get("rel"), get("href")) else { continue };
        let rel = rel.to_ascii_lowercase();
        let tokens: Vec<&str> = rel.split_whitespace().collect();
        if href.trim().is_empty() || tokens.contains(&"mask-icon") {
            continue;
        }
        let sizes = get("sizes").unwrap_or("").to_ascii_lowercase();
        let nice = get("type").is_some_and(|t| t.contains("svg"))
            || ["32", "48", "64", "any"].iter().any(|s| sizes.contains(s));
        let score = if tokens.contains(&"icon") {
            if nice { 5 } else { 10 }
        } else if tokens.iter().any(|t| t.starts_with("apple-touch-icon")) {
            20
        } else {
            continue;
        };
        found.push((score, href.trim().to_string()));
    }
    found.sort_by_key(|(score, _)| *score);
    let mut seen = HashSet::new();
    found
        .into_iter()
        .map(|(_, href)| href)
        .filter(|h| seen.insert(h.clone()))
        .collect()
}

/// `name="value"` pairs of one tag, names lowercased.
fn parse_attrs(tag: &str) -> Vec<(String, String)> {
    let b = tag.as_bytes();
    let (mut i, mut out) = (0, Vec::new());
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let name_start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        let name = tag[name_start..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let value = if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let quote = b[i];
                let s = i + 1;
                i = s;
                while i < b.len() && b[i] != quote {
                    i += 1;
                }
                let v = &tag[s..i];
                i += 1;
                v
            } else {
                let s = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                &tag[s..i]
            };
            if !name.is_empty() {
                out.push((name, value.to_string()));
            }
        }
    }
    out
}

/// Image bytes -> `data:` URL, by sniffing the content (servers send wrong types for icons).
fn to_data_url(bytes: &[u8]) -> Option<String> {
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0, 0, 1, 0]) {
        "image/x-icon"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.len() > 10
        && String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).contains("<svg")
    {
        "image/svg+xml"
    } else {
        return None;
    };
    (bytes.len() <= MAX_ICON_BYTES).then(|| format!("data:{mime};base64,{}", base64(bytes)))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_valid_host() {
        assert!(valid_host("yle.fi"));
        assert!(valid_host("www.is.fi"));
        assert!(!valid_host("localhost"));
        assert!(!valid_host("browser.newtab"));
        assert!(!valid_host("192.168.1.1"));
        assert!(!valid_host("Yle.fi"));
        assert!(!valid_host("a b.fi"));
        assert!(!valid_host(".fi"));
    }

    #[test]
    fn test_is_global() {
        assert!(is_global("8.8.8.8".parse().unwrap()));
        assert!(!is_global("127.0.0.1".parse().unwrap()));
        assert!(!is_global("192.168.0.5".parse().unwrap()));
        assert!(!is_global("100.64.1.1".parse().unwrap()));
        assert!(!is_global("::1".parse().unwrap()));
        assert!(!is_global("fd00::1".parse().unwrap()));
        assert!(!is_global("::ffff:10.0.0.1".parse().unwrap()));
        assert!(is_global("2606:4700::1111".parse().unwrap()));
    }

    #[test]
    fn test_parse_icon_links() {
        let html = r#"<html><head>
            <link rel="apple-touch-icon" href="/apple.png">
            <LINK REL="mask-icon" href="/mask.svg">
            <link rel="shortcut icon" href='/small.ico' sizes="16x16">
            <link href=/best.png rel=icon sizes=32x32>
            <link rel="stylesheet" href="/x.css">
            <link rel="icon" href="/small.ico">
            </head>"#;
        assert_eq!(parse_icon_links(html), vec!["/best.png", "/small.ico", "/apple.png"]);
    }

    #[test]
    fn test_to_data_url_sniffs_content() {
        let png = b"\x89PNG\r\n\x1a\nxxxx";
        assert!(to_data_url(png).unwrap().starts_with("data:image/png;base64,"));
        assert!(to_data_url(b"<!DOCTYPE html><html>").is_none());
        assert!(
            to_data_url(b"<?xml version=\"1.0\"?><svg xmlns=\"x\"/>")
                .unwrap()
                .starts_with("data:image/svg+xml;")
        );
        assert!(to_data_url(&vec![0u8; MAX_ICON_BYTES + 1]).is_none());
    }

    #[test]
    fn test_cache_claims_each_host_once() {
        let mut c = FaviconCache {
            map: HashMap::new(),
            failed: HashSet::new(),
            pending: HashSet::new(),
            path: None,
            dirty: false,
        };
        let hosts = vec!["yle.fi".to_string(), "localhost".to_string()];
        assert_eq!(c.claim_missing(&hosts), vec!["yle.fi"]);
        assert!(c.claim_missing(&hosts).is_empty());
        c.finish("yle.fi", Some("data:x".into()));
        assert_eq!(c.get("yle.fi").map(String::as_str), Some("data:x"));
        assert!(c.claim_missing(&hosts).is_empty());
    }
}
