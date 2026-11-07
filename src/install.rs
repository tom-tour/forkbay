// First start: lets the user keep the program where it is, or move it somewhere better.
use crate::core::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const EXE_NAME: &str = if cfg!(windows) { "Forkbay.exe" } else { "forkbay" };

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
    if cfg!(windows) { local_data_base().join("Programs").join("Forkbay") } else { local_data_base().join("forkbay") }
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

/// A chosen folder gets its own Forkbay subfolder, because Files is created next to the program.
fn target_dir(kind: &str, custom: &str) -> Result<PathBuf> {
    if kind == "recommended" {
        return Ok(recommended());
    }
    if custom.trim().is_empty() {
        bail!("Choisis un dossier.");
    }
    let p = PathBuf::from(custom.trim());
    if p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("forkbay")) { Ok(p) } else { Ok(p.join("Forkbay")) }
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
        bail!("Un autre Forkbay est déjà ouvert dans {} : ferme-le, puis réessaie.", dir.display());
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
    #[cfg(windows)]
    {
        if desktop { return known_desktop(); }
        let appdata = std::env::var_os("APPDATA").ok_or_else(|| anyhow!("APPDATA"))?;
        Ok(PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs"))
    }
    #[cfg(not(windows))]
    {
        Ok(if desktop { home().join("Desktop") } else { local_data_base().join("applications") })
    }
}

#[cfg(windows)]
fn known_desktop() -> Result<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath};
    unsafe {
        let mut out = std::ptr::null_mut();
        if SHGetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), &mut out) != 0 {
            bail!("Bureau introuvable.");
        }
        let len = (0..).take_while(|&i| *out.add(i) != 0).count();
        let path = String::from_utf16_lossy(std::slice::from_raw_parts(out, len));
        CoTaskMemFree(out as *const _);
        Ok(PathBuf::from(path))
    }
}

#[cfg(not(windows))]
fn write_shortcut(dir: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(dir)?;
    let icon = target.with_file_name("forkbay.png");
    fs::write(&icon, include_bytes!("../assets/icon.png"))?;
    let text = format!("[Desktop Entry]\nType=Application\nName=Forkbay\nComment=Switch 2 emulator manager\nExec=\"{}\"\nPath={}\nIcon={}\nTerminal=false\nCategories=Game;\n", target.display(), target.parent().unwrap_or(target).display(), icon.display());
    let file = dir.join("forkbay.desktop");
    fs::write(&file, text)?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

// Windows shortcuts only exist through these two COM interfaces. windows-sys has their
// identifiers but not their method tables, so the methods used here are laid out by hand.
#[cfg(windows)]
fn write_shortcut(dir: &Path, target: &Path) -> Result<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::core::GUID;
    use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    type Set = unsafe extern "system" fn(*mut c_void, *const u16) -> i32;
    #[repr(C)]
    struct ShellLink {
        query: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_path: usize,
        get_id_list: usize,
        set_id_list: usize,
        get_description: usize,
        set_description: Set,
        get_working_dir: usize,
        set_working_dir: Set,
        get_arguments: usize,
        set_arguments: usize,
        get_hotkey: usize,
        set_hotkey: usize,
        get_show: usize,
        set_show: usize,
        get_icon: usize,
        set_icon: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
        set_relative: usize,
        resolve: usize,
        set_path: Set,
    }
    #[repr(C)]
    struct PersistFile {
        query: usize,
        add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        class_id: usize,
        is_dirty: usize,
        load: usize,
        save: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
    }
    const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);
    const IID_SHELL_LINK: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
    const IID_PERSIST_FILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);
    fn wide(p: &std::ffi::OsStr) -> Vec<u16> { p.encode_wide().chain([0]).collect() }

    fs::create_dir_all(dir)?;
    let file = wide(dir.join("Forkbay.lnk").as_os_str());
    let exe = wide(target.as_os_str());
    let work = wide(target.parent().unwrap_or(target).as_os_str());
    let about = wide(std::ffi::OsStr::new("Switch 2 emulator manager"));
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let mut link: *mut c_void = std::ptr::null_mut();
        if CoCreateInstance(&CLSID_SHELL_LINK, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, &IID_SHELL_LINK, &mut link) != 0 || link.is_null() {
            bail!("Raccourci impossible.");
        }
        let v = &**(link as *mut *const ShellLink);
        let mut ok = (v.set_path)(link, exe.as_ptr()) == 0 && (v.set_working_dir)(link, work.as_ptr()) == 0;
        (v.set_description)(link, about.as_ptr());
        (v.set_icon)(link, exe.as_ptr(), 0);
        let mut persist: *mut c_void = std::ptr::null_mut();
        if ok && (v.query)(link, &IID_PERSIST_FILE, &mut persist) == 0 && !persist.is_null() {
            let p = &**(persist as *mut *const PersistFile);
            ok = (p.save)(persist, file.as_ptr(), 1) == 0;
            (p.release)(persist);
        } else {
            ok = false;
        }
        (v.release)(link);
        if !ok { bail!("Raccourci impossible."); }
    }
    Ok(())
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
