#![windows_subsystem = "windows"]

#[cfg(windows)]
#[path = "../win_platform.rs"]
mod win_platform;

include!(concat!(env!("OUT_DIR"), "/payload_path.rs"));

#[cfg(windows)]
mod installer {
    use super::{PAYLOAD, win_platform};
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Command, exit};
    use std::thread;
    use std::time::Duration;
    use windows::Win32::UI::WindowsAndMessaging::{
        IDCANCEL, IDRETRY, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION,
        MB_ICONWARNING, MB_OK, MB_RETRYCANCEL, MB_YESNO, MB_YESNOCANCEL,
    };

    const APP_NAME: &str = "Viima";
    const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
    const EXE_NAME: &str = "OmaSelain.exe";
    const WEBVIEW2_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

    struct SetupOptions {
        silent: bool,
        no_desktop: bool,
        launch_after: bool,
    }

    fn parse_args() -> SetupOptions {
        let mut opts = SetupOptions {
            silent: false,
            no_desktop: false,
            launch_after: false,
        };
        for arg in std::env::args().skip(1) {
            match arg.to_ascii_lowercase().as_str() {
                "--silent" | "/s" | "-s" => opts.silent = true,
                "--no-desktop" => opts.no_desktop = true,
                "--launch" => opts.launch_after = true,
                _ => {}
            }
        }
        opts
    }

    fn write_payload_atomic(target_exe: &Path, silent: bool) -> Result<(), String> {
        let parent = target_exe
            .parent()
            .ok_or_else(|| "Virheellinen asennuspolku.".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|e| format!("Asennuskansion luonti epäonnistui: {e}"))?;

        let tmp_path = parent.join("OmaSelain.exe.tmp");
        {
            let mut f = File::create(&tmp_path)
                .map_err(|e| format!("Väliaikaistiedoston luonti epäonnistui: {e}"))?;
            f.write_all(PAYLOAD)
                .map_err(|e| format!("Ohjelmatiedoston kirjoitus epäonnistui: {e}"))?;
            f.sync_all()
                .map_err(|e| format!("Tiedoston synkronointi epäonnistui: {e}"))?;
        }

        let mut attempts = 0u32;
        loop {
            match fs::rename(&tmp_path, target_exe) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    attempts += 1;
                    if silent {
                        if attempts >= 8 {
                            let _ = fs::remove_file(&tmp_path);
                            return Err(format!(
                                "Tiedoston {} korvaus epäonnistui (onko selain käynnissä?): {err}",
                                target_exe.display()
                            ));
                        }
                        thread::sleep(Duration::from_millis(400));
                    } else {
                        let choice = win_platform::show_message_box(
                            &format!("{APP_NAME} — Asennus"),
                            &format!(
                                "Tiedostoa {} ei voitu korvata, koska {APP_NAME} on todennäköisesti käynnissä.\n\nSulje kaikki {APP_NAME} -ikkunat ja napsauta Yritä uudelleen.\n\nLisätieto: {err}",
                                EXE_NAME
                            ),
                            MB_RETRYCANCEL | MB_ICONWARNING,
                        );
                        if choice != IDRETRY {
                            let _ = fs::remove_file(&tmp_path);
                            return Err("Asennus keskeytettiin käyttäjän toimesta.".into());
                        }
                    }
                }
            }
        }
    }

    fn perform_install(
        install_dir: &Path,
        create_desktop_shortcut: bool,
        silent: bool,
    ) -> Result<PathBuf, String> {
        let target_exe = install_dir.join(EXE_NAME);
        write_payload_atomic(&target_exe, silent)?;

        if let Some(start_shortcut) = win_platform::start_menu_shortcut_path() {
            win_platform::create_shortcut(
                &start_shortcut,
                &target_exe,
                "Viima — Nopea ja kevyt verkkoselain",
            )?;
        }

        if create_desktop_shortcut
            && let Some(desktop_shortcut) = win_platform::desktop_shortcut_path()
        {
            win_platform::create_shortcut(
                &desktop_shortcut,
                &target_exe,
                "Viima — Nopea ja kevyt verkkoselain",
            )?;
        }

        if let Some(legacy_start) = win_platform::legacy_start_menu_shortcut_path() {
            let _ = fs::remove_file(legacy_start);
        }
        if let Some(legacy_desk) = win_platform::legacy_desktop_shortcut_path() {
            let _ = fs::remove_file(legacy_desk);
        }

        let estimated_kb = (PAYLOAD.len() / 1024).max(1) as u32;
        win_platform::register_uninstall_entry(install_dir, &target_exe, APP_VERSION, estimated_kb)?;

        Ok(target_exe)
    }

    pub fn run() {
        let opts = parse_args();
        let title = format!("{APP_NAME} v{APP_VERSION} — Asennus");

        if PAYLOAD.is_empty() {
            if !opts.silent {
                win_platform::show_message_box(
                    &title,
                    "Asennuspaketti ei sisällä selaintiedostoa (OMA_SELAIN_PAYLOAD puuttui käännöksessä).\n\nKäytä julkaisupakettia tai skriptiä scripts/build_release.ps1.",
                    MB_OK | MB_ICONERROR,
                );
            }
            exit(1);
        }

        let Some(install_dir) = win_platform::default_install_dir() else {
            if !opts.silent {
                win_platform::show_message_box(
                    &title,
                    "Paikallisen sovelluskansion (%LOCALAPPDATA%) sijaintia ei voitu selvittää.",
                    MB_OK | MB_ICONERROR,
                );
            }
            exit(1);
        };

        if !opts.silent {
            if !win_platform::is_webview2_installed() {
                let wv = win_platform::show_message_box(
                    &title,
                    "Järjestelmästä ei löytynyt Microsoft Edge WebView2 Runtime -komponenttia, jota Oma Selain tarvitsee verkkosivujen näyttämiseen.\n\nHaluatko avata Microsoftin lataussivun nyt?\n(Kyllä = Avaa lataussivu, Ei = Jatka asennusta silti, Peruuta = Lopeta)",
                    MB_YESNOCANCEL | MB_ICONWARNING,
                );
                if wv == IDYES {
                    win_platform::shell_open_url(WEBVIEW2_URL);
                } else if wv == IDCANCEL {
                    return;
                }
            }

            let existing = install_dir.join(EXE_NAME).exists();
            let action_verb = if existing { "päivittää" } else { "asentaa" };
            let confirm = win_platform::show_message_box(
                &title,
                &format!(
                    "Haluatko {action_verb} {APP_NAME} v{APP_VERSION} -selaimen?\n\nAsennuskansio:\n{}\n\nAsennus luo Käynnistä-valikon pikakuvakkeen eikä vaadi järjestelmänvalvojan oikeuksia.",
                    install_dir.display()
                ),
                MB_YESNO | MB_ICONINFORMATION,
            );
            if confirm != IDYES {
                return;
            }
        }

        let create_desktop = if opts.silent {
            !opts.no_desktop
        } else if opts.no_desktop {
            false
        } else {
            let desktop_ans = win_platform::show_message_box(
                &title,
                "Luodaanko pikakuvake myös työpöydälle?",
                MB_YESNO | MB_ICONQUESTION,
            );
            desktop_ans == IDYES
        };

        match perform_install(&install_dir, create_desktop, opts.silent) {
            Ok(target_exe) => {
                if opts.silent {
                    if opts.launch_after {
                        let _ = Command::new(&target_exe).spawn();
                    }
                } else {
                    let launch = win_platform::show_message_box(
                        &title,
                        &format!(
                            "{APP_NAME} v{APP_VERSION} asennettiin onnistuneesti!\n\nHaluatko käynnistää selaimen nyt?"
                        ),
                        MB_YESNO | MB_ICONINFORMATION,
                    );
                    if launch == IDYES || opts.launch_after {
                        let _ = Command::new(&target_exe).spawn();
                    }
                }
            }
            Err(err) => {
                if !opts.silent && !err.contains("keskeytettiin") {
                    win_platform::show_message_box(
                        &title,
                        &format!("Asennus epäonnistui:\n\n{err}"),
                        MB_OK | MB_ICONERROR,
                    );
                }
                exit(1);
            }
        }
    }
}

fn main() {
    #[cfg(windows)]
    installer::run();
}
