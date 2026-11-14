use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_windows_rc() -> Option<PathBuf> {
    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            let cand = dir.join("rc.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let kits_bin = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = fs::read_dir(kits_bin)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("x64").join("rc.exe"))
        .filter(|p| p.is_file())
        .collect();
    versions.sort();
    versions.pop()
}

fn compile_windows_icon(out_dir: &Path) {
    let ico_rel = Path::new("assets/branding/viima.ico");
    println!("cargo:rerun-if-changed={}", ico_rel.display());
    if !ico_rel.is_file() {
        return;
    }

    let Some(rc_exe) = find_windows_rc() else {
        println!("cargo:warning=rc.exe ei löytynyt; Windows .exe -ikonia ei upotettu.");
        return;
    };

    let ico_abs = fs::canonicalize(ico_rel).unwrap();
    let ico_rc_path = ico_abs
        .to_string_lossy()
        .replace(r"\\?\", "")
        .replace('\\', "/");

    let rc_file = out_dir.join("viima_icon.rc");
    let res_file = out_dir.join("viima_icon.res");
    fs::write(&rc_file, format!("1 ICON \"{ico_rc_path}\"\n")).unwrap();

    let status = Command::new(&rc_exe)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_file)
        .arg(&rc_file)
        .status();

    if matches!(status, Ok(s) if s.success()) && res_file.is_file() {
        println!(
            "cargo:rustc-link-arg-bin=viima={}",
            res_file.display()
        );
        println!(
            "cargo:rustc-link-arg-bin=viima_setup={}",
            res_file.display()
        );
    } else {
        println!("cargo:warning=rc.exe epäonnistui kuvakkeen kääntämisessä.");
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=VIIMA_PAYLOAD");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("payload_path.rs");

    if env::var("CARGO_CFG_WINDOWS").is_ok() {
        compile_windows_icon(&out_dir);
    }

    if let Ok(payload) = env::var("VIIMA_PAYLOAD")
        && !payload.trim().is_empty()
    {
        let abs = fs::canonicalize(&payload).expect("VIIMA_PAYLOAD path not found");
        println!("cargo:rerun-if-changed={}", abs.display());
        let escaped = abs
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        fs::write(
            &dest,
            format!("pub const PAYLOAD: &[u8] = include_bytes!(\"{escaped}\");\n"),
        )
        .unwrap();
    } else {
        fs::write(&dest, "pub const PAYLOAD: &[u8] = &[];\n").unwrap();
    }
}
