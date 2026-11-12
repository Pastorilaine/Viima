use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn default_update_url() -> &'static str {
    option_env!("OMA_SELAIN_UPDATE_URL").unwrap_or(
        "https://github.com/Pastorilaine/Viima/releases/latest/download/latest.json",
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub release_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiUpdateStatus {
    pub state: &'static str,
    pub current_version: &'static str,
    pub latest_version: Option<String>,
    pub release_notes: Option<String>,
    pub error: Option<String>,
    pub can_self_update: bool,
}

impl Default for UiUpdateStatus {
    fn default() -> Self {
        Self {
            state: "idle",
            current_version: CURRENT_VERSION,
            latest_version: None,
            release_notes: None,
            error: None,
            can_self_update: can_self_update(),
        }
    }
}

pub fn can_self_update() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let lower = exe.to_string_lossy().to_ascii_lowercase();
    !lower.contains(r"\target\debug\") && !lower.contains(r"\target\release\")
}

fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let trimmed = v.trim().trim_start_matches(['v', 'V']);
    let core = trimmed.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

pub fn parse_manifest(body: &str) -> Result<UpdateInfo, String> {
    if let Ok(info) = serde_json::from_str::<UpdateInfo>(body)
        && !info.version.trim().is_empty()
        && !info.download_url.trim().is_empty()
    {
        if !info.download_url.starts_with("https://") {
            return Err("Päivityksen latausosoitteen täytyy käyttää HTTPS-yhteyttä.".into());
        }
        return Ok(info);
    }

    if let Ok(release) = serde_json::from_str::<GithubRelease>(body) {
        let asset = release
            .assets
            .iter()
            .find(|a| {
                let lower = a.name.to_ascii_lowercase();
                lower.ends_with(".exe") && !lower.contains("setup")
            })
            .ok_or_else(|| "GitHub-julkaisusta ei löytynyt OmaSelain.exe-tiedostoa.".to_string())?;

        if !asset.browser_download_url.starts_with("https://") {
            return Err("Päivitystiedoston osoite ei ole HTTPS.".into());
        }

        let sha256 = asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(|s| s.to_string());

        return Ok(UpdateInfo {
            version: release.tag_name,
            download_url: asset.browser_download_url.clone(),
            sha256,
            release_notes: release.body,
        });
    }

    Err("Tuntematon päivitystiedoston muoto.".into())
}

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into()
}

pub fn check_for_update(update_url: &str) -> Result<Option<UpdateInfo>, String> {
    let trimmed = update_url.trim();
    if !trimmed.starts_with("https://") {
        return Err("Päivitys-URL:n täytyy alkaa https://".into());
    }

    let agent = http_agent();
    let user_agent = format!("OmaSelain/{CURRENT_VERSION}");
    let mut resp = agent
        .get(trimmed)
        .header("User-Agent", &user_agent)
        .header("Accept", "application/vnd.github+json, application/json")
        .call()
        .map_err(|e| format!("Päivitysten tarkistus epäonnistui: {e}"))?;

    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Päivitysvastauksen luku epäonnistui: {e}"))?;

    let info = parse_manifest(&body)?;
    if is_newer_version(&info.version, CURRENT_VERSION) {
        Ok(Some(info))
    } else {
        Ok(None)
    }
}

fn updates_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(dir) = crate::win_platform::default_install_dir() {
        return dir.join("updates");
    }
    std::env::temp_dir().join("oma_selain_updates")
}

pub fn download_and_stage_update(info: &UpdateInfo) -> Result<PathBuf, String> {
    if !info.download_url.starts_with("https://") {
        return Err("Vain HTTPS-lataukset on sallittu.".into());
    }

    let dir = updates_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Päivityskansion luonti epäonnistui: {e}"))?;

    let part_path = dir.join("OmaSelain-staged.exe.part");
    let staged_path = dir.join("OmaSelain-staged.exe");
    let _ = fs::remove_file(&part_path);

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build();
    let agent: ureq::Agent = agent.into();
    let user_agent = format!("OmaSelain/{CURRENT_VERSION}");

    let mut resp = agent
        .get(&info.download_url)
        .header("User-Agent", &user_agent)
        .call()
        .map_err(|e| format!("Päivityksen lataus epäonnistui: {e}"))?;

    let mut reader = resp.body_mut().as_reader();
    let mut file =
        File::create(&part_path).map_err(|e| format!("Päivitystiedoston luonti epäonnistui: {e}"))?;

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 32 * 1024];
    let mut total: usize = 0;
    let mut mz_header = [0u8; 2];
    const MAX_BYTES: usize = 100 * 1024 * 1024;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Latausvirhe: {e}"))?;
        if n == 0 {
            break;
        }
        if total == 0 && n >= 2 {
            mz_header.copy_from_slice(&buf[..2]);
        } else if total == 0 && n == 1 {
            mz_header[0] = buf[0];
        } else if total == 1 {
            mz_header[1] = buf[0];
        }
        total += n;
        if total > MAX_BYTES {
            drop(file);
            let _ = fs::remove_file(&part_path);
            return Err("Päivitystiedosto ylittää sallitun enimmäiskoon.".into());
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])
            .map_err(|e| format!("Levylle kirjoitus epäonnistui: {e}"))?;
    }
    file.flush()
        .map_err(|e| format!("Puskurin tyhjennys epäonnistui: {e}"))?;
    drop(file);

    if total < 64 || &mz_header != b"MZ" {
        let _ = fs::remove_file(&part_path);
        return Err("Ladattu tiedosto ei ole kelvollinen Windows-ohjelma.".into());
    }

    if let Some(expected) = info.sha256.as_deref() {
        let expected_norm = expected.trim().to_ascii_lowercase();
        if !expected_norm.is_empty() {
            let actual = format!("{:x}", hasher.finalize());
            if actual != expected_norm {
                let _ = fs::remove_file(&part_path);
                return Err("Päivityksen SHA-256-eheystarkistus epäonnistui.".into());
            }
        }
    }

    let _ = fs::remove_file(&staged_path);
    fs::rename(&part_path, &staged_path)
        .map_err(|e| format!("Päivitystiedoston viimeistely epäonnistui: {e}"))?;

    Ok(staged_path)
}

#[cfg(windows)]
pub fn launch_staged_update(staged_path: &Path) -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("Polun selvitys epäonnistui: {e}"))?;
    let pid = std::process::id().to_string();

    std::process::Command::new(staged_path)
        .arg("--apply-update")
        .arg(pid)
        .arg(&current_exe)
        .spawn()
        .map_err(|e| format!("Päivitysapurin käynnistys epäonnistui: {e}"))?;
    Ok(())
}

#[cfg(windows)]
pub fn handle_cli_args() -> bool {
    let args: Vec<String> = std::env::args().collect();

    if let Some(pos) = args.iter().position(|a| a == "--apply-update")
        && let (Some(pid_str), Some(target_str)) = (args.get(pos + 1), args.get(pos + 2))
    {
        if let Ok(pid) = pid_str.parse::<u32>() {
            crate::win_platform::wait_for_process_exit(pid, 15_000);
        }
        let target = PathBuf::from(target_str);
        let Ok(self_exe) = std::env::current_exe() else {
            return true;
        };
        let backup = target.with_extension("exe.old");
        let _ = fs::remove_file(&backup);

        // Retry briefly in case antivirus holds the newly-exited process handle.
        let mut moved = false;
        for _ in 0..10 {
            if !target.exists() || fs::rename(&target, &backup).is_ok() {
                moved = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        if moved && fs::copy(&self_exe, &target).is_ok() {
            let _ = fs::remove_file(&backup);
            let _ = crate::win_platform::register_uninstall_entry(
                target.parent().unwrap_or_else(|| Path::new(".")),
                &target,
                CURRENT_VERSION,
                (fs::metadata(&target).map(|m| m.len() / 1024).unwrap_or(4096)) as u32,
            );
            let _ = std::process::Command::new(&target)
                .arg("--cleanup-update")
                .arg(&self_exe)
                .spawn();
        } else {
            // Rollback if copy failed
            if backup.exists() && !target.exists() {
                let _ = fs::rename(&backup, &target);
            }
            crate::win_platform::show_message_box(
                "Viima — Päivitys epäonnistui",
                "Päivitystä ei voitu asentaa. Sulje kaikki selaimen ikkunat ja yritä uudelleen.",
                windows::Win32::UI::WindowsAndMessaging::MB_OK
                    | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
        return true;
    }

    if let Some(pos) = args.iter().position(|a| a == "--cleanup-update")
        && let Some(staged_str) = args.get(pos + 1)
    {
        let staged = PathBuf::from(staged_str);
        for _ in 0..5 {
            if !staged.exists() || fs::remove_file(&staged).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        if let Ok(exe) = std::env::current_exe() {
            let _ = fs::remove_file(exe.with_extension("exe.old"));
        }
        return false;
    }

    if args.iter().any(|a| a == "--uninstall") {
        let quiet = args.iter().any(|a| a == "--quiet");
        run_uninstall(quiet);
        return true;
    }

    false
}

#[cfg(not(windows))]
pub fn handle_cli_args() -> bool {
    false
}

#[cfg(windows)]
fn run_uninstall(quiet: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
    };

    if !quiet {
        let answer = crate::win_platform::show_message_box(
            "Poista Viima",
            "Haluatko varmasti poistaa Viima-selaimen tietokoneeltasi?",
            MB_YESNO | MB_ICONQUESTION,
        );
        if answer != IDYES {
            return;
        }
    }

    let delete_user_data = if quiet {
        false
    } else {
        crate::win_platform::show_message_box(
            "Poista selaustiedot?",
            "Poistetaanko myös tallennetut kirjanmerkit, selaushistoria, asetukset ja kirjautumiset?",
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    };

    if let Some(desktop) = crate::win_platform::desktop_shortcut_path() {
        let _ = fs::remove_file(desktop);
    }
    if let Some(start_menu) = crate::win_platform::start_menu_shortcut_path() {
        let _ = fs::remove_file(start_menu);
    }
    if let Some(legacy_desktop) = crate::win_platform::legacy_desktop_shortcut_path() {
        let _ = fs::remove_file(legacy_desktop);
    }
    if let Some(legacy_start) = crate::win_platform::legacy_start_menu_shortcut_path() {
        let _ = fs::remove_file(legacy_start);
    }
    crate::win_platform::remove_uninstall_entry();

    if delete_user_data
        && let Some(proj) = directories::ProjectDirs::from("fi", "IT-Veljekset", "OmaSelain")
    {
        let _ = fs::remove_dir_all(proj.config_dir());
        let _ = fs::remove_dir_all(proj.data_local_dir());
    }

    if let Ok(exe) = std::env::current_exe() {
        let updates = exe.parent().map(|p| p.join("updates"));
        if let Some(u) = updates {
            let _ = fs::remove_dir_all(u);
        }
        let exe_str = exe.to_string_lossy().replace('"', "");
        let dir_str = exe
            .parent()
            .map(|p| p.to_string_lossy().replace('"', ""))
            .unwrap_or_default();

        // Spawn detached cleanup command from %TEMP% so the running exe can be deleted after exit.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let raw_cmd = format!(
            "/S /C \"ping 127.0.0.1 -n 3 > nul & del /f /q \"{exe_str}\" & rmdir \"{dir_str}\" 2>nul\""
        );
        let _ = std::process::Command::new("cmd.exe")
            .raw_arg(raw_cmd)
            .current_dir(std::env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }

    if !quiet {
        crate::win_platform::show_message_box(
            "Viima poistettu",
            "Viima on poistettu tietokoneeltasi.",
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison() {
        assert!(is_newer_version("0.3.1", "0.3.0"));
        assert!(is_newer_version("v0.4.0", "0.3.9"));
        assert!(is_newer_version("1.0.0", "0.99.99"));
        assert!(!is_newer_version("0.3.0", "0.3.0"));
        assert!(!is_newer_version("v0.2.9", "0.3.0"));
    }

    #[test]
    fn test_parse_latest_json_manifest() {
        let json = r#"{
            "version": "0.3.1",
            "download_url": "https://github.com/Pastorilaine/oma-selain/releases/download/v0.3.1/OmaSelain.exe",
            "sha256": "deadbeef",
            "release_notes": "Bugikorjauksia"
        }"#;
        let info = parse_manifest(json).unwrap();
        assert_eq!(info.version, "0.3.1");
        assert_eq!(info.sha256.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn test_parse_github_release_api() {
        let json = r#"{
            "tag_name": "v0.3.1",
            "body": "Uusi julkaisu",
            "assets": [
                {
                    "name": "OmaSelain-Setup.exe",
                    "browser_download_url": "https://github.com/Pastorilaine/oma-selain/releases/download/v0.3.1/OmaSelain-Setup.exe"
                },
                {
                    "name": "OmaSelain.exe",
                    "browser_download_url": "https://github.com/Pastorilaine/oma-selain/releases/download/v0.3.1/OmaSelain.exe",
                    "digest": "sha256:abcdef1234"
                }
            ]
        }"#;
        let info = parse_manifest(json).unwrap();
        assert_eq!(info.version, "v0.3.1");
        assert!(info.download_url.ends_with("/OmaSelain.exe"));
        assert_eq!(info.sha256.as_deref(), Some("abcdef1234"));
    }

    #[test]
    fn test_rejects_insecure_http_download() {
        let json = r#"{
            "version": "0.3.1",
            "download_url": "http://evil.com/OmaSelain.exe"
        }"#;
        assert!(parse_manifest(json).is_err());
    }
}
