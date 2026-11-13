// Find in page (Ctrl+F): floating bar in a closed shadow root, matches painted with the CSS
// Custom Highlight API so the page DOM is never modified.
(function () {
    if (window.top !== window || window.__viimaFind) return;
    const EN = window.__viimaLang === 'en'; // browser UI language, set by Viima before this script
    const MAX = 2000;
    let host = null, input = null, countEl = null, ranges = [], index = -1, timer = 0, styleEl = null;

    function ensureHighlightStyle() {
        if (styleEl && styleEl.isConnected) return;
        styleEl = document.createElement('style');
        styleEl.textContent = '::highlight(viima-find){background:#facc15;color:#111}' +
            '::highlight(viima-find-current){background:#f97316;color:#111}';
        (document.head || document.documentElement).appendChild(styleEl);
    }

    function build() {
        host = document.createElement('viima-find');
        host.style.cssText = 'all:initial;position:fixed;top:10px;right:16px;z-index:2147483647;';
        const root = host.attachShadow({ mode: 'closed' });
        root.innerHTML = `
            <style>
                .bar{display:flex;align-items:center;gap:6px;padding:6px 8px;border-radius:10px;
                    background:#1c1f26;color:#f1f5f9;border:1px solid rgba(255,255,255,.12);
                    box-shadow:0 8px 24px rgba(0,0,0,.35);font:13px "Segoe UI",system-ui,sans-serif}
                input{width:220px;background:#11141a;color:inherit;border:1px solid rgba(255,255,255,.14);
                    border-radius:6px;padding:5px 8px;font:inherit;outline:none}
                input:focus{border-color:#38bdf8}
                .count{min-width:56px;text-align:center;color:#94a3b8;font-variant-numeric:tabular-nums}
                button{all:unset;cursor:pointer;width:24px;height:24px;display:grid;place-items:center;
                    border-radius:5px;color:#cbd5e1}
                button:hover{background:rgba(255,255,255,.1)}
            </style>
            <div class="bar">
                <input type="text" spellcheck="false" placeholder="${EN ? 'Find on page' : 'Etsi sivulta'}">
                <span class="count"></span>
                <button data-a="prev" title="${EN ? 'Previous (Shift+Enter)' : 'Edellinen (Shift+Enter)'}">&#8593;</button>
                <button data-a="next" title="${EN ? 'Next (Enter)' : 'Seuraava (Enter)'}">&#8595;</button>
                <button data-a="close" title="${EN ? 'Close (Esc)' : 'Sulje (Esc)'}">&#10005;</button>
            </div>`;
        input = root.querySelector('input');
        countEl = root.querySelector('.count');
        input.addEventListener('input', () => {
            clearTimeout(timer);
            timer = setTimeout(() => search(input.value), 120);
        });
        input.addEventListener('keydown', (e) => {
            e.stopPropagation();
            if (e.key === 'Escape') close();
            else if (e.key === 'Enter') step(e.shiftKey ? -1 : 1);
        });
        root.addEventListener('click', (e) => {
            const a = e.target.closest('button') && e.target.closest('button').dataset.a;
            if (a === 'close') close();
            else if (a) step(a === 'prev' ? -1 : 1);
        });
    }

    function clear() {
        ranges = [];
        index = -1;
        if (window.CSS && CSS.highlights) {
            CSS.highlights.delete('viima-find');
            CSS.highlights.delete('viima-find-current');
        }
    }

    function search(query) {
        clear();
        const q = (query || '').toLocaleLowerCase();
        if (q && document.body) {
            const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
                acceptNode(node) {
                    const p = node.parentElement;
                    if (!p || p.closest('script,style,noscript,template,viima-find')) return NodeFilter.FILTER_REJECT;
                    return NodeFilter.FILTER_ACCEPT;
                }
            });
            let node;
            outer: while ((node = walker.nextNode())) {
                const text = node.data.toLocaleLowerCase();
                // toLocaleLowerCase can change length for a few characters; skip those nodes.
                if (text.length !== node.data.length) continue;
                let from = 0, at;
                while ((at = text.indexOf(q, from)) !== -1) {
                    const r = document.createRange();
                    r.setStart(node, at);
                    r.setEnd(node, at + q.length);
                    // Skip invisible text (display:none, collapsed sections).
                    if (r.getClientRects().length) ranges.push(r);
                    if (ranges.length >= MAX) break outer;
                    from = at + q.length;
                }
            }
        }
        if (ranges.length) {
            ensureHighlightStyle();
            if (window.CSS && CSS.highlights) CSS.highlights.set('viima-find', new Highlight(...ranges));
            index = 0;
            show();
        } else {
            countEl.textContent = q ? (EN ? 'No results' : 'Ei osumia') : '';
        }
    }

    function show() {
        countEl.textContent = `${index + 1} / ${ranges.length}${ranges.length >= MAX ? '+' : ''}`;
        const r = ranges[index];
        if (window.CSS && CSS.highlights) CSS.highlights.set('viima-find-current', new Highlight(r));
        const el = r.startContainer.parentElement;
        if (el) el.scrollIntoView({ block: 'center', inline: 'nearest' });
    }

    function step(dir) {
        if (!ranges.length) {
            search(input.value);
            return;
        }
        index = (index + dir + ranges.length) % ranges.length;
        show();
    }

    function open() {
        if (!host) build();
        if (!host.isConnected) document.documentElement.appendChild(host);
        const sel = String(window.getSelection() || '').trim();
        if (sel && sel.length < 200 && !sel.includes('\n')) input.value = sel;
        input.focus();
        input.select();
        if (input.value) search(input.value);
    }

    function close() {
        clear();
        if (host) host.remove();
    }

    window.__viimaFind = { open, close };
    window.addEventListener('keydown', (e) => {
        const k = (e.key || '').toLowerCase();
        if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && k === 'f') {
            e.preventDefault();
            e.stopPropagation();
            open();
        } else if (e.key === 'F3' && host && host.isConnected) {
            e.preventDefault();
            step(e.shiftKey ? -1 : 1);
        } else if (e.key === 'Escape' && host && host.isConnected) {
            close();
        }
    }, true);
})();
