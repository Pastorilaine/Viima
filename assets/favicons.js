// Site icons for the chrome and internal pages. Rust fetches and caches them and hands them
// over as data: URLs (see src/favicon.rs); pages only ask for the hosts they show.
(function () {
    const icons = {};
    const asked = new Set();
    const listeners = [];

    function host(url) {
        try {
            const u = new URL(url);
            return /^https?:$/.test(u.protocol) && !u.hostname.startsWith('browser.') ? u.hostname.toLowerCase() : null;
        } catch {
            return null;
        }
    }

    window.viimaFavicons = {
        // Icon (data: URL) for a page address, or null while unknown.
        get(url) {
            const h = host(url);
            return h && icons[h] ? icons[h] : null;
        },
        // Ask for the icons of these addresses; onUpdate callbacks run when they arrive.
        want(urls) {
            const hosts = [...new Set(urls.map(host).filter(h => h && !icons[h] && !asked.has(h)))].slice(0, 48);
            if (!hosts.length || !window.ipc) return;
            hosts.forEach(h => asked.add(h));
            window.ipc.postMessage(JSON.stringify({ action: 'request_favicons', hosts }));
        },
        onUpdate(cb) {
            listeners.push(cb);
        },
    };

    window.onFavicons = function (map) {
        Object.assign(icons, map);
        listeners.forEach(cb => cb());
    };
})();
