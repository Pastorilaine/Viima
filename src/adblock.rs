pub struct AdBlocker;

impl AdBlocker {
    // Sorted alphabetically for zero-allocation binary search in `should_block_url`.
    pub const BLOCKED_DOMAINS: &'static [&'static str] = &[
        "adform.net",
        "adnxs.com",
        "adroll.com",
        "ads-twitter.com",
        "adservice.google.com",
        "adsystem.com",
        "adtech.de",
        "advertising.amazon.com",
        "analytics.google.com",
        "chartbeat.net",
        "clarity.ms",
        "connect.facebook.net",
        "criteo.com",
        "criteo.net",
        "doubleclick.net",
        "googleadservices.com",
        "googlesyndication.com",
        "googletagmanager.com",
        "hotjar.com",
        "moatads.com",
        "openx.net",
        "outbrain.com",
        "pagead2.googlesyndication.com",
        "pixel.facebook.com",
        "pubmatic.com",
        "quantserve.com",
        "rubiconproject.com",
        "scorecardresearch.com",
        "smartadserver.com",
        "taboola.com",
    ];

    pub fn should_block_url(url: &str) -> bool {
        let Ok(parsed) = url::Url::parse(url) else {
            return false;
        };
        let Some(host) = parsed.host_str() else {
            return false;
        };
        // `url::Url` normalizes ASCII domains to lowercase, so we only need to trim trailing dots.
        let mut candidate = host.trim_end_matches('.');
        loop {
            if Self::BLOCKED_DOMAINS.binary_search(&candidate).is_ok() {
                return true;
            }
            let Some((_, rest)) = candidate.split_once('.') else {
                return false;
            };
            candidate = rest;
        }
    }

    pub fn get_injection_script() -> &'static str {
        r#"
        (function() {
            if (window.__omaSelainAdblockSetEnabled) return;
            let observer = null;
            let originalAdsbygooglePush = null;
            let hadOwnAdsbygooglePush = false;
            let createdAdsbygoogle = false;
            window.__omaSelainAdblockSetEnabled = function(enabled) {
                if (!enabled) {
                    if (observer) observer.disconnect();
                    observer = null;
                    const style = document.getElementById('__oma_selain_adblock_css');
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
                    window.__oma_selain_adblock_injected = false;
                    return;
                }
                if (window.__oma_selain_adblock_injected) return;
                window.__oma_selain_adblock_injected = true;

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
                style.id = '__oma_selain_adblock_css';
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
                    if (!window.__oma_selain_adblock_injected) return;
                    const target = document.head || document.documentElement;
                    if (target && !document.getElementById('__oma_selain_adblock_css')) {
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
                    if (window.__oma_selain_adblock_injected && document.body && observer) {
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
        assert!(!AdBlocker::should_block_url(
            "https://notdoubleclick.net/test"
        ));
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
