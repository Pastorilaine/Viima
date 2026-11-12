use std::net::{Ipv4Addr, Ipv6Addr};

pub struct UrlResolver;

impl UrlResolver {
    pub fn resolve(input: &str, default_engine: &str) -> String {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return "browser://newtab".to_string();
        }

        // Direct internal or supported schemes (case-insensitive scheme prefix)
        for scheme in ["browser://", "http://", "https://", "file://", "about:"] {
            if let Some(rest) = trimmed
                .get(..scheme.len())
                .filter(|p| p.eq_ignore_ascii_case(scheme))
                .map(|_| &trimmed[scheme.len()..])
            {
                return format!("{scheme}{rest}");
            }
        }

        // Bang shortcuts
        if let Some(resolved_bang) = Self::resolve_bang(trimmed) {
            return resolved_bang;
        }

        // Check if looks like a valid host / domain
        if let Some(use_http) = Self::classify_url(trimmed) {
            let prefix = if use_http { "http://" } else { "https://" };
            return format!("{prefix}{trimmed}");
        }

        // Fallback to search query
        Self::format_search_query(trimmed, default_engine)
    }

    fn resolve_bang(input: &str) -> Option<String> {
        if !input.starts_with('!') {
            return None;
        }
        let (bang, query) = match input.split_once(' ') {
            Some((b, q)) => (b, q.trim()),
            None => (input, ""),
        };
        let encoded_query = urlencoding_simple(query);

        match bang.to_ascii_lowercase().as_str() {
            "!g" => Some(format!("https://www.google.com/search?q={encoded_query}")),
            "!d" | "!ddg" => Some(format!("https://duckduckgo.com/?q={encoded_query}")),
            "!b" | "!bing" => Some(format!("https://www.bing.com/search?q={encoded_query}")),
            "!w" => Some(format!(
                "https://fi.wikipedia.org/wiki/Special:Search?search={encoded_query}"
            )),
            "!wen" => Some(format!(
                "https://en.wikipedia.org/wiki/Special:Search?search={encoded_query}"
            )),
            "!yt" => Some(format!(
                "https://www.youtube.com/results?search_query={encoded_query}"
            )),
            "!gh" => Some(format!("https://github.com/search?q={encoded_query}")),
            "!yle" => Some(format!("https://haku.yle.fi/?query={encoded_query}")),
            "!r" | "!reddit" => Some(format!("https://www.reddit.com/search/?q={encoded_query}")),
            _ => None,
        }
    }

    /// Returns `Some(use_http)` if `input` looks like a direct host/URL, or `None` if it is a search query.
    fn classify_url(input: &str) -> Option<bool> {
        if input.chars().any(char::is_whitespace) {
            return None;
        }

        // Authority is everything before path/query/fragment.
        let authority = input.split(['/', '?', '#']).next().unwrap_or("");
        if authority.is_empty() || authority.contains('@') {
            return None;
        }

        // Handle bracketed IPv6 literal, e.g. `[::1]` or `[::1]:8080`.
        if let Some(after_open) = authority.strip_prefix('[') {
            let (addr_str, port_part) = after_open.split_once(']')?;
            let addr: Ipv6Addr = addr_str.parse().ok()?;
            if !port_part.is_empty() {
                let port = port_part.strip_prefix(':')?;
                if port.is_empty() || port.parse::<u16>().is_err() {
                    return None;
                }
            }
            return Some(addr.is_loopback());
        }

        let host = match authority.split_once(':') {
            Some((h, port)) => {
                if port.is_empty() || port.parse::<u16>().is_err() {
                    return None;
                }
                h
            }
            None => authority,
        };

        if host.eq_ignore_ascii_case("localhost") {
            return Some(true);
        }

        if let Ok(ip) = host.parse::<Ipv4Addr>() {
            return Some(ip.is_loopback() || ip.is_private());
        }

        if !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
            return None;
        }

        let mut labels = host.split('.');
        let last = labels.next_back()?;
        // TLD must be >= 2 alphabetic characters or valid punycode (`xn--...`).
        let valid_tld = (last.len() >= 2 && last.chars().all(|c| c.is_ascii_alphabetic()))
            || (last.len() > 4
                && last[..4].eq_ignore_ascii_case("xn--")
                && last[4..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        if !valid_tld {
            return None;
        }

        let valid_labels = host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
        });

        valid_labels.then_some(false)
    }

    pub fn format_search_query(query: &str, engine: &str) -> String {
        let encoded = urlencoding_simple(query);
        match engine {
            "duckduckgo" => format!("https://duckduckgo.com/?q={encoded}"),
            "bing" => format!("https://www.bing.com/search?q={encoded}"),
            "wikipedia" => format!("https://fi.wikipedia.org/wiki/Special:Search?search={encoded}"),
            _ => format!("https://www.google.com/search?q={encoded}"),
        }
    }
}

fn urlencoding_simple(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push(HEX[(b >> 4) as usize] as char);
                encoded.push(HEX[(b & 0x0F) as usize] as char);
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urls() {
        assert_eq!(UrlResolver::resolve("google.com", "duckduckgo"), "https://google.com");
        assert_eq!(UrlResolver::resolve("example.com:8080/api", "duckduckgo"), "https://example.com:8080/api");
        assert_eq!(UrlResolver::resolve("localhost:3000", "duckduckgo"), "http://localhost:3000");
        assert_eq!(UrlResolver::resolve("localhost.evil.com", "duckduckgo"), "https://localhost.evil.com");
        assert_eq!(UrlResolver::resolve("192.168.1.1", "duckduckgo"), "http://192.168.1.1");
        assert_eq!(UrlResolver::resolve("[::1]:8080", "duckduckgo"), "http://[::1]:8080");
        assert_eq!(UrlResolver::resolve("https://yle.fi", "duckduckgo"), "https://yle.fi");
        assert_eq!(UrlResolver::resolve("HTTPS://Yle.fi/Uutiset", "duckduckgo"), "https://Yle.fi/Uutiset");
        assert_eq!(UrlResolver::resolve("!g rust", "duckduckgo"), "https://www.google.com/search?q=rust");
        assert_eq!(UrlResolver::resolve("!G rust", "duckduckgo"), "https://www.google.com/search?q=rust");
        assert_eq!(UrlResolver::resolve("rust tutorial", "duckduckgo"), "https://duckduckgo.com/?q=rust+tutorial");
        // Numbers, versions, emails and malformed dots should search, not navigate
        assert_eq!(UrlResolver::resolve("3.14", "duckduckgo"), "https://duckduckgo.com/?q=3.14");
        assert_eq!(UrlResolver::resolve("v1.2.3", "duckduckgo"), "https://duckduckgo.com/?q=v1.2.3");
        assert_eq!(UrlResolver::resolve("user@example.com", "duckduckgo"), "https://duckduckgo.com/?q=user%40example.com");
        assert_eq!(UrlResolver::resolve("foo..bar.com", "duckduckgo"), "https://duckduckgo.com/?q=foo..bar.com");
    }
}
