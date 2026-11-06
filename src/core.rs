// Settings, paths and the shared data folder. Everything is relative to the executable.
use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const GAME_EXTENSIONS: [&str; 3] = ["nup", "nsp", "xci"];

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Sync {
    pub enabled: bool,
    pub provider: String,
    pub folder_path: String,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_refresh_token: String,
    pub keep_backups: u32,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub setup_done: bool,
    pub theme: String,
    pub language: String,
    pub auto_update_check: bool,
    pub terms_accepted: u32,
    pub custom_colors: BTreeMap<String, String>,
    pub mode: String,
    pub gradient: bool,
    pub custom_surfaces: bool,
    pub user_themes: Vec<serde_json::Value>,
    pub steam_grid_key: String,
    pub data_location: String,
    pub custom_data_path: String,
    pub default_emulator: String,
    pub games: BTreeMap<String, String>,
    pub sync: Sync,
    pub auto_backup: bool,
    pub backup_keep: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            setup_done: false,
            theme: "aurora".into(),
            language: "auto".into(),
            auto_update_check: true,
            terms_accepted: 0,
            custom_colors: BTreeMap::new(),
            mode: "dark".into(),
            gradient: true,
            custom_surfaces: false,
            user_themes: vec![],
            steam_grid_key: String::new(),
            data_location: "portable".into(),
            custom_data_path: String::new(),
            default_emulator: "Lichii".into(),
            games: BTreeMap::new(),
            sync: Sync { provider: "folder".into(), keep_backups: 10, ..Default::default() },
            auto_backup: true,
            backup_keep: 10,
        }
    }
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| ".".into())
}

/// The Files folder next to the executable (FORKBAY_ROOT overrides it for tests).
pub fn root() -> PathBuf {
    if let Ok(r) = std::env::var("FORKBAY_ROOT") {
        return r.into();
    }
    exe_dir().join("Files")
}

pub fn manager_dir() -> PathBuf {
    root().join("Manager")
}

pub fn load_settings() -> Settings {
    let path = manager_dir().join("settings.json");
    let mut s: Settings = match fs::read_to_string(&path) {
        // Text editors may add a byte order mark: it must not reset the settings.
        Ok(t) => serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or_else(|e| {
            // A damaged file is kept aside instead of being overwritten by the next save.
            let _ = fs::copy(&path, path.with_extension("json.damaged"));
            // Written directly: the normal journal reads the settings, which would loop here.
            crate::log::raw("ERROR", &format!("settings.json unreadable: {e}"));
            Settings::default()
        }),
        Err(_) => Settings::default(),
    };
    crate::theme::migrate(&mut s);
    s
}

pub fn save_settings(s: &Settings) -> Result<()> {
    fs::create_dir_all(manager_dir())?;
    let path = manager_dir().join("settings.json");
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn local_data_base() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| ".".into())
    } else {
        home().join(".local/share")
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| ".".into())
}

pub fn data_path_for(s: &Settings) -> PathBuf {
    match s.data_location.as_str() {
        "local" => local_data_base().join("Forkbay").join("Data"),
        "custom" if !s.custom_data_path.is_empty() => PathBuf::from(&s.custom_data_path),
        _ => root().join("Data"),
    }
}

pub fn data_path() -> PathBuf {
    data_path_for(&load_settings())
}

pub fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

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

// --- Files ---
pub fn copy_tree(from: &Path, to: &Path, overwrite: bool) -> Result<u64> {
    let mut count = 0;
    if !from.exists() {
        return Ok(0);
    }
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        let kind = e.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            count += copy_tree(&e.path(), &target, overwrite)?;
        } else if overwrite || !target.exists() {
            fs::copy(e.path(), &target).with_context(|| format!("Copie impossible : {}", e.path().display()))?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk_files(&e.path(), out);
        } else {
            out.push(e.path());
        }
    }
}

pub fn tree_stats(dir: &Path) -> (u64, u64) {
    let mut files = vec![];
    walk_files(dir, &mut files);
    let size = files.iter().filter_map(|f| f.metadata().ok()).map(|m| m.len()).sum();
    (files.len() as u64, size)
}

// --- Games ---
pub fn rom_files() -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(root().join("Roms"))
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| GAME_EXTENSIONS.contains(&x.to_lowercase().as_str())))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn display_name(file: &str) -> String {
    let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file);
    let clean = Regex::new(r"\s*[\[(][^\])]*[\])]").unwrap().replace_all(stem, "").trim().to_string();
    if clean.is_empty() { stem.to_string() } else { clean }
}

pub fn game_emulator(s: &Settings, file: &str) -> String {
    s.games.get(file).filter(|e| !e.is_empty()).cloned().unwrap_or_else(|| s.default_emulator.clone())
}

// --- Links and paths ---
#[cfg(windows)]
fn make_link(link: &Path, target: &Path) -> Result<()> {
    let ok = command("cmd").arg("/c").arg("mklink").arg("/J").arg(link).arg(target).output()?.status.success();
    if !ok {
        bail!("Impossible de créer le lien : {}", link.display());
    }
    Ok(())
}

#[cfg(not(windows))]
fn make_link(link: &Path, target: &Path) -> Result<()> {
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

pub fn set_emulator_link(folder: &Path, data: &Path) -> Result<()> {
    let link = folder.join("user");
    if let Ok(meta) = fs::symlink_metadata(&link) {
        if !is_link(&meta) {
            bail!("{} est un dossier normal alors qu'un lien est attendu : déplace son contenu dans Data, puis supprime-le.", link.display());
        }
        if fs::canonicalize(&link).ok() == fs::canonicalize(data).ok() && fs::canonicalize(data).is_ok() {
            return Ok(());
        }
        remove_link(&link)?;
    }
    make_link(&link, data)
}

pub fn set_config_paths(data: &Path) -> Result<()> {
    let ini = data.join("config").join("qt-config.ini");
    let Ok(old) = fs::read_to_string(&ini) else { return Ok(()) };
    let d = slash(data);
    let roms = slash(&root().join("Roms"));
    let new = Regex::new(r"(?m)^(nand|sdmc|load|dump|tas)_directory=[^\r\n]*").unwrap().replace_all(&old, |c: &regex::Captures| format!("{0}_directory={d}/{0}", &c[1]));
    let new = Regex::new(r"(?m)^(Screenshots\\screenshot_path)=[^\r\n]*").unwrap().replace_all(&new, |c: &regex::Captures| format!("{}={d}/screenshots", &c[1]));
    let new = Regex::new(r"(?m)^(Paths\\gamedirs\\\d+\\path)=([^\r\n]*)").unwrap().replace_all(&new, |c: &regex::Captures| {
        if c[2].contains(":/") || c[2].starts_with('/') || &c[2] == "__LOCAL__" { format!("{}={roms}", &c[1]) } else { c[0].to_string() }
    });
    if new != old {
        fs::write(&ini, new.as_bytes())?;
    }
    Ok(())
}

pub fn repair_setup() -> Result<PathBuf> {
    let data = data_path();
    for sub in ["config", "keys", "log", "load", "sdmc", "dump", "tas", "screenshots", "nand/user/save", "nand/system/Contents/registered"] {
        fs::create_dir_all(data.join(sub))?;
    }
    fs::create_dir_all(root().join("Roms"))?;
    for folder in crate::emu::emulator_folders() {
        set_emulator_link(&folder, &data)?;
    }
    set_config_paths(&data)?;
    Ok(data)
}

// --- Keys and firmware ---
pub fn has_keys(data: &Path) -> bool {
    data.join("keys").join("prod.keys").is_file()
}

pub fn firmware_count(data: &Path) -> usize {
    fs::read_dir(data.join("nand/system/Contents/registered"))
        .map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".nca")).count())
        .unwrap_or(0)
}

/// Copies prod.keys (and title.keys when it sits beside it). Refuses a file that is not a key file.
pub fn install_keys(path: &Path) -> Result<String> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if !name.ends_with(".keys") {
        bail!("Choisis ton fichier prod.keys.");
    }
    let text = fs::read_to_string(path).map_err(|_| anyhow!("Ce fichier n'est pas un fichier de clés lisible."))?;
    if !text.lines().any(|l| l.contains('=') && l.to_lowercase().contains("key")) {
        bail!("Ce fichier ne ressemble pas à un fichier de clés.");
    }
    let data = repair_setup()?;
    let dir = data.join("keys");
    fs::copy(path, dir.join("prod.keys"))?;
    let title = path.with_file_name("title.keys");
    let mut done = "prod.keys installé.";
    if title.is_file() {
        fs::copy(&title, dir.join("title.keys"))?;
        done = "prod.keys et title.keys installés.";
    }
    Ok(done.to_string())
}

/// Installs firmware from a zip or a folder of .nca files. Refuses anything else.
pub fn install_firmware(path: &Path) -> Result<String> {
    let data = repair_setup()?;
    let reg = data.join("nand/system/Contents/registered");
    let mut count = 0;
    if path.is_dir() {
        let mut files = vec![];
        walk_files(path, &mut files);
        for f in files.iter().filter(|f| f.to_string_lossy().ends_with(".nca")) {
            fs::copy(f, reg.join(f.file_name().unwrap()))?;
            count += 1;
        }
    } else {
        let mut zip = zip::ZipArchive::new(fs::File::open(path)?).map_err(|_| anyhow!("Ce fichier n'est pas une archive zip de firmware."))?;
        for i in 0..zip.len() {
            let mut e = zip.by_index(i)?;
            let Some(name) = e.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) else { continue };
            if !name.to_string_lossy().ends_with(".nca") {
                continue;
            }
            let mut out = fs::File::create(reg.join(name))?;
            std::io::copy(&mut e, &mut out)?;
            count += 1;
        }
    }
    if count == 0 {
        bail!("Aucun fichier .nca trouvé : ce n'est pas un firmware.");
    }
    Ok(format!("Firmware installé : {count} fichiers."))
}

/// Looks for keys and firmware already placed in Files.
pub fn find_local_keys() -> Option<PathBuf> {
    let rd = fs::read_dir(root()).ok()?;
    let dirs = std::iter::once(root()).chain(rd.flatten().filter(|e| e.path().is_dir()).filter(|e| !["Data", "Roms", "Roms Dumps", "Old Versions", "Save Backups", "Covers"].contains(&&*e.file_name().to_string_lossy())).map(|e| e.path()));
    dirs.map(|d| d.join("prod.keys")).find(|p| p.is_file())
}

pub fn find_local_firmware() -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(root()).ok()?.flatten().map(|e| e.path()).filter(|p| {
        p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_lowercase().starts_with("firmware") && n.to_lowercase().ends_with(".zip"))
    }).collect();
    v.sort();
    v.pop()
}

/// Moves Data. The old copy stays until the user deletes it.
pub fn move_data(location: &str, custom: &str) -> Result<String> {
    if crate::emu::any_running() {
        bail!("Ferme les émulateurs avant de déplacer les données.");
    }
    let mut s = load_settings();
    let from = data_path_for(&s);
    s.data_location = location.into();
    s.custom_data_path = custom.into();
    let to = data_path_for(&s);
    if from == to {
        return Ok("Les données sont déjà à cet endroit.".into());
    }
    if to.exists() && fs::read_dir(&to)?.next().is_some() {
        bail!("Le dossier cible n'est pas vide : {}", to.display());
    }
    copy_tree(&from, &to, true)?;
    if tree_stats(&from) != tree_stats(&to) {
        bail!("La copie est incomplète : rien n'a changé.");
    }
    save_settings(&s)?;
    repair_setup()?;
    let mut stamp = 0;
    let old = loop {
        let name = format!("{}.old{}", from.file_name().unwrap().to_string_lossy(), if stamp == 0 { String::new() } else { format!("-{stamp}") });
        let p = from.with_file_name(name);
        if !p.exists() {
            break p;
        }
        stamp += 1;
    };
    fs::rename(&from, &old)?;
    Ok(format!("Données déplacées vers {}. L'ancienne copie reste dans {} : supprime-la quand tout fonctionne.", to.display(), old.display()))
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

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02X}")).collect()
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    std::io::copy(&mut fs::File::open(path)?, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02X}")).collect())
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().user_agent("Forkbay").timeout_read(std::time::Duration::from_secs(120)).timeout_connect(std::time::Duration::from_secs(20)).build()
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
