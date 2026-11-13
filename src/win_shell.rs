//! Windows integration used by the browser itself: single instance, native popup menus,
//! Snap Layouts, Explorer, default-browser registration and the taskbar Jump List.
#![cfg(windows)]

use crate::win_platform::{shell_open_url, to_wide_null};
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegSetValueExW,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify, ShellLink,
};
use windows::core::{HSTRING, Interface, PCWSTR};

// ---------------------------------------------------------------------------------------------
// Single instance: a second `Viima.exe <url>` hands its URLs to the running window and exits.
// ---------------------------------------------------------------------------------------------

pub const MAIN_WINDOW_CLASS: &str = "ViimaBrowserWindow";
const COPYDATA_MAGIC: usize = 0x5649_494D; // "VIIM"

/// Returns true if another Viima window took `payload` (newline-separated URLs; may be empty).
/// The first instance keeps the mutex for its whole lifetime.
pub fn forward_to_running_instance(payload: &str) -> bool {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, SMTO_ABORTIFHUNG,
        SendMessageTimeoutW, WM_COPYDATA,
    };

    unsafe {
        let name = HSTRING::from("Local\\ViimaBrowserSingleInstance");
        let Ok(handle) = CreateMutexW(None, false, &name) else {
            return false;
        };
        if GetLastError() != ERROR_ALREADY_EXISTS {
            // We are the first instance: the handle stays open until exit.
            let _ = handle; // never closed: released by Windows when the process exits
            return false;
        }
        let _ = CloseHandle(handle);

        let class = HSTRING::from(MAIN_WINDOW_CLASS);
        let wide: Vec<u16> = payload.encode_utf16().collect();
        // The first instance may still be creating its window: retry for ~3 s.
        for _ in 0..30 {
            if let Ok(hwnd) = FindWindowW(&class, PCWSTR::null())
                && !hwnd.is_invalid()
            {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let _ = AllowSetForegroundWindow(pid);
                let cds = COPYDATASTRUCT {
                    dwData: COPYDATA_MAGIC,
                    cbData: (wide.len() * 2) as u32,
                    lpData: wide.as_ptr() as *mut _,
                };
                let mut result = 0usize;
                SendMessageTimeoutW(
                    hwnd,
                    WM_COPYDATA,
                    WPARAM(0),
                    LPARAM(&cds as *const _ as isize),
                    SMTO_ABORTIFHUNG,
                    5000,
                    Some(&mut result),
                );
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }
}

type CopyDataCallback = Box<dyn Fn(String)>;

unsafe extern "system" fn copydata_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::WM_COPYDATA;
    unsafe {
        if msg == WM_COPYDATA && lparam.0 != 0 {
            let cds = &*(lparam.0 as *const COPYDATASTRUCT);
            if cds.dwData == COPYDATA_MAGIC && cds.cbData <= 64 * 1024 {
                let len = cds.cbData as usize / 2;
                let text = if len == 0 || cds.lpData.is_null() {
                    String::new()
                } else {
                    String::from_utf16_lossy(std::slice::from_raw_parts(
                        cds.lpData as *const u16,
                        len,
                    ))
                };
                let cb = &*(data as *const CopyDataCallback);
                cb(text);
                return LRESULT(1);
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

/// Calls `callback` with the payload of `forward_to_running_instance` (on the UI thread).
pub fn install_copydata_hook(hwnd: isize, callback: CopyDataCallback) {
    use windows::Win32::UI::Shell::SetWindowSubclass;
    let data = Box::into_raw(Box::new(callback)) as usize; // lives as long as the window
    unsafe {
        let _ = SetWindowSubclass(HWND(hwnd as *mut _), Some(copydata_subclass_proc), 1, data);
    }
}

/// Brings the window to the front even when it is minimized.
pub fn bring_to_front(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

// ---------------------------------------------------------------------------------------------
// Native popup menus (tab context menu, shield menu, bookmark folders). The chrome webview is
// only as tall as the toolbar, so HTML menus would be clipped.
// ---------------------------------------------------------------------------------------------

pub enum MenuEntry {
    Item {
        id: u32,
        label: String,
        checked: bool,
        enabled: bool,
    },
    Separator,
}

impl MenuEntry {
    pub fn item(id: u32, label: impl Into<String>) -> Self {
        Self::Item {
            id,
            label: label.into(),
            checked: false,
            enabled: true,
        }
    }
    pub fn check(id: u32, label: impl Into<String>, checked: bool) -> Self {
        Self::Item {
            id,
            label: label.into(),
            checked,
            enabled: true,
        }
    }
    pub fn info(label: impl Into<String>) -> Self {
        Self::Item {
            id: 0,
            label: label.into(),
            checked: false,
            enabled: false,
        }
    }
}

/// Shows a menu at the mouse cursor and returns the chosen id (None if dismissed).
pub fn popup_menu(hwnd: isize, entries: &[MenuEntry]) -> Option<u32> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, MF_CHECKED, MF_GRAYED,
        MF_SEPARATOR, MF_STRING, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON,
        TrackPopupMenu,
    };
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for entry in entries {
            match entry {
                MenuEntry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                MenuEntry::Item {
                    id,
                    label,
                    checked,
                    enabled,
                } => {
                    let mut flags = MF_STRING;
                    if *checked {
                        flags |= MF_CHECKED;
                    }
                    if !*enabled {
                        flags |= MF_GRAYED;
                    }
                    // `&` marks an accelerator in menu text; page titles must show it literally.
                    let text = HSTRING::from(label.replace('&', "&&").as_str());
                    let _ = AppendMenuW(menu, flags, *id as usize, &text);
                }
            }
        }
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        (cmd.0 > 0).then_some(cmd.0 as u32)
    }
}

/// Windows 11 Snap Layouts: a frameless window has no native maximize button to hover, so open
/// the same flyout with Win+Z — only while our window is the active one.
pub fn show_snap_layouts(hwnd: isize) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
        SendInput, VIRTUAL_KEY, VK_LWIN,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe {
        if GetForegroundWindow().0 as isize != hwnd {
            return;
        }
        let key = |vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let z = VIRTUAL_KEY(b'Z' as u16);
        let inputs = [
            key(VK_LWIN, KEYBD_EVENT_FLAGS(0)),
            key(z, KEYBD_EVENT_FLAGS(0)),
            key(z, KEYEVENTF_KEYUP),
            key(VK_LWIN, KEYEVENTF_KEYUP),
        ];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

// ---------------------------------------------------------------------------------------------
// Files: open a download, show it in Explorer.
// ---------------------------------------------------------------------------------------------

pub fn open_path(path: &str) {
    if Path::new(path).exists() {
        shell_open_url(path);
    }
}

pub fn show_in_folder(path: &str) {
    use std::os::windows::process::CommandExt;
    let path = path.replace('"', "");
    let _ = std::process::Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{path}\""))
        .spawn();
}

// ---------------------------------------------------------------------------------------------
// Default browser registration (per user, no admin rights needed).
// ---------------------------------------------------------------------------------------------

const CLIENT_KEY: &str = r"Software\Clients\StartMenuInternet\Viima";
const PROGID_URL: &str = "ViimaURL";
const PROGID_HTML: &str = "ViimaHTML";

fn reg_set(path: &str, name: &str, value: &str) -> Result<(), String> {
    let subkey_w = to_wide_null(path);
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey_w.as_ptr()),
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("Rekisteriavaimen {path} luonti epäonnistui: {status:?}"));
    }
    let name_w = to_wide_null(name);
    let val_w = to_wide_null(value);
    let bytes = unsafe { std::slice::from_raw_parts(val_w.as_ptr() as *const u8, val_w.len() * 2) };
    let status = unsafe {
        let s = RegSetValueExW(hkey, PCWSTR(name_w.as_ptr()), Some(0), REG_SZ, Some(bytes));
        let _ = RegCloseKey(hkey);
        s
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("Rekisteriarvon {path}\\{name} kirjoitus epäonnistui: {status:?}"))
    }
}

/// Registers Viima under "Default apps" (http/https, .htm/.html/.pdf) for the current user.
pub fn register_browser_capabilities(exe: &Path) -> Result<(), String> {
    let exe = exe.to_string_lossy();
    let open_cmd = format!("\"{exe}\" \"%1\"");
    let icon = format!("{exe},0");
    let caps = format!(r"{CLIENT_KEY}\Capabilities");
    for (progid, desc) in [(PROGID_URL, "Viima URL"), (PROGID_HTML, "Viima HTML Document")] {
        let base = format!(r"Software\Classes\{progid}");
        reg_set(&base, "", desc)?;
        reg_set(&format!(r"{base}\DefaultIcon"), "", &icon)?;
        reg_set(&format!(r"{base}\shell\open\command"), "", &open_cmd)?;
    }
    reg_set(&format!(r"Software\Classes\{PROGID_URL}"), "URL Protocol", "")?;
    reg_set(CLIENT_KEY, "", "Viima")?;
    reg_set(&format!(r"{CLIENT_KEY}\DefaultIcon"), "", &icon)?;
    reg_set(&format!(r"{CLIENT_KEY}\shell\open\command"), "", &format!("\"{exe}\""))?;
    reg_set(&caps, "ApplicationName", "Viima")?;
    reg_set(&caps, "ApplicationIcon", &icon)?;
    reg_set(&caps, "ApplicationDescription", "Viima — nopea ja kevyt verkkoselain")?;
    reg_set(&format!(r"{caps}\StartMenu"), "StartMenuInternet", "Viima")?;
    for scheme in ["http", "https"] {
        reg_set(&format!(r"{caps}\URLAssociations"), scheme, PROGID_URL)?;
    }
    for ext in [".htm", ".html", ".xhtml", ".pdf"] {
        reg_set(&format!(r"{caps}\FileAssociations"), ext, PROGID_HTML)?;
    }
    reg_set(r"Software\RegisteredApplications", "Viima", &caps)?;
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    Ok(())
}

pub fn unregister_browser_capabilities() {
    for key in [
        CLIENT_KEY,
        r"Software\Classes\ViimaURL",
        r"Software\Classes\ViimaHTML",
    ] {
        let w = to_wide_null(key);
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(w.as_ptr()));
        }
    }
    let apps = to_wide_null(r"Software\RegisteredApplications");
    let name = to_wide_null("Viima");
    unsafe {
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(apps.as_ptr()),
            PCWSTR(name.as_ptr()),
        );
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

/// Registers the capabilities and opens Windows "Default apps" on Viima's page.
pub fn open_default_apps_settings(exe: &Path) -> Result<(), String> {
    register_browser_capabilities(exe)?;
    shell_open_url("ms-settings:defaultapps?registeredAppUser=Viima");
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Taskbar Jump List: "New tab", "New private window" and recent bookmarks.
// ---------------------------------------------------------------------------------------------

fn shell_link(exe: &Path, args: &str, title: &str) -> windows::core::Result<IShellLinkW> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(exe.as_os_str()))?;
        link.SetArguments(&HSTRING::from(args))?;
        link.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0)?;
        link.SetDescription(&HSTRING::from(title))?;
        let store: IPropertyStore = link.cast()?;
        store.SetValue(&PKEY_Title, &PROPVARIANT::from(title))?;
        store.Commit()?;
        Ok(link)
    }
}

/// `tasks` and `bookmarks` are (title, command-line arguments). Call from a background thread.
pub fn set_jump_list(
    exe: &Path,
    tasks: &[(String, String)],
    bookmarks_title: &str,
    bookmarks: &[(String, String)],
) -> windows::core::Result<()> {
    use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
    use windows::Win32::UI::Shell::{
        DestinationList, EnumerableObjectCollection, ICustomDestinationList,
    };
    unsafe {
        let did_init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| {
            let list: ICustomDestinationList =
                CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
            let mut slots = 0u32;
            let _removed: IObjectArray = list.BeginList(&mut slots)?;

            if !bookmarks.is_empty() {
                let coll: IObjectCollection =
                    CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
                for (title, args) in bookmarks {
                    coll.AddObject(&shell_link(exe, args, title)?)?;
                }
                let arr: IObjectArray = coll.cast()?;
                list.AppendCategory(&HSTRING::from(bookmarks_title), &arr)?;
            }

            let coll: IObjectCollection =
                CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
            for (title, args) in tasks {
                coll.AddObject(&shell_link(exe, args, title)?)?;
            }
            let arr: IObjectArray = coll.cast()?;
            list.AddUserTasks(&arr)?;
            list.CommitList()
        })();
        if did_init {
            CoUninitialize();
        }
        result
    }
}
