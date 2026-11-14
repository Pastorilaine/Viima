// Reports the page's favicon to the tab strip. Browser keyboard shortcuts are handled
// natively (WebView2 accelerator keys), so pages can't fake them.
(function () {
    if (window.top !== window || !/^https?:$/.test(location.protocol)) return;
    function sendIcon() {
        if (!window.ipc) return;
        const l = document.querySelector('link[rel~="icon"]');
        const url = l && l.href ? l.href : location.origin + '/favicon.ico';
        window.ipc.postMessage(JSON.stringify({ action: 'favicon', url }));
    }
    if (document.readyState === 'complete') sendIcon();
    else window.addEventListener('load', sendIcon, { once: true });
})();
