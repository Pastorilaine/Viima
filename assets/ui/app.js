// State management
let currentState = {
    tabs: [],
    active_tab_id: 0,
    bookmarks: [],
    settings: {
        search_engine: 'google',
        adblock_enabled: true
    }
};

// SVG Icon Definitions (High-res, modern, geometric)
const ICONS = {
    globe: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>`,
    close: `<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>`,
    lock: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>`,
    bolt: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg>`,
    warning: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>`,
    search: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line></svg>`,
    starOutline: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`,
    starFilled: `<svg width="15" height="15" viewBox="0 0 24 24" fill="#f59e0b" stroke="#f59e0b" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`
};

// Name of the internal page for browser://x or http://browser.x (x = one of the four), else null.
function internalPage(url) {
    const m = /^(?:browser:\/\/|http:\/\/browser\.)(newtab|history|bookmarks|settings)(?:[\/?#].*)?$/.exec(url);
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
const btnStarBookmarkEl = document.getElementById('btnStarBookmark');
const btnUpdateAppEl = document.getElementById('btnUpdateApp');
const updateAppTextEl = document.getElementById('updateAppText');
const btnAdblockEl = document.getElementById('btnAdblock');
const adblockTextEl = document.getElementById('adblockText');
const btnBookmarksMenuEl = document.getElementById('btnBookmarksMenu');
const btnHistoryMenuEl = document.getElementById('btnHistoryMenu');
const btnSettingsMenuEl = document.getElementById('btnSettingsMenu');
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

// Keyed DOM reconciliation for Tabs (avoids wiping DOM & restarting spinners on every update)
function renderTabs() {
    const activeIds = new Set();

    currentState.tabs.forEach((tab, index) => {
        activeIds.add(tab.id);
        let entry = tabElements.get(tab.id);
        if (!entry) {
            const tabEl = document.createElement('div');
            tabEl.dataset.tabId = String(tab.id);
            tabEl.innerHTML = `
                <span class="tab-icon">${ICONS.globe}</span>
                <span class="tab-spinner"></span>
                <span class="tab-title"></span>
                <span class="tab-close" title="Sulje välilehti (Ctrl+W)">${ICONS.close}</span>
            `;
            entry = {
                el: tabEl,
                titleEl: tabEl.querySelector('.tab-title'),
                iconEl: tabEl.querySelector('.tab-icon'),
                favicon: null,
            };
            tabElements.set(tab.id, entry);
        }

        // Site favicon, falling back to the globe if it fails to load.
        const favicon = internalPage(tab.url || '') ? null : (tab.favicon || null);
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

        const nextClass =
            'tab' +
            (tab.id === currentState.active_tab_id ? ' active' : '') +
            (tab.is_loading ? ' loading' : '');
        if (entry.el.className !== nextClass) {
            entry.el.className = nextClass;
        }

        const nextTooltip = tab.title || tab.url || '';
        if (entry.el.title !== nextTooltip) {
            entry.el.title = nextTooltip;
        }

        let titleText = tab.title || '';
        if (!titleText || internalPage(titleText) === 'newtab') {
            titleText = 'Uusi välilehti';
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

// Delegated tab strip interactions
tabsListEl.addEventListener('click', (e) => {
    const tabEl = e.target.closest('.tab');
    if (!tabEl) return;
    const id = Number(tabEl.dataset.tabId);
    if (!id) return;
    if (e.target.closest('.tab-close')) {
        sendIpc('close_tab', { id });
    } else {
        omniboxInputEl.blur();
        sendIpc('switch_tab', { id });
    }
});

// Middle click closes a tab (mousedown preventDefault stops the autoscroll cursor).
tabsListEl.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();
});
tabsListEl.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    const tabEl = e.target.closest('.tab');
    const id = tabEl && Number(tabEl.dataset.tabId);
    if (id) {
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

// Render Active Tab Details (Omnibox, Star, Nav buttons)
function renderActiveTab() {
    const activeTab = currentState.tabs.find(t => t.id === currentState.active_tab_id);
    if (!activeTab) return;

    // Omnibox URL Formatting
    // Only the four exact internal pages get a browser:// display name. Everything else shows its
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
            omniboxIconEl.title = 'Yhteys ei ole salattu (http). Älä syötä salasanoja.';
        } else {
            omniboxIconEl.innerHTML = ICONS.search;
            omniboxIconEl.className = 'omnibox-icon';
            omniboxIconEl.removeAttribute('title');
        }
    }

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
            btnStarBookmarkEl.title = 'Poista kirjanmerkeistä';
        } else {
            btnStarBookmarkEl.innerHTML = ICONS.starOutline;
            btnStarBookmarkEl.classList.remove('bookmarked');
            btnStarBookmarkEl.title = 'Lisää kirjanmerkkeihin';
        }
    }

    // Nav buttons
    // Dim only (stay clickable): SPA sites can change history without any event reaching us.
    btnBackEl.classList.toggle('dim', !activeTab.can_go_back);
    btnForwardEl.classList.toggle('dim', !activeTab.can_go_forward);

    // Reload icon indicator
    const reloadTitle = activeTab.is_loading ? 'Ladataan... (Napsauta päivittääksesi)' : 'Päivitä sivu (Ctrl+R)';
    if (btnReloadEl.title !== reloadTitle) {
        btnReloadEl.title = reloadTitle;
    }
}

// Receive State Update from Rust Backend
window.onBrowserStateUpdate = function(state) {
    // Light updates carry only tabs; merge so bookmarks/settings survive.
    currentState = Object.assign(currentState, state);
    state = currentState;
    renderTabs();
    renderActiveTab();

    // Update Adblock button appearance
    if (state.settings && state.settings.adblock_enabled) {
        btnAdblockEl.classList.add('shield-active');
        const active = state.tabs.find(t => t.id === state.active_tab_id);
        const blocked = active && active.blocked ? active.blocked : 0;
        if (adblockTextEl) adblockTextEl.textContent = blocked ? `Suojattu · ${blocked}` : 'Suojattu';
        btnAdblockEl.title = blocked
            ? `${blocked} mainos- tai seurantapyyntöä estetty tällä sivulla`
            : 'Mainosesto päällä';
    } else {
        btnAdblockEl.classList.remove('shield-active');
        if (adblockTextEl) adblockTextEl.textContent = 'Ei käytössä';
        btnAdblockEl.title = 'Mainosesto pois päältä';
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
                if (updateAppTextEl) updateAppTextEl.textContent = `Käynnistä uudelleen${ver}`;
                btnUpdateAppEl.title = `Päivitys${ver} valmiina asennettavaksi — napsauta käynnistääksesi uudelleen`;
            } else if (upd.state === 'downloading') {
                if (updateAppTextEl) updateAppTextEl.textContent = 'Ladataan...';
                btnUpdateAppEl.title = 'Ladataan päivitystä...';
            } else {
                if (updateAppTextEl) updateAppTextEl.textContent = `Päivitä${ver}`;
                btnUpdateAppEl.title = `Uusi versio${ver} saatavilla — napsauta ladataksesi`;
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

btnHomeEl.addEventListener('click', () => {
    sendIpc('navigate', { url: 'browser://newtab' });
});

btnStarBookmarkEl.addEventListener('click', () => {
    sendIpc('toggle_bookmark');
});

btnAdblockEl.addEventListener('click', () => {
    sendIpc('toggle_adblock');
});

document.getElementById('btnTheme').addEventListener('click', () => {
    sendIpc('toggle_theme');
});

// Navigate directly to internal pages on toolbar clicks
btnBookmarksMenuEl.addEventListener('click', () => {
    sendIpc('navigate', { url: 'browser://bookmarks' });
});

btnHistoryMenuEl.addEventListener('click', () => {
    sendIpc('navigate', { url: 'browser://history' });
});

btnSettingsMenuEl.addEventListener('click', () => {
    sendIpc('navigate', { url: 'browser://settings' });
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

// Keyboard shortcuts inside Chrome UI
window.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key === 'Tab') {
        e.preventDefault();
        sendIpc('shortcut', { key: e.shiftKey ? 'prev_tab' : 'next_tab' });
    } else if ((e.ctrlKey || e.metaKey) && !e.altKey && e.shiftKey && e.key.toLowerCase() === 't') {
        e.preventDefault();
        sendIpc('shortcut', { key: 'reopen_tab' });
    } else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) {
        const key = e.key.toLowerCase();
        if (/^[1-9]$/.test(key)) {
            e.preventDefault();
            sendIpc('shortcut', { key: 'tab_' + key });
        } else if (key === 't') {
            e.preventDefault();
            sendIpc('new_tab', { url: 'browser://newtab' });
        } else if (key === 'w') {
            e.preventDefault();
            sendIpc('close_tab', { id: currentState.active_tab_id });
        } else if (key === 'l') {
            e.preventDefault();
            window.focusOmnibox();
        } else if (key === 'r') {
            e.preventDefault();
            sendIpc('reload');
        } else if (key === 'd') {
            e.preventDefault();
            sendIpc('toggle_bookmark');
        } else if (key === 'h') {
            e.preventDefault();
            sendIpc('navigate', { url: 'browser://history' });
        }
    } else if (e.key === 'F5' && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        sendIpc('reload');
    }
});

// Window Controls (Minimize, Maximize, Close)
if (btnWinMinimizeEl) {
    btnWinMinimizeEl.addEventListener('click', () => sendIpc('minimize'));
}
if (btnWinMaximizeEl) {
    btnWinMaximizeEl.addEventListener('click', () => sendIpc('maximize'));
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
