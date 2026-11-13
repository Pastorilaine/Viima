// Keyboard shortcuts for focused web pages (the chrome webview never sees those key events).
// Reload, history navigation, zoom (Ctrl +/-/0), print (Ctrl+P) and DevTools (F12) are handled
// natively by WebView2; Ctrl+F is handled by the find bar script.
window.addEventListener('keydown', function (e) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || !window.ipc) return;
    const k = (e.key || '').toLowerCase();
    let key = null;
    if (k === 'tab') key = e.shiftKey ? 'prev_tab' : 'next_tab';
    else if (e.shiftKey) key = { t: 'reopen_tab', n: 'private_window', b: 'bookmarks_bar' }[k] || null;
    else if (/^[1-9]$/.test(k)) key = 'tab_' + k;
    else key = { t: 'new_tab', w: 'close_tab', l: 'focus_omnibox', h: 'history', j: 'downloads', d: 'bookmark' }[k] || null;
    if (!key) return;
    e.preventDefault();
    e.stopPropagation();
    window.ipc.postMessage(JSON.stringify({ action: 'shortcut', key }));
}, true);
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
