// Floating video player, page side. Run by the browser in the tab whose webview was moved into
// the small player window. Shows only the video (everything else on the page is hidden with CSS,
// the video element itself never moves, so playback and the site's player keep working) and
// draws the controls on top. Commands that need the window (move, resize, back, close) go to the
// browser through ipc; the rest act on the video directly.
(function () {
    if (window.__viimaPip && window.__viimaPip.stop) window.__viimaPip.stop();
    var S = { v: null, marked: [], timers: [], undo: [] };

    function find() {
        var best = null, area = 0;
        document.querySelectorAll('video').forEach(function (v) {
            if (v.paused || v.ended || v.readyState < 2 || !v.videoWidth) return;
            var a = v.videoWidth * v.videoHeight;
            if (a > area) { area = a; best = v; }
        });
        S.v = best;
        return best ? best.videoWidth + 'x' + best.videoHeight : '';
    }

    function send(cmd, dir) {
        try { window.ipc.postMessage(JSON.stringify({ action: 'pip_ui', cmd: cmd, dir: dir || '' })); } catch (e) {}
    }

    function fmt(s) {
        s = Math.max(0, Math.floor(s || 0));
        var m = Math.floor(s / 60), h = Math.floor(m / 60), ss = ('0' + (s % 60)).slice(-2);
        return h ? h + ':' + ('0' + (m % 60)).slice(-2) + ':' + ss : m + ':' + ss;
    }

    var ICON = {
        play: 'M8 5v14l11-7z',
        pause: 'M6 19h4V5H6v14zm8-14v14h4V5h-4z',
        back10: 'M12 5V1L7 6l5 5V7c3.3 0 6 2.7 6 6s-2.7 6-6 6-6-2.7-6-6H4c0 4.4 3.6 8 8 8s8-3.6 8-8-3.6-8-8-8z',
        fwd10: 'M12 5V1l5 5-5 5V7c-3.3 0-6 2.7-6 6s2.7 6 6 6 6-2.7 6-6h2c0 4.4-3.6 8-8 8s-8-3.6-8-8 3.6-8 8-8z',
        vol: 'M3 9v6h4l5 5V4L7 9H3zm13.5 3A4.5 4.5 0 0 0 14 8v8a4.5 4.5 0 0 0 2.5-4z',
        mute: 'M16.5 12A4.5 4.5 0 0 0 14 8v2.18l2.45 2.45c.03-.2.05-.41.05-.63zM4.27 3 3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06a8.99 8.99 0 0 0 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4 9.91 6.09 12 8.18V4z',
        back: 'M19 19H5V5h7V3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7h-2v7zM14 3v2h3.59l-9.83 9.83 1.41 1.41L19 6.41V10h2V3h-7z',
        close: 'M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z'
    };
    var NS = 'http://www.w3.org/2000/svg';
    function el(tag, cls, kids) {
        var e = document.createElement(tag);
        if (cls) e.className = cls;
        (kids || []).forEach(function (k) { e.appendChild(k); });
        return e;
    }
    function btn(cls, d) {
        var b = el('button', cls), s = document.createElementNS(NS, 'svg'), p = document.createElementNS(NS, 'path');
        s.setAttribute('viewBox', '0 0 24 24'); p.setAttribute('d', d); s.appendChild(p); b.appendChild(s);
        return b;
    }
    function range(cls, max, val) {
        var i = el('input', cls); i.type = 'range'; i.min = 0; i.max = max; i.value = val;
        return i;
    }

    var CSS =
        ':host{all:initial;--z:1}*{box-sizing:border-box;margin:0;padding:0}' +
        // The page is zoomed out by --z so the site keeps its desktop layout; the controls are laid out
        // at window size and scaled back up, so they come out at normal size on screen.
        '.o{position:fixed;left:0;top:0;width:calc(100vw * var(--z));height:calc(100vh * var(--z));transform:scale(calc(1 / var(--z)));transform-origin:0 0;font:12px Bahnschrift,"Segoe UI",sans-serif;color:#fff;user-select:none;cursor:none}' +
        '.o.on{cursor:default}' +
        '.ui{position:absolute;inset:0;display:flex;flex-direction:column;justify-content:space-between;opacity:0;transition:opacity .18s}' +
        '.on .ui{opacity:1}' +
        '.top,.bot{display:flex;align-items:center;gap:2px;padding:8px 10px}' +
        '.top{background:linear-gradient(rgba(0,0,0,.6),transparent)}' +
        '.bot{background:linear-gradient(transparent,rgba(0,0,0,.65));padding-top:22px}' +
        '.mid{display:flex;justify-content:center;align-items:center;gap:22px}' +
        '.site{flex:1;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;opacity:.85;pointer-events:none}' +
        'button{flex:none;width:32px;height:32px;border:0;border-radius:6px;background:none;color:#fff;display:grid;place-items:center;cursor:pointer;transition:background .12s,transform .08s}' +
        'button:hover{background:rgba(255,255,255,.18)}button:active{transform:scale(.9)}' +
        'button:focus-visible{outline:2px solid #38bdf8}' +
        '.big{width:46px;height:46px;border-radius:50%;background:rgba(0,0,0,.45)}.big svg{width:26px;height:26px}' +
        'svg{width:19px;height:19px;fill:#fff}' +
        '.t{flex:none;min-width:34px;text-align:center;opacity:.85;font-variant-numeric:tabular-nums}' +
        'input{-webkit-appearance:none;appearance:none;background:none;height:18px;cursor:pointer;min-width:0;--p:0%}' +
        'input::-webkit-slider-runnable-track{height:3px;border-radius:2px;background:linear-gradient(to right,#38bdf8 var(--p),rgba(255,255,255,.3) var(--p))}' +
        'input::-webkit-slider-thumb{-webkit-appearance:none;width:11px;height:11px;border-radius:50%;background:#fff;margin-top:-4px}' +
        '.seek{flex:1}.vol{width:56px;flex:none}' +
        '@media (max-width:380px){.vol,.mute{display:none}}' +
        '.e{position:absolute;z-index:2}' +
        '.n{top:0;left:8px;right:8px;height:5px;cursor:ns-resize}.s{bottom:0;left:8px;right:8px;height:5px;cursor:ns-resize}' +
        '.w{left:0;top:8px;bottom:8px;width:5px;cursor:ew-resize}.ea{right:0;top:8px;bottom:8px;width:5px;cursor:ew-resize}' +
        '.nw{top:0;left:0;width:9px;height:9px;cursor:nwse-resize}.se{bottom:0;right:0;width:9px;height:9px;cursor:nwse-resize}' +
        '.ne{top:0;right:0;width:9px;height:9px;cursor:nesw-resize}.sw{bottom:0;left:0;width:9px;height:9px;cursor:nesw-resize}' +
        '@media (prefers-reduced-motion:reduce){*{transition:none!important}}';

    function start(L, z) {
        var v = S.v;
        if (!v || !v.isConnected) return false;
        L = L || {};

        // Video only: hide everything that is not on the way from <html> down to the video.
        for (var n = v; n && n !== document.documentElement; n = n.parentElement) {
            n.setAttribute('data-viima-pip-a', '');
            S.marked.push(n);
            Array.prototype.forEach.call(n.parentElement.children, function (sib) {
                if (sib !== n) { sib.setAttribute('data-viima-pip-h', ''); S.marked.push(sib); }
            });
        }
        v.setAttribute('data-viima-pip-v', '');
        S.undo.push(function () { v.removeAttribute('data-viima-pip-v'); });
        S.marked.forEach(function (el) { S.undo.push(function () { el.removeAttribute('data-viima-pip-a'); el.removeAttribute('data-viima-pip-h'); }); });
        var oldControls = v.controls;
        v.controls = false;
        S.undo.push(function () { v.controls = oldControls; });

        var st = document.createElement('style');
        st.textContent =
            '[data-viima-pip-h]{display:none!important}' +
            '[data-viima-pip-a]{position:static!important;transform:none!important;filter:none!important;contain:none!important;perspective:none!important;clip-path:none!important;overflow:visible!important;margin:0!important;padding:0!important;border:0!important;max-width:none!important;max-height:none!important;opacity:1!important}' +
            'video[data-viima-pip-v]{position:fixed!important;inset:0!important;width:100vw!important;height:100vh!important;max-width:none!important;max-height:none!important;object-fit:contain!important;background:#000!important;z-index:2147483646!important;transform:none!important;margin:0!important;visibility:visible!important}' +
            'html,body{background:#000!important;overflow:hidden!important}';
        document.documentElement.appendChild(st);
        S.undo.push(function () { st.remove(); });

        var host = document.createElement('div');
        host.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483647';
        var root = host.attachShadow({ mode: 'closed' });
        // Built with DOM calls, not innerHTML: sites like YouTube enforce Trusted Types.
        var style = el('style');
        style.textContent = CSS;
        var site = el('span', 'site');
        var cur = el('span', 't cur'); cur.textContent = '0:00';
        var dur = el('span', 't dur'); dur.textContent = '0:00';
        var edges = ['n', 's', 'w', 'ea', 'nw', 'ne', 'sw', 'se'].map(function (d) {
            var e = el('div', 'e ' + d); e.dataset.d = d === 'ea' ? 'e' : d; return e;
        });
        var ui = el('div', 'ui', [
            el('div', 'top', [site, btn('back', ICON.back), btn('close', ICON.close)]),
            el('div', 'mid', [btn('big b10', ICON.back10), btn('big play', ICON.play), btn('big f10', ICON.fwd10)]),
            el('div', 'bot', [cur, range('seek', 1000, 0), dur, btn('mute', ICON.vol), range('vol', 100, 100)])
        ]);
        root.appendChild(style);
        root.appendChild(el('div', 'o', [ui].concat(edges)));
        host.style.setProperty('--z', z || 1);
        S.host = host;
        document.documentElement.appendChild(host);
        S.undo.push(function () { host.remove(); S.host = null; });

        var q = function (s) { return root.querySelector(s); };
        var o = q('.o'), seek = q('.seek'), vol = q('.vol'), dragging = false;
        q('.site').textContent = location.hostname.replace(/^www\./, '');
        q('.back').title = L.back || ''; q('.close').title = L.close || '';
        q('.play').title = L.play || ''; q('.mute').title = L.mute || '';
        var fill = function (el, f) { el.style.setProperty('--p', (f * 100).toFixed(1) + '%'); };
        var path = function (sel, d) { q(sel + ' path').setAttribute('d', d); };

        function render() {
            var d = isFinite(v.duration) ? v.duration : 0;
            path('.play', v.paused ? ICON.play : ICON.pause);
            path('.mute', v.muted || v.volume === 0 ? ICON.mute : ICON.vol);
            q('.dur').textContent = d ? fmt(d) : '';
            if (!dragging) {
                var f = d ? Math.min(1, v.currentTime / d) : 0;
                seek.value = f * 1000; fill(seek, f); q('.cur').textContent = fmt(v.currentTime);
            }
            var vv = v.muted ? 0 : v.volume;
            vol.value = vv * 100; fill(vol, vv);
        }
        var toggle = function () { v.paused ? v.play() : v.pause(); };
        var skip = function (s) { v.currentTime = Math.max(0, v.currentTime + s); };

        q('.back').onclick = function () { send('back'); };
        q('.close').onclick = function () { send('close'); };
        q('.play').onclick = toggle;
        q('.b10').onclick = function () { skip(-10); };
        q('.f10').onclick = function () { skip(10); };
        q('.mute').onclick = function () { v.muted = !v.muted; };
        seek.oninput = function () {
            dragging = true;
            var d = isFinite(v.duration) ? v.duration : 0;
            fill(seek, seek.value / 1000); q('.cur').textContent = fmt(seek.value / 1000 * d);
        };
        seek.onchange = function () {
            var d = isFinite(v.duration) ? v.duration : 0;
            if (d) v.currentTime = seek.value / 1000 * d;
            dragging = false;
        };
        vol.oninput = function () { v.volume = vol.value / 100; v.muted = false; };

        // Controls show while the mouse moves, fade after a moment.
        var idle;
        function wake() {
            o.classList.add('on'); clearTimeout(idle);
            idle = setTimeout(function () { o.classList.remove('on'); }, 2200);
        }
        o.addEventListener('mousemove', wake);
        o.addEventListener('mouseleave', function () { clearTimeout(idle); o.classList.remove('on'); });
        S.undo.push(function () { clearTimeout(idle); });

        // Drag the window by the picture; a plain click plays/pauses; edges resize.
        var down = null;
        o.addEventListener('mousedown', function (e) {
            var edge = e.target.dataset && e.target.dataset.d;
            if (edge) { send('resize', edge); return; }
            if (e.target === o || e.target.classList.contains('ui') || e.target.classList.contains('mid')) {
                down = { x: e.screenX, y: e.screenY };
            }
        });
        o.addEventListener('mousemove', function (e) {
            if (down && e.buttons === 1 && Math.abs(e.screenX - down.x) + Math.abs(e.screenY - down.y) > 4) {
                down = null; send('drag');
            }
        });
        o.addEventListener('mouseup', function () { if (down) { down = null; toggle(); } });

        function key(e) {
            if (e.key === ' ') { e.preventDefault(); e.stopPropagation(); toggle(); }
            else if (e.key === 'ArrowLeft') { e.stopPropagation(); skip(-5); }
            else if (e.key === 'ArrowRight') { e.stopPropagation(); skip(5); }
        }
        window.addEventListener('keydown', key, true);
        S.undo.push(function () { window.removeEventListener('keydown', key, true); });

        ['timeupdate', 'play', 'pause', 'volumechange', 'durationchange'].forEach(function (ev) {
            v.addEventListener(ev, render);
            S.undo.push(function () { v.removeEventListener(ev, render); });
        });
        // Next video or an ad with another shape: the window follows.
        var lastDims = v.videoWidth + 'x' + v.videoHeight;
        function dims() {
            var d = v.videoWidth + 'x' + v.videoHeight;
            if (v.videoWidth && d !== lastDims) { lastDims = d; send('ratio', d); }
        }
        v.addEventListener('resize', dims);
        S.undo.push(function () { v.removeEventListener('resize', dims); });
        // The page took its video away (navigated, removed it): nothing left to show.
        var watch = setInterval(function () { if (!v.isConnected) send('gone'); }, 700);
        S.undo.push(function () { clearInterval(watch); });
        render();
        return true;
    }

    function stop() {
        S.undo.reverse().forEach(function (f) { try { f(); } catch (e) {} });
        S.undo = []; S.marked = [];
    }

    function setZoom(z) { if (S.host) S.host.style.setProperty('--z', z); }

    window.__viimaPip = { find: find, start: start, stop: stop, setZoom: setZoom, video: function () { return S.v; } };
})();
