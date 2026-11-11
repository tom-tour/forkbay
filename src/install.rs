// First start: lets the user keep the program where it is, or move it somewhere better.
use crate::core::*;
use crate::files::*;
use crate::sys::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const EXE_NAME: &str = if cfg!(windows) { "NX2 Launcher.exe" } else { "nx2-launcher" };

fn test_mode() -> bool {
    std::env::var("FORKBAY_INSTALLTEST").is_ok()
}

/// Only a brand new copy asks: anything already set up next to it stays where it is.
pub fn needed() -> bool {
    if std::env::var("FORKBAY_ROOT").is_ok() {
        return false;
    }
    let s = load_settings();
    if s.setup_done || s.install_asked {
        return false;
    }
    let Ok(rd) = fs::read_dir(root()) else { return true };
    !rd.flatten().any(|e| e.file_name() != "Manager")
}

fn recommended() -> PathBuf {
    if cfg!(windows) { local_data_base().join("Programs").join("NX2 Launcher") } else { local_data_base().join("nx2-launcher") }
}

/// Folders people download to or clean up often.
fn looks_temporary(dir: &Path) -> bool {
    let p = slash(dir).to_lowercase();
    let words = ["/downloads", "/téléchargements", "/telechargements", "/desktop", "/bureau", "/appdata/local/temp", "/tmp"];
    words.iter().any(|w| p.contains(w))
}

pub fn options() -> Value {
    let here = exe_dir();
    json!({ "here": here.to_string_lossy(), "recommended": recommended().to_string_lossy(), "temporary": looks_temporary(&here) })
}

pub fn stay() -> Result<()> {
    let mut s = load_settings();
    s.install_asked = true;
    save_settings(&s)
}

/// A chosen folder gets its own NX2 Launcher subfolder, because Files is created next to the program.
fn target_dir(kind: &str, custom: &str) -> Result<PathBuf> {
    if kind == "recommended" {
        return Ok(recommended());
    }
    if custom.trim().is_empty() {
        bail!("Choisis un dossier.");
    }
    let p = PathBuf::from(custom.trim());
    if p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("nx2 launcher") || n.to_string_lossy().eq_ignore_ascii_case("forkbay")) { Ok(p) } else { Ok(p.join("NX2 Launcher")) }
}

/// Copies the program, prepares its new place and returns the new program's path.
pub fn install(kind: &str, custom: &str, start_menu: bool, desktop: bool) -> Result<PathBuf> {
    let dir = target_dir(kind, custom)?;
    let current = std::env::current_exe()?;
    if current.parent().and_then(|p| fs::canonicalize(p).ok()) == fs::canonicalize(&dir).ok() {
        stay()?;
        return Ok(current);
    }
    let cannot = || anyhow!("Impossible d'écrire dans {} : choisis un autre dossier.", dir.display());
    fs::create_dir_all(&dir).map_err(|_| cannot())?;
    let target = dir.join(EXE_NAME);
    let tmp = dir.join(format!("{EXE_NAME}.tmp"));
    fs::copy(&current, &tmp).map_err(|_| cannot())?;
    if fs::rename(&tmp, &target).is_err() {
        let _ = fs::remove_file(&tmp);
        bail!("Un autre NX2 Launcher est déjà ouvert dans {} : ferme-le, puis réessaie.", dir.display());
    }
    // The new copy must not ask again.
    let manager = dir.join("Files").join("Manager");
    let settings = manager.join("settings.json");
    if !settings.is_file() {
        fs::create_dir_all(&manager)?;
        let mut s = load_settings();
        s.install_asked = true;
        fs::write(&settings, serde_json::to_vec_pretty(&s)?)?;
    }
    for (wanted, place) in [(start_menu, shortcut_dir(false)), (desktop, shortcut_dir(true))] {
        if !wanted { continue; }
        // The program is in place: a missing shortcut must not undo that.
        if let Err(e) = place.and_then(|d| write_shortcut(&d, &target)) {
            crate::log::error(&format!("shortcut: {e}"));
        }
    }
    Ok(target)
}

/// Started by the new copy: removes the old program and the empty Files it left behind.
pub fn cleanup_old(old: &Path) {
    let me = std::env::current_exe().unwrap_or_default();
    if old == me || !old.is_file() { return; }
    // Never touches a file that is not this same program.
    let same = matches!((sha256_file(old), sha256_file(&me)), (Ok(a), Ok(b)) if a == b);
    if !same { crate::log::error("install cleanup: the old file is a different program"); return; }
    for _ in 0..40 {
        if fs::remove_file(old).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let Some(files) = old.parent().map(|p| p.join("Files")) else { return };
    let only_manager = fs::read_dir(&files).is_ok_and(|rd| rd.flatten().all(|e| e.file_name() == "Manager"));
    let untouched = fs::read_to_string(files.join("Manager").join("settings.json")).ok().and_then(|t| serde_json::from_str::<Settings>(t.trim_start_matches('\u{feff}')).ok()).is_none_or(|s| !s.setup_done);
    if only_manager && untouched {
        for _ in 0..40 {
            if fs::remove_dir_all(&files).is_ok() { break; }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
}

fn shortcut_dir(desktop: bool) -> Result<PathBuf> {
    if let Ok(d) = std::env::var("FORKBAY_SHORTCUT_DIR") {
        return Ok(PathBuf::from(d).join(if desktop { "desktop" } else { "menu" }));
    }
    crate::sys::shortcut_folder(desktop)
}

/// Opens the new copy, which removes the old one once this process has gone.
pub fn restart_at(target: &Path) -> Result<()> {
    if test_mode() {
        return Ok(());
    }
    let mut c = std::process::Command::new(target);
    c.arg("--cleanup-install").arg(std::env::current_exe()?);
    if let Some(d) = target.parent() { c.current_dir(d); }
    c.spawn()?;
    std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_millis(600)); std::process::exit(0); });
    Ok(())
}
