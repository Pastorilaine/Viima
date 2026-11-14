//! Browser extensions: unpacked Chromium (Manifest V3/V2) extensions loaded by WebView2.
//!
//! * `<config>/extensions/<id>/manifest.json`           - enabled, loaded at startup
//! * `<config>/extensions-disabled/<id>/manifest.json`  - switched off in Settings
//! * `<config>/extensions-invalid/`                      - stray items moved out of the way
//!
//! WebView2 loads *every* entry of the folder it is given and aborts webview creation when one
//! is not an extension, so `prepare` quarantines anything without a manifest first. Changes made
//! in Settings apply at the next start (the set of extensions is fixed per run).
//! Developer guide: docs/EXTENSIONS.md.
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExtensionInfo {
    /// Folder name; also the identifier used by Settings.
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
}

pub fn enabled_dir() -> PathBuf {
    crate::storage::config_dir().join("extensions")
}

fn disabled_dir() -> PathBuf {
    crate::storage::config_dir().join("extensions-disabled")
}

fn invalid_dir() -> PathBuf {
    crate::storage::config_dir().join("extensions-invalid")
}

/// Folder names come from the settings page: only plain names are acceptable.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id != "."
        && id != ".."
        && !id.contains(['/', '\\', ':', '\0'])
}

fn read_manifest(dir: &Path) -> Option<serde_json::Value> {
    let path = dir.join("manifest.json");
    if fs::metadata(&path).ok()?.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    // Chrome allows a UTF-8 BOM in manifests; serde_json does not.
    let text = fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    matches!(v.get("manifest_version").and_then(|m| m.as_u64()), Some(2 | 3)).then_some(v)
}

/// `__MSG_name__` placeholders live in _locales; showing the folder name is good enough.
fn manifest_text(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|s| s.as_str())
        .filter(|s| !s.starts_with("__MSG_"))
        .map(|s| s.chars().take(200).collect())
        .unwrap_or_default()
}

fn info_for(dir: &Path, enabled: bool) -> Option<ExtensionInfo> {
    let id = dir.file_name()?.to_str()?.to_string();
    let manifest = read_manifest(dir)?;
    let name = manifest_text(&manifest, "name");
    Some(ExtensionInfo {
        name: if name.is_empty() { id.clone() } else { name },
        version: manifest_text(&manifest, "version"),
        description: manifest_text(&manifest, "description"),
        id,
        enabled,
    })
}

fn scan(dir: &Path, enabled: bool) -> Vec<ExtensionInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut list: Vec<_> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| info_for(&e.path(), enabled))
        .collect();
    list.sort_by_key(|e| e.name.to_lowercase());
    list
}

/// Everything installed, enabled ones first.
pub fn list() -> Vec<ExtensionInfo> {
    let mut all = scan(&enabled_dir(), true);
    all.extend(scan(&disabled_dir(), false));
    all
}

/// Called once at startup. Returns the folder to hand to WebView2, or `None` when there is
/// nothing to load (extension support then stays off: no cost for users without extensions).
pub fn prepare() -> Option<PathBuf> {
    let dir = enabled_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::create_dir_all(disabled_dir());
    let mut loadable = 0;
    for entry in fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && read_manifest(&path).is_some() {
            loadable += 1;
        } else {
            quarantine(&path);
        }
    }
    (loadable > 0).then_some(dir)
}

fn quarantine(path: &Path) {
    let Some(name) = path.file_name() else { return };
    let target_dir = invalid_dir();
    let _ = fs::create_dir_all(&target_dir);
    let mut target = target_dir.join(name);
    let mut n = 1;
    while target.exists() {
        target = target_dir.join(format!("{}-{n}", name.to_string_lossy()));
        n += 1;
    }
    let _ = fs::rename(path, target);
}

pub fn set_enabled(id: &str, enabled: bool) -> bool {
    if !valid_id(id) {
        return false;
    }
    let (from, to) = if enabled {
        (disabled_dir(), enabled_dir())
    } else {
        (enabled_dir(), disabled_dir())
    };
    let src = from.join(id);
    let dst = to.join(id);
    if !src.is_dir() || dst.exists() {
        return false;
    }
    let _ = fs::create_dir_all(&to);
    fs::rename(src, dst).is_ok()
}

pub fn remove(id: &str) -> bool {
    if !valid_id(id) {
        return false;
    }
    let mut removed = false;
    for dir in [enabled_dir(), disabled_dir()] {
        let path = dir.join(id);
        if path.is_dir() {
            removed |= fs::remove_dir_all(path).is_ok();
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("viima-ext-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn test_ids_cannot_escape_the_folder() {
        assert!(valid_id("my-ext"));
        for bad in ["", ".", "..", "../x", "a/b", r"a\b", "C:x", "a\0b"] {
            assert!(!valid_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn test_manifest_reading() {
        let d = temp("manifest");
        assert!(info_for(&d, true).is_none(), "no manifest");
        fs::write(d.join("manifest.json"), "\u{feff}{\"manifest_version\":3,\"name\":\"__MSG_n__\",\"version\":\"1.2\"}").unwrap();
        let info = info_for(&d, true).unwrap();
        assert_eq!(info.version, "1.2");
        assert_eq!(info.name, info.id, "message placeholders fall back to the folder name");
        fs::write(d.join("manifest.json"), "{\"manifest_version\":1}").unwrap();
        assert!(info_for(&d, true).is_none(), "unsupported manifest version");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn test_scan_skips_files_and_broken_folders() {
        let d = temp("scan");
        fs::create_dir_all(d.join("good")).unwrap();
        fs::write(d.join("good").join("manifest.json"), r#"{"manifest_version":3,"name":"Good","version":"1"}"#).unwrap();
        fs::create_dir_all(d.join("broken")).unwrap();
        fs::write(d.join("note.txt"), "x").unwrap();
        let found = scan(&d, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Good");
        let _ = fs::remove_dir_all(&d);
    }
}
