pub struct AdBlocker;

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

    /// Blocks ad/tracker subresources (scripts, iframes, XHR, images) at the network level.
    /// `with_navigation_handler` only sees top-level navigations, so it can't do this.
    #[cfg(windows)]
    pub fn install_network_filter(
        webview: &wry::WebView,
        enabled: std::rc::Rc<std::cell::Cell<bool>>,
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
                    if !enabled.get() {
                        return Ok(());
                    }
                    let mut uri = PWSTR::null();
                    args.Request()?.Uri(&mut uri)?;
                    if Self::should_block_url(&take_pwstr(uri)) {
                        let resp =
                            env.CreateWebResourceResponse(None, 403, w!("Blocked"), w!(""))?;
                        args.SetResponse(&resp)?;
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }
        Ok(())
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
