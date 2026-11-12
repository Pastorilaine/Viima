#![cfg(windows)]
#![allow(dead_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HWND};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IPersistFile,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT,
    SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify, SHGetKnownFolderPath, ShellExecuteW,
    ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, MessageBoxW, SW_SHOWNORMAL,
};
use windows::core::{GUID, HSTRING, Interface, PCWSTR};

const UNINSTALL_REG_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Viima";
const LEGACY_UNINSTALL_REG_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\OmaSelain";
const WEBVIEW2_CLIENT_GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

pub fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn known_folder(folder_id: &GUID) -> Option<PathBuf> {
    unsafe {
        let ptr = SHGetKnownFolderPath(folder_id, KF_FLAG_DEFAULT, None).ok()?;
        let path_str = ptr.to_string().ok();
        CoTaskMemFree(Some(ptr.0 as *const _));
        path_str.map(PathBuf::from)
    }
}

pub fn default_install_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_LocalAppData).map(|d| d.join("IT-Veljekset").join("Viima"))
}

pub fn desktop_shortcut_path() -> Option<PathBuf> {
    known_folder(&FOLDERID_Desktop).map(|d| d.join("Viima.lnk"))
}

pub fn start_menu_shortcut_path() -> Option<PathBuf> {
    known_folder(&FOLDERID_Programs).map(|d| d.join("Viima.lnk"))
}

pub fn legacy_desktop_shortcut_path() -> Option<PathBuf> {
    known_folder(&FOLDERID_Desktop).map(|d| d.join("Oma Selain.lnk"))
}

pub fn legacy_start_menu_shortcut_path() -> Option<PathBuf> {
    known_folder(&FOLDERID_Programs).map(|d| d.join("Oma Selain.lnk"))
}

pub fn create_shortcut(
    shortcut_path: &Path,
    target_exe: &Path,
    description: &str,
) -> Result<(), String> {
    if let Some(parent) = shortcut_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Kansion luonti epäonnistui: {e}"))?;
    }

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let did_init = hr.is_ok();

        let result: Result<(), String> = (|| {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("ShellLink-luonti epäonnistui: {e}"))?;

            let target_w = HSTRING::from(target_exe.as_os_str());
            link.SetPath(&target_w)
                .map_err(|e| format!("SetPath epäonnistui: {e}"))?;

            if let Some(dir) = target_exe.parent() {
                let dir_w = HSTRING::from(dir.as_os_str());
                link.SetWorkingDirectory(&dir_w)
                    .map_err(|e| format!("SetWorkingDirectory epäonnistui: {e}"))?;
            }

            let desc_w = HSTRING::from(description);
            link.SetDescription(&desc_w)
                .map_err(|e| format!("SetDescription epäonnistui: {e}"))?;

            link.SetIconLocation(&target_w, 0)
                .map_err(|e| format!("SetIconLocation epäonnistui: {e}"))?;

            let persist: IPersistFile = link
                .cast()
                .map_err(|e| format!("IPersistFile-muunnos epäonnistui: {e}"))?;
            let shortcut_w = HSTRING::from(shortcut_path.as_os_str());
            persist
                .Save(&shortcut_w, true)
                .map_err(|e| format!("Pikakuvakkeen tallennus epäonnistui: {e}"))?;
            Ok(())
        })();

        if did_init {
            CoUninitialize();
        }
        result
    }
}

fn reg_set_sz(hkey: HKEY, name: &str, value: &str) -> Result<(), String> {
    let name_w = to_wide_null(name);
    let val_w = to_wide_null(value);
    let bytes = unsafe {
        std::slice::from_raw_parts(val_w.as_ptr() as *const u8, val_w.len() * std::mem::size_of::<u16>())
    };
    let status = unsafe {
        RegSetValueExW(hkey, PCWSTR(name_w.as_ptr()), Some(0), REG_SZ, Some(bytes))
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("RegSetValueExW({name}) epäonnistui: {:?}", status))
    }
}

fn reg_set_dword(hkey: HKEY, name: &str, value: u32) -> Result<(), String> {
    let name_w = to_wide_null(name);
    let bytes = value.to_le_bytes();
    let status = unsafe {
        RegSetValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            Some(0),
            REG_DWORD,
            Some(&bytes),
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("RegSetValueExW({name}) epäonnistui: {:?}", status))
    }
}

pub fn register_uninstall_entry(
    install_dir: &Path,
    exe_path: &Path,
    version: &str,
    estimated_kb: u32,
) -> Result<(), String> {
    let subkey_w = to_wide_null(UNINSTALL_REG_PATH);
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
        return Err(format!("Rekisteriavaimen luonti epäonnistui: {:?}", status));
    }

    let exe_str = exe_path.to_string_lossy();
    let dir_str = install_dir.to_string_lossy();
    let uninstall_cmd = format!("\"{exe_str}\" --uninstall");
    let quiet_cmd = format!("\"{exe_str}\" --uninstall --quiet");
    let icon = format!("{exe_str},0");

    let res = (|| {
        reg_set_sz(hkey, "DisplayName", "Viima")?;
        reg_set_sz(hkey, "DisplayVersion", version)?;
        reg_set_sz(hkey, "Publisher", "IT-Veljekset Group")?;
        reg_set_sz(hkey, "InstallLocation", &dir_str)?;
        reg_set_sz(hkey, "DisplayIcon", &icon)?;
        reg_set_sz(hkey, "UninstallString", &uninstall_cmd)?;
        reg_set_sz(hkey, "QuietUninstallString", &quiet_cmd)?;
        reg_set_dword(hkey, "NoModify", 1)?;
        reg_set_dword(hkey, "NoRepair", 1)?;
        reg_set_dword(hkey, "EstimatedSize", estimated_kb)?;
        Ok(())
    })();

    unsafe {
        let _ = RegCloseKey(hkey);
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    res
}

pub fn remove_uninstall_entry() {
    let subkey_w = to_wide_null(UNINSTALL_REG_PATH);
    let legacy_w = to_wide_null(LEGACY_UNINSTALL_REG_PATH);
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(subkey_w.as_ptr()));
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(legacy_w.as_ptr()));
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

fn has_webview2_key(root: HKEY, subkey: &str) -> bool {
    let subkey_w = to_wide_null(subkey);
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(root, PCWSTR(subkey_w.as_ptr()), Some(0), KEY_READ, &mut hkey)
    };
    if status != ERROR_SUCCESS {
        return false;
    }

    let val_name = to_wide_null("pv");
    let mut buf_size: u32 = 128;
    let mut buf = vec![0u8; buf_size as usize];
    let q_status = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(val_name.as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut buf_size),
        )
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    q_status == ERROR_SUCCESS && buf_size > 2
}

pub fn is_webview2_installed() -> bool {
    let wow64 = format!(r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT_GUID}");
    let native = format!(r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT_GUID}");
    let user = format!(r"Software\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT_GUID}");
    has_webview2_key(HKEY_LOCAL_MACHINE, &wow64)
        || has_webview2_key(HKEY_LOCAL_MACHINE, &native)
        || has_webview2_key(HKEY_CURRENT_USER, &user)
}

pub fn wait_for_process_exit(pid: u32, timeout_ms: u32) {
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(handle, timeout_ms);
            let _ = CloseHandle(handle);
        }
    }
}

pub fn show_message_box(title: &str, text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let title_w = to_wide_null(title);
    let text_w = to_wide_null(text);
    unsafe {
        MessageBoxW(
            Some(HWND::default()),
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            flags,
        )
    }
}

pub fn shell_open_url(url: &str) {
    let open_w = to_wide_null("open");
    let url_w = to_wide_null(url);
    unsafe {
        let _ = ShellExecuteW(
            None,
            PCWSTR(open_w.as_ptr()),
            PCWSTR(url_w.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}
