use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

pub struct AdBlocker;

/// Domains from the downloaded filter lists (EasyList & co.), on top of `BLOCKED_DOMAINS`.
static EXTRA_DOMAINS: LazyLock<RwLock<HashSet<String>>> = LazyLock::new(Default::default);

const FILTER_LISTS: &[&str] = &[
    "https://easylist.to/easylist/easylist.txt",
    "https://easylist.to/easylist/easyprivacy.txt",
    "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=nohtml&showintro=0&mimetype=plaintext",
];

/// How often the filter lists are refreshed.
pub const LIST_MAX_AGE_SECS: u64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct BlockedCounts {
    pub ads: u32,
    pub trackers: u32,
    pub scripts: u32,
}

impl BlockedCounts {
    pub fn total(self) -> u32 {
        self.ads + self.trackers + self.scripts
    }
}

impl AdBlocker {
    // Sorted alphabetically for zero-allocation binary search in `should_block_url`.
    pub const BLOCKED_DOMAINS: &'static [&'static str] = &[
        "2mdn.net",
        "33across.com",
        "3lift.com",
        "adform.net",
        "adition.com",
        "adnxs.com",
        "adroll.com",
        "ads-twitter.com",
        "adsafeprotected.com",
        "adservice.google.com",
        "adsrvr.org",
        "adsystem.com",
        "adtech.de",
        "advertising.amazon.com",
        "amazon-adsystem.com",
        "analytics.google.com",
        "bidswitch.net",
        "casalemedia.com",
        "chartbeat.net",
        "clarity.ms",
        "connect.facebook.net",
        "contextweb.com",
        "criteo.com",
        "criteo.net",
        "doubleclick.net",
        "doubleverify.com",
        "google-analytics.com",
        "googleadservices.com",
        "googlesyndication.com",
        "googletagmanager.com",
        "googletagservices.com",
        "hotjar.com",
        "indexww.com",
        "lijit.com",
        "media.net",
        "mgid.com",
        "moatads.com",
        "openx.net",
        "outbrain.com",
        "pagead2.googlesyndication.com",
        "pixel.facebook.com",
        "pubmatic.com",
        "quantserve.com",
        "revcontent.com",
        "rubiconproject.com",
        "scorecardresearch.com",
        "sharethrough.com",
        "smartadserver.com",
        "sovrn.com",
        "taboola.com",
        "teads.tv",
        "yieldmo.com",
        "zemanta.com",
    ];

    /// Analytics/tracking hosts among the built-in list, for the "trackers" count.
    const TRACKER_DOMAINS: &'static [&'static str] = &[
        "analytics.google.com",
        "chartbeat.net",
        "clarity.ms",
        "connect.facebook.net",
        "google-analytics.com",
        "googletagmanager.com",
        "hotjar.com",
        "pixel.facebook.com",
        "quantserve.com",
        "scorecardresearch.com",
    ];

    fn host_of(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        // `url::Url` normalizes ASCII domains to lowercase, so we only need to trim trailing dots.
        parsed
            .host_str()
            .map(|h| h.trim_end_matches('.').to_string())
    }

    /// Checks `host` and each parent domain against `matches`.
    fn any_suffix(host: &str, mut matches: impl FnMut(&str) -> bool) -> bool {
        let mut candidate = host;
        loop {
            if matches(candidate) {
                return true;
            }
            let Some((_, rest)) = candidate.split_once('.') else {
                return false;
            };
            candidate = rest;
        }
    }

    pub fn should_block_url(url: &str) -> bool {
        let Some(host) = Self::host_of(url) else {
            return false;
        };
        let extra = EXTRA_DOMAINS.read().ok();
        Self::any_suffix(&host, |c| {
            Self::BLOCKED_DOMAINS.binary_search(&c).is_ok()
                || extra.as_ref().is_some_and(|set| set.contains(c))
        })
    }

    fn is_tracker(url: &str) -> bool {
        Self::host_of(url).is_some_and(|host| {
            Self::any_suffix(&host, |c| Self::TRACKER_DOMAINS.contains(&c))
                || host.contains("analytics")
                || host.contains("track")
                || host.contains("pixel")
        })
    }

    /// Blocks ad/tracker subresources (scripts, iframes, XHR, images) at the network level.
    /// `with_navigation_handler` only sees top-level navigations, so it can't do this.
    /// `site_allowed` = the user turned blocking off for the page currently shown in this tab.
    #[cfg(windows)]
    pub fn install_network_filter(
        webview: &wry::WebView,
        enabled: std::rc::Rc<std::cell::Cell<bool>>,
        site_allowed: std::rc::Rc<std::cell::Cell<bool>>,
        blocked: std::rc::Rc<std::cell::Cell<BlockedCounts>>,
    ) -> windows::core::Result<()> {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        use webview2_com::{WebResourceRequestedEventHandler, take_pwstr};
        use windows::core::{Interface, PWSTR, w};
        use wry::WebViewExtWindows;

        let core = webview.webview();
        let env = webview.environment();
        unsafe {
            // Newer runtimes need explicit source kinds to also see iframe/worker requests.
            match core.cast::<ICoreWebView2_22>() {
                Ok(wv22) => wv22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    w!("*"),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                )?,
                Err(_) => core.AddWebResourceRequestedFilter(
                    w!("*"),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                )?,
            }
            let mut token = Default::default();
            core.add_WebResourceRequested(
                &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    if !enabled.get() || site_allowed.get() {
                        return Ok(());
                    }
                    let mut uri = PWSTR::null();
                    args.Request()?.Uri(&mut uri)?;
                    let uri = take_pwstr(uri);
                    if Self::should_block_url(&uri) {
                        let resp =
                            env.CreateWebResourceResponse(None, 403, w!("Blocked"), w!(""))?;
                        args.SetResponse(&resp)?;
                        let mut ctx = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                        let _ = args.ResourceContext(&mut ctx);
                        let mut counts = blocked.get();
                        if Self::is_tracker(&uri) {
                            counts.trackers += 1;
                        } else if ctx == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT {
                            counts.scripts += 1;
                        } else {
                            counts.ads += 1;
                        }
                        blocked.set(counts);
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }
        Ok(())
    }

    /// Extracts blockable domains from an AdBlock Plus list (`||domain^` rules without options),
    /// a hosts file (`0.0.0.0 domain`) or a plain one-domain-per-line list.
    pub fn parse_filter_list(text: &str) -> impl Iterator<Item = String> + '_ {
        text.lines().filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with(['!', '#', '[', '@']) {
                return None;
            }
            let domain = if let Some(rest) = line.strip_prefix("||") {
                rest.strip_suffix('^')?
            } else if let Some((ip, d)) = line.split_once(char::is_whitespace) {
                if !matches!(ip, "0.0.0.0" | "127.0.0.1") {
                    return None;
                }
                d.split('#').next()?.trim()
            } else {
                line
            };
            let domain = domain.to_ascii_lowercase();
            let valid = domain.len() <= 253
                && domain.contains('.')
                && !domain.starts_with(['.', '-'])
                && domain
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                && !matches!(domain.as_str(), "localhost" | "local" | "0.0.0.0");
            valid.then_some(domain)
        })
    }

    fn lists_cache_path() -> std::path::PathBuf {
        crate::storage::config_dir().join("adblock_domains.txt")
    }

    /// Loads the last downloaded filter lists (call off the UI thread).
    pub fn load_cached_lists() {
        if let Ok(text) = std::fs::read_to_string(Self::lists_cache_path()) {
            let set: HashSet<String> = text.lines().map(str::to_string).collect();
            if let Ok(mut slot) = EXTRA_DOMAINS.write() {
                *slot = set;
            }
        }
    }

    pub fn clear_lists() {
        if let Ok(mut slot) = EXTRA_DOMAINS.write() {
            slot.clear();
        }
    }

    /// Downloads the filter lists, caches them on disk and activates them. Returns the rule count.
    pub fn update_lists() -> Result<usize, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .build()
            .into();
        let mut set = HashSet::new();
        let mut errors = Vec::new();
        for url in FILTER_LISTS {
            match agent
                .get(*url)
                .header("User-Agent", &format!("Viima/{}", crate::updater::CURRENT_VERSION))
                .call()
                .and_then(|mut r| r.body_mut().with_config().limit(20 * 1024 * 1024).read_to_string())
            {
                Ok(text) => set.extend(Self::parse_filter_list(&text)),
                Err(e) => errors.push(format!("{url}: {e}")),
            }
        }
        if set.is_empty() {
            return Err(errors.join("; "));
        }
        let mut lines: Vec<&str> = set.iter().map(String::as_str).collect();
        lines.sort_unstable();
        let _ = std::fs::write(Self::lists_cache_path(), lines.join("\n"));
        let count = set.len();
        if let Ok(mut slot) = EXTRA_DOMAINS.write() {
            *slot = set;
        }
        Ok(count)
    }

    /// Hides common cookie-consent dialogs (OneTrust, Cookiebot, Didomi, Sourcepoint, ...).
    /// Hiding without answering = no consent given, which is the privacy-preserving default.
    pub fn cookie_banner_script() -> &'static str {
        r#"
        (function() {
            if (window.__viimaCookieHide) return;
            const SEL = '#onetrust-banner-sdk,#onetrust-consent-sdk,#CybotCookiebotDialog,#CybotCookiebotDialogBodyUnderlay,.fc-consent-root,#usercentrics-root,#usercentrics-cmp-ui,.qc-cmp2-container,#qc-cmp2-container,#didomi-host,[id^="sp_message_container"],[id^="sp_message_iframe"],#cmpbox,#cmpbox2,.cc-window.cc-banner,#cookie-law-info-bar,#cookiescript_injected,#truste-consent-track,.truste_box_overlay,.osano-cm-window,.cky-consent-container,#moove_gdpr_cookie_info_bar,.cmplz-cookiebanner,#BorlabsCookieBox,#sd-cmp,.almacmp-modal,#almacmp-modal-layer1,#almacmp-modal-layer2';
            let style = null;
            window.__viimaCookieHide = function(on) {
                if (!on) { if (style) style.remove(); style = null; return; }
                if (style) return;
                style = document.createElement('style');
                style.textContent = SEL + '{display:none!important}';
                const add = () => (document.head || document.documentElement).appendChild(style);
                if (document.documentElement) add(); else document.addEventListener('DOMContentLoaded', add, { once: true });
                // Consent walls lock scrolling; unlock only when one of them is on the page.
                window.addEventListener('load', () => setTimeout(() => {
                    if (!style || !document.querySelector(SEL)) return;
                    for (const el of [document.documentElement, document.body]) {
                        if (el && getComputedStyle(el).overflow === 'hidden') el.style.setProperty('overflow', 'auto', 'important');
                    }
                }, 800), { once: true });
            };
        })();
        "#
    }

    pub fn get_injection_script() -> &'static str {
        r#"
        (function() {
            if (window.__viimaAdblockSetEnabled) return;
            let observer = null;
            let originalAdsbygooglePush = null;
            let hadOwnAdsbygooglePush = false;
            let createdAdsbygoogle = false;
            window.__viimaAdblockSetEnabled = function(enabled) {
                if (!enabled) {
                    if (observer) observer.disconnect();
                    observer = null;
                    const style = document.getElementById('__viima_adblock_css');
                    if (style) style.remove();
                    if (createdAdsbygoogle) {
                        delete window.adsbygoogle;
                    } else if (window.adsbygoogle) {
                        if (hadOwnAdsbygooglePush) {
                            window.adsbygoogle.push = originalAdsbygooglePush;
                        } else {
                            delete window.adsbygoogle.push;
                        }
                    }
                    originalAdsbygooglePush = null;
                    hadOwnAdsbygooglePush = false;
                    createdAdsbygoogle = false;
                    window.__viima_adblock_injected = false;
                    return;
                }
                if (window.__viima_adblock_injected) return;
                window.__viima_adblock_injected = true;

                // Safe mock for common ad APIs to prevent page crashes
                createdAdsbygoogle = !window.adsbygoogle;
                hadOwnAdsbygooglePush = Boolean(
                    window.adsbygoogle && Object.prototype.hasOwnProperty.call(window.adsbygoogle, 'push')
                );
                originalAdsbygooglePush = hadOwnAdsbygooglePush ? window.adsbygoogle.push : null;
                window.adsbygoogle = window.adsbygoogle || [];
                window.adsbygoogle.push = function() { return 0; };

                // Inject cosmetic filter CSS
                const style = document.createElement('style');
                style.id = '__viima_adblock_css';
                style.textContent = `
                ins.adsbygoogle,
                div[id*="google_ads"],
                div[id*="gpt-ad"],
                div[id*="ad-slot"],
                div[class*="ad-banner"],
                div[class*="advertisement"],
                div[class*="sponsored-post"],
                div[class*="ob-widget"],
                div[class*="trc_rbox_div"],
                .adsbox,
                .ad-zone,
                .ad-slot,
                .commercial-unit,
                .top-banner-ad,
                .sticky-bottom-ad,
                iframe[src*="adservice"],
                iframe[src*="doubleclick"],
                iframe[src*="criteo"],
                iframe[src*="adnxs"],
                iframe[src*="taboola"] {
                    display: none !important;
                    visibility: hidden !important;
                    height: 0 !important;
                    width: 0 !important;
                    opacity: 0 !important;
                    pointer-events: none !important;
                }
                `;

                function applyStyle() {
                    if (!window.__viima_adblock_injected) return;
                    const target = document.head || document.documentElement;
                    if (target && !document.getElementById('__viima_adblock_css')) {
                        target.appendChild(style);
                    }
                }

                if (document.head || document.documentElement) applyStyle();
                else document.addEventListener('DOMContentLoaded', applyStyle, { once: true });

                // Clean dynamically added ad iframes & nodes
                const adIframeRe = /doubleclick|googleadservices|adservice|criteo|adnxs|taboola/i;
                observer = new MutationObserver(function(mutations) {
                    for (const mutation of mutations) {
                        for (const node of mutation.addedNodes) {
                            if (node.nodeType === 1) {
                                const tag = node.tagName.toLowerCase();
                                if (tag === 'ins' && node.classList.contains('adsbygoogle')) {
                                    node.remove();
                                } else if (tag === 'iframe') {
                                    const src = node.getAttribute('src') || '';
                                    if (adIframeRe.test(src)) node.remove();
                                }
                            }
                        }
                    }
                });

                const observe = () => {
                    if (window.__viima_adblock_injected && document.body && observer) {
                        observer.observe(document.body, { childList: true, subtree: true });
                    }
                };
                if (document.body) observe();
                else document.addEventListener('DOMContentLoaded', observe, { once: true });
            };
        })();
        "#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_domains_sorted() {
        assert!(AdBlocker::BLOCKED_DOMAINS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn test_parse_filter_list() {
        let list = "! comment\n[Adblock Plus 2.0]\n||ads.example.com^\n||track.example.org^$third-party\n\
                    ||example.net/path^\n@@||good.com^\n0.0.0.0 tracker.example.io # c\n127.0.0.1 localhost\n\
                    plain-ads.example.fi\n##.ad-banner\n||BAD_chars!.com^";
        let got: Vec<String> = AdBlocker::parse_filter_list(list).collect();
        assert_eq!(
            got,
            vec!["ads.example.com", "tracker.example.io", "plain-ads.example.fi"]
        );
        assert!(AdBlocker::is_tracker("https://www.google-analytics.com/a.js"));
        assert!(!AdBlocker::is_tracker("https://doubleclick.net/ad"));
    }

    #[test]
    fn test_adblock_domain_matching() {
        assert!(AdBlocker::should_block_url(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js"
        ));
        assert!(AdBlocker::should_block_url(
            "https://securepubads.g.doubleclick.net/gampad/ads"
        ));
        assert!(AdBlocker::should_block_url(
            "HTTPS://SECUREPUBADS.G.DOUBLECLICK.NET./gampad/ads"
        ));
        assert!(AdBlocker::should_block_url(
            "https://connect.facebook.net/en_US/fbevents.js"
        ));
        assert!(AdBlocker::should_block_url(
            "https://criteo.com/delivery/ajs.php"
        ));
        assert!(AdBlocker::should_block_url(
            "https://c.amazon-adsystem.com/aax2/apstag.js"
        ));
        assert!(AdBlocker::should_block_url(
            "https://www.google-analytics.com/analytics.js"
        ));
        assert!(!AdBlocker::should_block_url(
            "https://notdoubleclick.net/test"
        ));
        assert!(!AdBlocker::should_block_url("https://amazon.com/"));
        assert!(!AdBlocker::should_block_url(
            "https://www.google.com/search?q=rust"
        ));
        assert!(!AdBlocker::should_block_url("https://yle.fi/uutiset"));
        assert!(!AdBlocker::should_block_url(
            "https://github.com/rust-lang/rust"
        ));
        assert!(!AdBlocker::should_block_url("not a url"));
    }
}
