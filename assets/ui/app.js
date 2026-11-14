// State management
let currentState = {
    tabs: [],
    active_tab_id: 0,
    bookmarks: [],
    settings: {
        search_engine: 'google',
        adblock_enabled: true
    },
    downloads: { active: 0, percent: -1, total: 0 },
    private: false
};

// i18n.js defines window.t; keep working if it failed to load.
const tr = window.t || ((key, fi) => fi);

// SVG Icon Definitions (High-res, modern, geometric)
const ICONS = {
    globe: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>`,
    close: `<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>`,
    lock: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>`,
    bolt: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg>`,
    warning: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>`,
    search: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line></svg>`,
    starOutline: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`,
    starFilled: `<svg width="15" height="15" viewBox="0 0 24 24" fill="#f59e0b" stroke="#f59e0b" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`,
    speaker: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M15.5 8.5a5 5 0 0 1 0 7"></path><path d="M19 5a10 10 0 0 1 0 14"></path></svg>`,
    muted: `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><line x1="22" y1="9" x2="16" y2="15"></line><line x1="16" y1="9" x2="22" y2="15"></line></svg>`,
    folder: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path></svg>`
};

const INTERNAL_PAGES = 'newtab|history|bookmarks|settings|downloads|insecure';
const INTERNAL_RE = new RegExp(`^(?:browser:\\/\\/|http:\\/\\/browser\\.)(${INTERNAL_PAGES})(?:[\\/?#].*)?$`);

// Name of the internal page for browser://x or http://browser.x, else null.
function internalPage(url) {
    const m = INTERNAL_RE.exec(url);
    return m ? m[1] : null;
}

function normalizeUrl(url) {
    if (!url) return '';
    try {
        return new URL(url).href;
    } catch {
        return url;
    }
}

// Safe IPC sender
function sendIpc(action, payload = {}) {
    const message = JSON.stringify({ action, ...payload });
    if (window.ipc && window.ipc.postMessage) {
        window.ipc.postMessage(message);
    } else {
        console.log("IPC message:", message);
    }
}

// Elements
const tabsListEl = document.getElementById('tabsList');
const btnNewTabEl = document.getElementById('btnNewTab');
const btnBackEl = document.getElementById('btnBack');
const btnForwardEl = document.getElementById('btnForward');
const btnReloadEl = document.getElementById('btnReload');
const btnHomeEl = document.getElementById('btnHome');
const omniboxInputEl = document.getElementById('omniboxInput');
const omniboxIconEl = document.getElementById('omniboxIcon');
const omniboxWrapperEl = document.querySelector('.omnibox-wrapper');
const omniboxPrettyEl = document.getElementById('omniboxPretty');
const zoomBadgeEl = document.getElementById('zoomBadge');
const btnStarBookmarkEl = document.getElementById('btnStarBookmark');
const btnUpdateAppEl = document.getElementById('btnUpdateApp');
const updateAppTextEl = document.getElementById('updateAppText');
const btnDownloadsEl = document.getElementById('btnDownloads');
const downloadsTextEl = document.getElementById('downloadsText');
const downloadsProgressEl = document.getElementById('downloadsProgress');
const btnAdblockEl = document.getElementById('btnAdblock');
const adblockTextEl = document.getElementById('adblockText');
const btnBookmarksMenuEl = document.getElementById('btnBookmarksMenu');
const btnHistoryMenuEl = document.getElementById('btnHistoryMenu');
const btnSettingsMenuEl = document.getElementById('btnSettingsMenu');
const bookmarksBarEl = document.getElementById('bookmarksBar');
const storageErrorEl = document.getElementById('storageError');

// Window Controls
const btnWinMinimizeEl = document.getElementById('btnWinMinimize');
const btnWinMaximizeEl = document.getElementById('btnWinMaximize');
const btnWinCloseEl = document.getElementById('btnWinClose');

const tabElements = new Map();
let lastActiveTabId = 0;
let lastIconKind = '';
let lastBookmarked = null;
let lastDismissedError = '';
let lastBarKey = '';
let prettySource = null;

// Draw the URL with its real host emphasised (scheme/path dimmed, "https://" dropped), so
// tricks like https://bank.fi@evil.example or long subdomain chains stand out.
// Built with textContent only: the URL is page-controlled.
function renderPrettyUrl(displayUrl) {
    if (displayUrl !== prettySource) {
        prettySource = displayUrl;
        omniboxPrettyEl.replaceChildren();
        let u = null;
        try { u = new URL(displayUrl); } catch {}
        if (u && (u.protocol === 'https:' || u.protocol === 'http:') && u.host) {
            const part = (text, cls) => {
                if (!text) return;
                const span = document.createElement('span');
                if (cls) span.className = cls;
                span.textContent = text;
                omniboxPrettyEl.appendChild(span);
            };
            part(u.protocol === 'http:' ? 'http://' : '');
            part(u.username ? `${u.username}${u.password ? ':' + u.password : ''}@` : '');
            part(u.host, 'host');
            const rest = u.pathname + u.search + u.hash;
            part(rest === '/' ? '' : rest);
        } else {
            prettySource = null;
        }
    }
    syncPrettyUrl();
}

// Only overlay while the input still holds exactly the URL the overlay was built from.
function syncPrettyUrl() {
    omniboxWrapperEl.classList.toggle('pretty', prettySource !== null && omniboxInputEl.value === prettySource);
}

function newTabTitle() {
    return tr('ui.new_tab', 'Uusi välilehti');
}

// Keyed DOM reconciliation for Tabs (avoids wiping DOM & restarting spinners on every update)
function renderTabs() {
    const activeIds = new Set();

    currentState.tabs.forEach((tab, index) => {
        activeIds.add(tab.id);
        let entry = tabElements.get(tab.id);
        if (!entry) {
            const tabEl = document.createElement('div');
            tabEl.dataset.tabId = String(tab.id);
            tabEl.draggable = true;
            tabEl.innerHTML = `
                <span class="tab-icon">${ICONS.globe}</span>
                <span class="tab-spinner"></span>
                <span class="tab-title"></span>
                <span class="tab-audio"></span>
                <span class="tab-close">${ICONS.close}</span>
            `;
            tabEl.querySelector('.tab-close').title = tr('ui.close_tab', 'Sulje välilehti (Ctrl+W)');
            entry = {
                el: tabEl,
                titleEl: tabEl.querySelector('.tab-title'),
                iconEl: tabEl.querySelector('.tab-icon'),
                audioEl: tabEl.querySelector('.tab-audio'),
                favicon: null,
                audio: '',
            };
            tabElements.set(tab.id, entry);
        }

        // Site favicon, falling back to the globe if it fails to load.
        // The cached copy (from Rust) wins: it needs no network and survives restarts.
        const favicon = internalPage(tab.url || '') ? null : (viimaFavicons.get(tab.url) || tab.favicon || null);
        if (entry.favicon !== favicon) {
            entry.favicon = favicon;
            if (favicon) {
                const img = document.createElement('img');
                img.alt = '';
                img.referrerPolicy = 'no-referrer';
                img.addEventListener('error', () => { entry.iconEl.innerHTML = ICONS.globe; }, { once: true });
                img.src = favicon;
                entry.iconEl.replaceChildren(img);
            } else {
                entry.iconEl.innerHTML = ICONS.globe;
            }
        }

        // Audio indicator: click mutes / unmutes without switching tabs.
        const audio = tab.muted ? 'muted' : (tab.audible ? 'playing' : '');
        if (entry.audio !== audio) {
            entry.audio = audio;
            entry.audioEl.innerHTML = audio === 'muted' ? ICONS.muted : (audio ? ICONS.speaker : '');
            entry.audioEl.title = audio === 'muted'
                ? tr('ui.unmute', 'Poista mykistys')
                : tr('ui.mute', 'Mykistä välilehti');
        }

        const nextClass =
            'tab' +
            (tab.id === currentState.active_tab_id ? ' active' : '') +
            (tab.is_loading ? ' loading' : '') +
            (tab.pinned ? ' pinned' : '') +
            (tab.sleeping ? ' sleeping' : '') +
            (audio ? ' has-audio' : '');
        if (entry.el.className.replace(/ ?(drop-before|drop-after|dragging)/g, '') !== nextClass) {
            entry.el.className = nextClass;
        }

        let titleText = tab.title || '';
        if (!titleText || internalPage(titleText) === 'newtab') {
            titleText = newTabTitle();
        }
        const nextTooltip = (tab.title || tab.url || '') +
            (tab.sleeping ? ` — ${tr('ui.sleeping', 'lepotilassa')}` : '');
        if (entry.el.title !== nextTooltip) {
            entry.el.title = nextTooltip;
        }
        if (entry.titleEl.textContent !== titleText) {
            entry.titleEl.textContent = titleText;
        }

        const currentAtIndex = tabsListEl.children[index];
        if (currentAtIndex !== entry.el) {
            tabsListEl.insertBefore(entry.el, currentAtIndex || null);
        }
    });

    for (const [id, entry] of tabElements) {
        if (!activeIds.has(id)) {
            entry.el.remove();
            tabElements.delete(id);
        }
    }

    if (currentState.active_tab_id !== lastActiveTabId) {
        lastActiveTabId = currentState.active_tab_id;
        const activeEntry = tabElements.get(lastActiveTabId);
        if (activeEntry) {
            activeEntry.el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        }
    }
}

function tabIdFromEvent(e) {
    const tabEl = e.target.closest('.tab');
    return tabEl ? Number(tabEl.dataset.tabId) : 0;
}

function tabById(id) {
    return currentState.tabs.find(t => t.id === id);
}

// Delegated tab strip interactions
tabsListEl.addEventListener('click', (e) => {
    const id = tabIdFromEvent(e);
    if (!id) return;
    if (e.target.closest('.tab-close')) {
        sendIpc('close_tab', { id });
    } else if (e.target.closest('.tab-audio') && e.target.closest('.tab-audio').innerHTML) {
        sendIpc('toggle_mute', { id });
    } else {
        omniboxInputEl.blur();
        sendIpc('switch_tab', { id });
    }
});

// Right click: native tab menu (reload, duplicate, pin, mute, close others...).
tabsListEl.addEventListener('contextmenu', (e) => {
    const id = tabIdFromEvent(e);
    if (!id) return;
    e.preventDefault();
    sendIpc('tab_menu', { id });
});

// Middle click closes a tab (mousedown preventDefault stops the autoscroll cursor).
// Pinned tabs are protected from accidental closing.
tabsListEl.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();
});
tabsListEl.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    const id = tabIdFromEvent(e);
    const tab = tabById(id);
    if (tab && !tab.pinned) {
        e.preventDefault();
        sendIpc('close_tab', { id });
    }
});

tabsListEl.addEventListener('wheel', (e) => {
    if (e.deltaY !== 0) {
        tabsListEl.scrollLeft += e.deltaY;
        e.preventDefault();
    }
}, { passive: false });

// Drag & drop reordering
let dragId = 0;
function clearDropMarks() {
    tabsListEl.querySelectorAll('.drop-before, .drop-after').forEach(el => el.classList.remove('drop-before', 'drop-after'));
}
tabsListEl.addEventListener('dragstart', (e) => {
    dragId = tabIdFromEvent(e);
    if (!dragId) return;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', String(dragId));
    e.target.closest('.tab').classList.add('dragging');
});
tabsListEl.addEventListener('dragover', (e) => {
    if (!dragId) return;
    e.preventDefault();
    clearDropMarks();
    const target = e.target.closest('.tab');
    if (!target || Number(target.dataset.tabId) === dragId) return;
    const r = target.getBoundingClientRect();
    target.classList.add(e.clientX > r.left + r.width / 2 ? 'drop-after' : 'drop-before');
});
tabsListEl.addEventListener('drop', (e) => {
    if (!dragId) return;
    e.preventDefault();
    const target = e.target.closest('.tab');
    const ids = currentState.tabs.map(t => t.id);
    const from = ids.indexOf(dragId);
    let to = ids.length - 1;
    if (target) {
        const targetIndex = ids.indexOf(Number(target.dataset.tabId));
        const r = target.getBoundingClientRect();
        to = e.clientX > r.left + r.width / 2 ? targetIndex + 1 : targetIndex;
        if (from < to) to -= 1;
    }
    if (from !== -1 && to !== from) sendIpc('reorder_tab', { id: dragId, index: Math.max(0, to) });
    clearDropMarks();
});
tabsListEl.addEventListener('dragend', () => {
    dragId = 0;
    clearDropMarks();
    tabsListEl.querySelectorAll('.dragging').forEach(el => el.classList.remove('dragging'));
});

// Render Active Tab Details (Omnibox, Star, Nav buttons)
function renderActiveTab() {
    const activeTab = currentState.tabs.find(t => t.id === currentState.active_tab_id);
    if (!activeTab) return;

    // Only the exact internal pages get a browser:// display name. Everything else shows its
    // real URL, so a web page can never pass itself off as an internal page.
    const rawUrl = activeTab.url || '';
    const internal = internalPage(rawUrl);
    let displayUrl = rawUrl;
    if (internal === 'newtab' || displayUrl === 'about:blank') {
        displayUrl = '';
    } else if (internal) {
        displayUrl = 'browser://' + internal;
    }

    // Update omnibox unless the user is actively typing in it inside this focused chrome view.
    if (document.activeElement !== omniboxInputEl || !document.hasFocus()) {
        if (omniboxInputEl.value !== displayUrl) {
            omniboxInputEl.value = displayUrl;
        }
    }
    renderPrettyUrl(displayUrl);
    document.documentElement.toggleAttribute('data-loading', Boolean(activeTab.is_loading));

    // Omnibox Security / Type SVG Icon (only update DOM when icon state changes)
    let iconKind = 'search';
    if (internal) {
        iconKind = 'internal';
    } else if (rawUrl.startsWith('https://')) {
        iconKind = 'secure';
    } else if (rawUrl.startsWith('http://')) {
        iconKind = 'insecure';
    }

    if (iconKind !== lastIconKind) {
        lastIconKind = iconKind;
        if (iconKind === 'internal') {
            omniboxIconEl.innerHTML = ICONS.bolt;
            omniboxIconEl.className = 'omnibox-icon internal';
            omniboxIconEl.removeAttribute('title');
        } else if (iconKind === 'secure') {
            omniboxIconEl.innerHTML = ICONS.lock;
            omniboxIconEl.className = 'omnibox-icon secure';
            omniboxIconEl.removeAttribute('title');
        } else if (iconKind === 'insecure') {
            omniboxIconEl.innerHTML = ICONS.warning;
            omniboxIconEl.className = 'omnibox-icon insecure';
            omniboxIconEl.title = tr('ui.insecure', 'Yhteys ei ole salattu (http). Älä syötä salasanoja.');
        } else {
            omniboxIconEl.innerHTML = ICONS.search;
            omniboxIconEl.className = 'omnibox-icon';
            omniboxIconEl.removeAttribute('title');
        }
    }

    // Zoom indicator (only when not 100 %)
    const zoom = Math.round((activeTab.zoom || 1) * 100);
    zoomBadgeEl.classList.toggle('hidden', zoom === 100);
    zoomBadgeEl.textContent = `${zoom}%`;

    // Bookmark star status SVG
    const normalizedActive = normalizeUrl(rawUrl);
    const isBookmarked =
        !internal &&
        rawUrl !== 'about:blank' &&
        currentState.bookmarks.some(b => normalizeUrl(b.url) === normalizedActive);

    if (isBookmarked !== lastBookmarked) {
        lastBookmarked = isBookmarked;
        if (isBookmarked) {
            btnStarBookmarkEl.innerHTML = ICONS.starFilled;
            btnStarBookmarkEl.classList.add('bookmarked');
            btnStarBookmarkEl.title = tr('ui.bookmark_remove', 'Poista kirjanmerkeistä');
        } else {
            btnStarBookmarkEl.innerHTML = ICONS.starOutline;
            btnStarBookmarkEl.classList.remove('bookmarked');
            btnStarBookmarkEl.title = tr('ui.bookmark_add', 'Lisää kirjanmerkkeihin (Ctrl+D)');
        }
    }

    // Nav buttons
    // Dim only (stay clickable): SPA sites can change history without any event reaching us.
    btnBackEl.classList.toggle('dim', !activeTab.can_go_back);
    btnForwardEl.classList.toggle('dim', !activeTab.can_go_forward);

    // Reload icon indicator
    const reloadTitle = activeTab.is_loading
        ? tr('ui.loading', 'Ladataan... (Napsauta päivittääksesi)')
        : tr('ui.reload', 'Päivitä sivu (Ctrl+R / F5)');
    if (btnReloadEl.title !== reloadTitle) {
        btnReloadEl.title = reloadTitle;
    }
}

// Bookmarks bar: root bookmarks as chips, folders open a native menu.
function renderBookmarksBar() {
    const show = Boolean(currentState.settings && currentState.settings.show_bookmarks_bar);
    bookmarksBarEl.classList.toggle('hidden', !show);
    if (!show) return;
    const bookmarks = currentState.bookmarks || [];
    const key = JSON.stringify(bookmarks.map(b => [b.url, b.title, b.folder]));
    if (key === lastBarKey) return;
    lastBarKey = key;
    bookmarksBarEl.replaceChildren();

    const folders = [...new Set(bookmarks.map(b => b.folder || '').filter(Boolean))];
    for (const folder of folders) {
        const chip = document.createElement('button');
        chip.className = 'bm-chip bm-folder';
        chip.innerHTML = ICONS.folder;
        const label = document.createElement('span');
        label.textContent = folder;
        chip.appendChild(label);
        chip.addEventListener('click', () => sendIpc('bookmark_folder_menu', { folder }));
        bookmarksBarEl.appendChild(chip);
    }
    for (const b of bookmarks.filter(b => !b.folder)) {
        const chip = document.createElement('button');
        chip.className = 'bm-chip';
        chip.title = `${b.title}\n${b.url}`;
        const icon = viimaFavicons.get(b.url);
        if (icon) {
            const img = document.createElement('img');
            img.alt = '';
            img.src = icon;
            chip.appendChild(img);
        } else {
            const letter = document.createElement('span');
            letter.className = 'bm-letter';
            letter.textContent = (b.title || b.url || '?').trim().charAt(0).toUpperCase();
            chip.appendChild(letter);
        }
        const label = document.createElement('span');
        label.textContent = b.title || b.url;
        chip.appendChild(label);
        chip.addEventListener('click', () => sendIpc('navigate', { url: b.url }));
        chip.addEventListener('auxclick', (e) => {
            if (e.button === 1) sendIpc('new_tab', { url: b.url });
        });
        bookmarksBarEl.appendChild(chip);
    }
    if (!bookmarks.length) {
        const empty = document.createElement('span');
        empty.className = 'bm-empty';
        empty.textContent = tr('ui.bar_empty', 'Lisää kirjanmerkkejä tähtipainikkeella (Ctrl+D)');
        bookmarksBarEl.appendChild(empty);
    }
}

function renderDownloads() {
    const d = currentState.downloads || { active: 0, percent: -1, total: 0 };
    btnDownloadsEl.classList.toggle('hidden', !d.total);
    btnDownloadsEl.classList.toggle('dl-active', d.active > 0);
    let text = '';
    if (d.active > 0) text = d.percent >= 0 ? `${d.percent}%` : String(d.active);
    downloadsTextEl.textContent = text;
    downloadsProgressEl.style.width = d.active > 0 && d.percent >= 0 ? `${d.percent}%` : '0';
}

// Receive State Update from Rust Backend
window.onBrowserStateUpdate = function(state) {
    // Light updates carry only tabs; merge so bookmarks/settings survive.
    currentState = Object.assign(currentState, state);
    state = currentState;
    document.documentElement.toggleAttribute('data-private', Boolean(state.private));
    viimaFavicons.want([...state.tabs.map(t => t.url), ...(state.bookmarks || []).map(b => b.url)]);
    renderTabs();
    renderActiveTab();
    renderBookmarksBar();
    renderDownloads();

    // Update Adblock button appearance
    const active = state.tabs.find(t => t.id === state.active_tab_id);
    if (state.settings && state.settings.adblock_enabled) {
        btnAdblockEl.classList.add('shield-active');
        const blocked = active && active.blocked ? active.blocked : 0;
        if (adblockTextEl) adblockTextEl.textContent = blocked ? (blocked > 99 ? '99+' : String(blocked)) : '';
        btnAdblockEl.title = (blocked
            ? tr('ui.blocked_n', '{n} mainos- tai seurantapyyntöä estetty tällä sivulla', { n: blocked })
            : tr('ui.adblock_on', 'Mainosesto päällä')) + ' — ' + tr('ui.shield_click', 'napsauta: sivustokohtaiset asetukset');
    } else {
        btnAdblockEl.classList.remove('shield-active');
        if (adblockTextEl) adblockTextEl.textContent = tr('ui.off', 'Ei käytössä');
        btnAdblockEl.title = tr('ui.adblock_off', 'Mainosesto pois päältä');
    }

    // Update app update pill button when an update is available or ready
    if (btnUpdateAppEl) {
        const upd = state.update;
        if (upd && (upd.state === 'available' || upd.state === 'ready' || upd.state === 'downloading')) {
            btnUpdateAppEl.classList.remove('hidden');
            btnUpdateAppEl.classList.toggle('state-ready', upd.state === 'ready');
            btnUpdateAppEl.disabled = upd.state === 'downloading';
            const ver = upd.latest_version ? ` v${upd.latest_version}` : '';
            if (upd.state === 'ready') {
                if (updateAppTextEl) updateAppTextEl.textContent = tr('ui.update_restart', 'Käynnistä uudelleen') + ver;
                btnUpdateAppEl.title = tr('ui.update_ready', 'Päivitys valmiina asennettavaksi — napsauta käynnistääksesi uudelleen');
            } else if (upd.state === 'downloading') {
                if (updateAppTextEl) updateAppTextEl.textContent = tr('ui.update_downloading', 'Ladataan...');
                btnUpdateAppEl.title = tr('ui.update_downloading_title', 'Ladataan päivitystä...');
            } else {
                if (updateAppTextEl) updateAppTextEl.textContent = tr('ui.update', 'Päivitä') + ver;
                btnUpdateAppEl.title = tr('ui.update_available', 'Uusi versio saatavilla — napsauta ladataksesi');
            }
        } else {
            btnUpdateAppEl.classList.add('hidden');
            btnUpdateAppEl.classList.remove('state-ready');
            btnUpdateAppEl.disabled = false;
        }
    }

    if (storageErrorEl) {
        const err = state.storage_error || '';
        if (err && err !== lastDismissedError) {
            storageErrorEl.textContent = err;
            storageErrorEl.hidden = false;
        } else if (!err) {
            storageErrorEl.textContent = '';
            storageErrorEl.hidden = true;
            lastDismissedError = '';
        }
    }
};

if (storageErrorEl) {
    storageErrorEl.addEventListener('click', () => {
        lastDismissedError = storageErrorEl.textContent || '';
        storageErrorEl.hidden = true;
    });
}

// Event Listeners for Nav & Actions
btnNewTabEl.addEventListener('click', () => {
    omniboxInputEl.blur();
    sendIpc('new_tab', { url: 'browser://newtab' });
});

btnBackEl.addEventListener('click', () => sendIpc('back'));
btnForwardEl.addEventListener('click', () => sendIpc('forward'));
btnReloadEl.addEventListener('click', () => sendIpc('reload'));
btnHomeEl.addEventListener('click', () => sendIpc('navigate', { url: 'browser://newtab' }));
btnStarBookmarkEl.addEventListener('click', () => sendIpc('toggle_bookmark'));
zoomBadgeEl.addEventListener('click', () => sendIpc('zoom', { dir: 0 }));
btnAdblockEl.addEventListener('click', () => sendIpc('shield_menu'));
btnDownloadsEl.addEventListener('click', () => sendIpc('new_tab', { url: 'browser://downloads' }));
document.getElementById('btnTheme').addEventListener('click', () => sendIpc('toggle_theme'));

// Bookmarks, history, downloads and settings open in a tab of their own (an open one is reused),
// so the page you're on stays put.
btnBookmarksMenuEl.addEventListener('click', () => sendIpc('new_tab', { url: 'browser://bookmarks' }));
btnHistoryMenuEl.addEventListener('click', () => sendIpc('new_tab', { url: 'browser://history' }));
btnSettingsMenuEl.addEventListener('click', () => sendIpc('new_tab', { url: 'browser://settings' }));

// Icons arrive from Rust a moment after the tabs/bookmarks they belong to.
viimaFavicons.onUpdate(() => {
    renderTabs();
    lastBarKey = '';
    renderBookmarksBar();
});

if (btnUpdateAppEl) {
    btnUpdateAppEl.addEventListener('click', () => {
        const stateName = currentState.update && currentState.update.state;
        if (stateName === 'ready') {
            sendIpc('install_update');
        } else if (stateName === 'available') {
            sendIpc('download_update');
        }
    });
}

// Omnibox submit on Enter
omniboxInputEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
        const val = omniboxInputEl.value.trim();
        if (val) {
            sendIpc('navigate', { url: val });
            omniboxInputEl.blur();
        }
    }
});

// Inline autocomplete from history/bookmarks: Rust answers via window.onAutocomplete.
// Only on insertions with the caret at the end, so Backspace/Delete never fight the user.
omniboxInputEl.addEventListener('input', (e) => {
    syncPrettyUrl();
    if (e.inputType && !e.inputType.startsWith('insert')) return;
    const value = omniboxInputEl.value;
    if (omniboxInputEl.selectionStart !== value.length) return;
    sendIpc('autocomplete', { q: value });
});

window.onAutocomplete = function(typed, suffix) {
    if (!suffix || omniboxInputEl.value !== typed || document.activeElement !== omniboxInputEl) return;
    omniboxInputEl.value = typed + suffix;
    omniboxInputEl.setSelectionRange(typed.length, typed.length + suffix.length);
};

// Clear stale activeElement focus when user clicks into a page webview
window.addEventListener('blur', () => {
    if (document.activeElement === omniboxInputEl) {
        omniboxInputEl.blur();
    }
});

// No browser context menu on the toolbar itself (the omnibox keeps cut/copy/paste).
document.addEventListener('contextmenu', (e) => {
    if (!e.target.closest('input')) e.preventDefault();
});

// Keyboard shortcuts inside Chrome UI
window.addEventListener('keydown', (e) => {
    const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey;
    const key = (e.key || '').toLowerCase();
    let handled = true;
    if (ctrl && key === 'tab') {
        sendIpc('shortcut', { key: e.shiftKey ? 'prev_tab' : 'next_tab' });
    } else if (ctrl && e.shiftKey && key === 't') {
        sendIpc('shortcut', { key: 'reopen_tab' });
    } else if (ctrl && e.shiftKey && key === 'n') {
        sendIpc('private_window');
    } else if (ctrl && e.shiftKey && key === 'b') {
        sendIpc('shortcut', { key: 'bookmarks_bar' });
    } else if ((ctrl && e.shiftKey && key === 'i') || e.key === 'F12') {
        sendIpc('devtools');
    } else if (ctrl && !e.shiftKey && /^[1-9]$/.test(key)) {
        sendIpc('shortcut', { key: 'tab_' + key });
    } else if (ctrl && (key === '+' || key === '=' || e.code === 'NumpadAdd')) {
        sendIpc('zoom', { dir: 1 });
    } else if (ctrl && (key === '-' || e.code === 'NumpadSubtract')) {
        sendIpc('zoom', { dir: -1 });
    } else if (ctrl && key === '0') {
        sendIpc('zoom', { dir: 0 });
    } else if (ctrl && !e.shiftKey && key === 't') {
        sendIpc('new_tab', { url: 'browser://newtab' });
    } else if (ctrl && !e.shiftKey && key === 'w') {
        sendIpc('close_tab', { id: currentState.active_tab_id });
    } else if (ctrl && !e.shiftKey && key === 'l') {
        window.focusOmnibox();
    } else if (ctrl && !e.shiftKey && key === 'r') {
        sendIpc('reload');
    } else if (ctrl && !e.shiftKey && key === 'd') {
        sendIpc('toggle_bookmark');
    } else if (ctrl && !e.shiftKey && key === 'h') {
        sendIpc('new_tab', { url: 'browser://history' });
    } else if (ctrl && !e.shiftKey && key === 'j') {
        sendIpc('new_tab', { url: 'browser://downloads' });
    } else if (ctrl && !e.shiftKey && key === 'f') {
        sendIpc('find');
    } else if (ctrl && !e.shiftKey && key === 'p') {
        sendIpc('print');
    } else if (e.key === 'F5' && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        sendIpc('reload');
    } else {
        handled = false;
    }
    if (handled) e.preventDefault();
});

// Window Controls (Minimize, Maximize, Close)
if (btnWinMinimizeEl) {
    btnWinMinimizeEl.addEventListener('click', () => sendIpc('minimize'));
}
if (btnWinMaximizeEl) {
    btnWinMaximizeEl.addEventListener('click', () => sendIpc('maximize'));
    // Windows 11 Snap Layouts: hovering the maximize button opens the layout flyout.
    let snapTimer = 0;
    btnWinMaximizeEl.addEventListener('mouseenter', () => {
        clearTimeout(snapTimer);
        snapTimer = setTimeout(() => sendIpc('snap_layouts'), 700);
    });
    btnWinMaximizeEl.addEventListener('mouseleave', () => clearTimeout(snapTimer));
    btnWinMaximizeEl.addEventListener('mousedown', () => clearTimeout(snapTimer));
}
if (btnWinCloseEl) {
    btnWinCloseEl.addEventListener('click', () => sendIpc('close'));
}

// Window Dragging & Double-click maximize on top bar
const tabStrip = document.querySelector('.tab-strip-container');
if (tabStrip) {
    tabStrip.addEventListener('mousedown', (e) => {
        // Only drag if not clicking buttons or tabs
        if (e.target.closest('.win-btn') || e.target.closest('.btn-new-tab') || e.target.closest('.tab')) {
            return;
        }
        if (e.button === 0) {
            if (e.detail === 2) {
                sendIpc('maximize');
            } else {
                sendIpc('drag_window');
            }
        }
    });
}

// Also called from Rust (Ctrl+L while a web page has focus)
window.focusOmnibox = function() {
    omniboxInputEl.focus();
    omniboxInputEl.select();
};

// Request initial state on boot (Rust also pushes it when the page finishes loading)
document.addEventListener('DOMContentLoaded', () => {
    sendIpc('request_initial_state');
});
