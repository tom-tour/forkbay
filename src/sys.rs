// Everything that works differently on Windows and Linux: processes, links, secrets,
// shortcuts, virus scan and system details. Other modules call these and stay the same on both.
#[cfg(not(windows))]
use crate::core::*;
#[cfg(windows)]
use anyhow::bail;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

// --- Commands without a console window ---
pub fn command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

pub fn running_processes() -> Vec<String> {
    let out = if cfg!(windows) { command("tasklist").args(["/FO", "CSV", "/NH"]).output() } else { command("ps").args(["-eo", "comm="]).output() };
    let Ok(out) = out else { return vec![] };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().trim_start_matches('"').split('"').next().map(|n| n.trim().trim_end_matches(".exe").to_lowercase()))
        .collect()
}

// --- Links and paths ---
#[cfg(windows)]
pub fn make_link(link: &Path, target: &Path) -> Result<()> {
    let ok = command("cmd").arg("/c").arg("mklink").arg("/J").arg(link).arg(target).output()?.status.success();
    if !ok {
        bail!("Impossible de créer le lien : {}", link.display());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn make_link(link: &Path, target: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

/// Removes the link only, never its target.
pub fn remove_link(link: &Path) -> Result<()> {
    if cfg!(windows) { fs::remove_dir(link)? } else { fs::remove_file(link)? };
    Ok(())
}

pub fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

// --- Secrets: encrypted for the Windows account; owner-only settings file on Linux ---
#[cfg(windows)]
pub fn protect(text: &str) -> String {
    use base64::Engine;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    if text.is_empty() {
        return String::new();
    }
    let mut bytes = text.as_bytes().to_vec();
    let input = CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_mut_ptr() };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    unsafe {
        if CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut output) == 0 {
            return String::new();
        }
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        format!("dpapi:{}", base64::engine::general_purpose::STANDARD.encode(out))
    }
}

#[cfg(windows)]
pub fn unprotect(text: &str) -> String {
    use base64::Engine;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let Some(b64) = text.strip_prefix("dpapi:") else { return String::new() };
    let Ok(mut bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else { return String::new() };
    let input = CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_mut_ptr() };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    unsafe {
        if CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0, &mut output) == 0 {
            return String::new();
        }
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        String::from_utf8(out).unwrap_or_default()
    }
}

#[cfg(not(windows))]
pub fn protect(text: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(manager_dir().join("settings.json"), fs::Permissions::from_mode(0o600));
    if text.is_empty() { String::new() } else { format!("plain:{text}") }
}

#[cfg(not(windows))]
pub fn unprotect(text: &str) -> String {
    text.strip_prefix("plain:").unwrap_or("").to_string()
}

/// Opens an address or a file with the system's default program.
pub fn open_external(target: &str) {
    // Never through cmd: it would run whatever follows an & in an address.
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let (verb, file) = (wide("open"), wide(target));
        let code = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1) } as isize;
        if code <= 32 {
            crate::log::error(&format!("could not open a link or file: code {code}"));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = command("xdg-open").arg(target).spawn();
    }
}

// --- Processes ---
// A number left behind by a crash can belong to another program by now.
pub fn is_process(pid: u64, exe: &str) -> bool {
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        let Ok(out) = command("tasklist").args(["/FI", filter.as_str(), "/FO", "CSV", "/NH"]).output() else { return false };
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.lines().filter_map(|l| l.strip_prefix('"')).filter_map(|l| l.split('"').next()).any(|n| n.eq_ignore_ascii_case(exe))
    }
    #[cfg(not(windows))]
    {
        // An AppImage runs under another name, so only the number can be checked.
        let _ = exe;
        Path::new(&format!("/proc/{pid}")).exists()
    }
}

/// Scans a new emulator with Windows Defender before it is used. Nothing to do on Linux.
pub fn virus_scan(dir: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let defender = PathBuf::from(std::env::var_os("ProgramFiles").unwrap_or_default()).join("Windows Defender").join("MpCmdRun.exe");
        if defender.is_file() {
            let st = command(&defender.to_string_lossy()).args(["-Scan", "-ScanType", "3", "-DisableRemediation", "-File"]).arg(dir).output()?;
            if st.status.code() == Some(2) {
                bail!("Defender signale une menace : installation annulée.");
            }
        }
    }
    #[cfg(not(windows))]
    let _ = dir;
    Ok(())
}

// --- Shortcuts ---
/// The Start menu (or applications menu) folder, or the desktop.
pub fn shortcut_folder(desktop: bool) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        if desktop { return known_desktop(); }
        let appdata = std::env::var_os("APPDATA").ok_or_else(|| anyhow::anyhow!("APPDATA"))?;
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
pub fn write_shortcut(dir: &Path, target: &Path) -> Result<()> {
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
pub fn write_shortcut(dir: &Path, target: &Path) -> Result<()> {
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

// --- System details for bug reports: name, processor, graphics cards, memory ---
#[cfg(not(windows))]
fn first_line(mut c: std::process::Command) -> String {
    c.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("; ")).unwrap_or_default()
}

#[cfg(windows)]
pub fn system_info() -> (String, String, String, String) {
    (windows_name(), windows_cpu(), gpu_name(), windows_memory())
}

#[cfg(windows)]
fn registry_text(key: &winreg::RegKey, name: &str) -> String {
    key.get_value::<String, _>(name).unwrap_or_default().trim().to_string()
}

#[cfg(windows)]
fn windows_name() -> String {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").map(|k| format!("{} (build {})", registry_text(&k, "ProductName"), registry_text(&k, "CurrentBuild"))).unwrap_or_default()
}

#[cfg(windows)]
fn windows_cpu() -> String {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    hklm.open_subkey(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0").map(|k| registry_text(&k, "ProcessorNameString")).unwrap_or_default()
}

#[cfg(windows)]
fn windows_memory() -> String {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 { return String::new(); }
    format!("{} GB", (m.ullTotalPhys as f64 / 1_073_741_824.0).round() as u64)
}

/// Graphics cards from the registry, which needs no helper program.
#[cfg(windows)]
pub fn gpu_name() -> String {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let Ok(class) = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}") else { return String::new() };
    class.enum_keys().flatten().filter_map(|n| class.open_subkey(n).ok()).filter_map(|k| {
        let name = registry_text(&k, "DriverDesc");
        (!name.is_empty()).then(|| format!("{name} (driver {})", registry_text(&k, "DriverVersion")))
    }).collect::<Vec<_>>().join("; ")
}

#[cfg(not(windows))]
pub fn system_info() -> (String, String, String, String) {
    let os = fs::read_to_string("/etc/os-release").ok().and_then(|t| t.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| l[12..].trim_matches('"').to_string())).unwrap_or_else(|| "Linux".into());
    let cpu = fs::read_to_string("/proc/cpuinfo").ok().and_then(|t| t.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split(':').nth(1)).map(|s| s.trim().to_string())).unwrap_or_default();
    let mut c = command("sh");
    c.args(["-c", "lspci 2>/dev/null | grep -Ei 'vga|3d|display' | cut -d: -f3-"]);
    let gpu = first_line(c);
    let ram = fs::read_to_string("/proc/meminfo").ok().and_then(|t| t.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|k| k.parse::<u64>().ok())).map(|k| format!("{} GB", k / 1_048_576)).unwrap_or_default();
    (os, cpu, gpu, ram)
}

#[cfg(not(windows))]
pub fn gpu_name() -> String {
    system_info().2
}

// --- Time zone and language ---
/// Seconds to add to UTC for the local time.
#[cfg(windows)]
pub fn local_offset() -> i64 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut z: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    let bias = match unsafe { GetTimeZoneInformation(&mut z) } {
        1 => z.Bias + z.StandardBias,
        2 => z.Bias + z.DaylightBias,
        _ => z.Bias,
    };
    -(bias as i64) * 60
}

#[cfg(not(windows))]
pub fn local_offset() -> i64 {
    let out = command("date").arg("+%z").output();
    out.ok().and_then(|o| {
        let t = String::from_utf8_lossy(&o.stdout).trim().to_string();
        let sign = if t.starts_with('-') { -1 } else { 1 };
        let h: i64 = t.get(1..3)?.parse().ok()?;
        let m: i64 = t.get(3..5)?.parse().ok()?;
        Some(sign * (h * 3600 + m * 60))
    }).unwrap_or(0)
}

/// The system language, lowercase, such as fr-fr.
pub fn system_locale() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85];
        let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if n > 1 {
            return String::from_utf16_lossy(&buf[..n as usize - 1]).to_lowercase();
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty())).unwrap_or_default().to_lowercase()
    }
}
