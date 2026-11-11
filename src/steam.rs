// Steam shortcuts: binary shortcuts.vdf (0 = group, 1 = text, 2 = integer, 8 = end).
use crate::core::*;
use crate::files::*;
use crate::games::*;
use crate::sys::*;
use anyhow::{bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub enum Val { Map(Vec<Node>), Text(String), Int(i32) }
#[derive(Clone)]
pub struct Node { pub key: String, pub val: Val }

fn read_cstr(b: &[u8], pos: &mut usize) -> Result<String> {
    let start = *pos;
    while *pos < b.len() && b[*pos] != 0 { *pos += 1; }
    if *pos >= b.len() { bail!("Fichier tronqué."); }
    let s = String::from_utf8(b[start..*pos].to_vec())?;
    *pos += 1;
    Ok(s)
}

fn read_map(b: &[u8], pos: &mut usize) -> Result<Vec<Node>> {
    let mut nodes = vec![];
    loop {
        if *pos >= b.len() { bail!("Fichier tronqué."); }
        let t = b[*pos];
        *pos += 1;
        if t == 8 { return Ok(nodes); }
        let key = read_cstr(b, pos)?;
        let val = match t {
            0 => Val::Map(read_map(b, pos)?),
            1 => Val::Text(read_cstr(b, pos)?),
            2 => {
                if *pos + 4 > b.len() { bail!("Fichier tronqué."); }
                let v = i32::from_le_bytes(b[*pos..*pos + 4].try_into().unwrap());
                *pos += 4;
                Val::Int(v)
            }
            _ => bail!("Type inconnu : {t}"),
        };
        nodes.push(Node { key, val });
    }
}

fn write_map(out: &mut Vec<u8>, nodes: &[Node]) {
    for n in nodes {
        let t = match n.val { Val::Map(_) => 0u8, Val::Text(_) => 1, Val::Int(_) => 2 };
        out.push(t);
        out.extend_from_slice(n.key.as_bytes());
        out.push(0);
        match &n.val {
            Val::Map(m) => write_map(out, m),
            Val::Text(s) => { out.extend_from_slice(s.as_bytes()); out.push(0); }
            Val::Int(i) => out.extend_from_slice(&i.to_le_bytes()),
        }
    }
    out.push(8);
}

/// Refuses any file it could not rewrite byte for byte.
pub fn read_shortcuts(path: &Path) -> Result<Vec<Node>> {
    let Ok(bytes) = fs::read(path) else { return Ok(vec![]) };
    if bytes.is_empty() { return Ok(vec![]); }
    let mut pos = 0;
    let mut root = read_map(&bytes, &mut pos)?;
    if pos != bytes.len() || root.len() != 1 || !root[0].key.eq_ignore_ascii_case("shortcuts") {
        bail!("Format de shortcuts.vdf inattendu.");
    }
    let mut check = vec![];
    write_map(&mut check, &root);
    if check != bytes { bail!("Impossible de réécrire ce fichier à l'identique : il n'a pas été modifié."); }
    match root.remove(0).val { Val::Map(m) => Ok(m), _ => bail!("Format de shortcuts.vdf inattendu.") }
}

fn field<'a>(e: &'a Node, name: &str) -> Option<&'a Val> {
    if let Val::Map(m) = &e.val { m.iter().find(|n| n.key.eq_ignore_ascii_case(name)).map(|n| &n.val) } else { None }
}

fn text_field(e: &Node, name: &str) -> String {
    if let Some(Val::Text(s)) = field(e, name) { s.clone() } else { String::new() }
}

fn set_text(e: &mut Node, name: &str, value: &str) {
    if let Val::Map(m) = &mut e.val {
        if let Some(n) = m.iter_mut().find(|n| n.key.eq_ignore_ascii_case(name)) { n.val = Val::Text(value.into()); }
    }
}

const TOOL_FLAG: &str = "--steam-ui";
const OLD_TOOL_FLAG: &str = "--fullscreen";

fn is_tool(e: &Node) -> bool {
    let o = text_field(e, "LaunchOptions");
    o.contains(TOOL_FLAG) || o.contains(OLD_TOOL_FLAG)
}

/// True in Steam's full-screen interfaces (Big Picture, the Steam Deck's Gaming Mode), not on the desktop.
pub fn big_picture() -> bool {
    let on = |k: &str| std::env::var(k).is_ok_and(|v| v == "1");
    if on("SteamGamepadUI") || on("SteamTenfoot") || std::env::var_os("GAMESCOPE_WAYLAND_DISPLAY").is_some() {
        return true;
    }
    if std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|v| v.eq_ignore_ascii_case("gamescope")) {
        return true;
    }
    #[cfg(windows)]
    {
        let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam");
        if key.and_then(|k| k.get_value::<u32, _>("BigPictureInForeground")).is_ok_and(|v| v != 0) {
            return true;
        }
    }
    false
}

pub fn managed_game(e: &Node) -> Option<String> {
    let re = Regex::new(r#"--launch (?:'((?:[^']|'')+)'|"([^"]+)")"#).unwrap();
    re.captures(&text_field(e, "LaunchOptions")).and_then(|c| c.get(1).map(|m| m.as_str().replace("''", "'")).or_else(|| c.get(2).map(|m| m.as_str().to_string())))
}

fn game_rel(file: &str) -> String { if cfg!(windows) { game_arg(file) } else { game_arg(file).replace('\'', "''") } }

const LAUNCHER: &str = "forkbay-launcher.exe";

// Steam keeps its shortcuts on this PC, so the small finder stays next to them.
fn prepare_launcher(vdf: &Path) -> Result<PathBuf> {
    if !cfg!(windows) {
        return Ok(exe_path());
    }
    let dir = vdf.parent().ok_or_else(|| anyhow::anyhow!("Steam introuvable sur ce PC."))?;
    fs::create_dir_all(dir)?;
    let target = dir.join(LAUNCHER);
    let me = exe_path();
    if target.is_file() && sha256_file(&target)? == sha256_file(&me)? {
        return Ok(target);
    }
    let tmp = target.with_extension("exe.tmp");
    fs::copy(&me, &tmp)?;
    // A copy in use still works: it only has to find the program.
    if fs::rename(&tmp, &target).is_err() {
        let _ = fs::remove_file(&tmp);
        if !target.is_file() {
            bail!("Copie impossible : {}", target.display());
        }
    }
    Ok(target)
}

/// Started by Steam on Windows: looks for the program on every drive, then waits for it.
pub fn find_and_run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let Some(rel) = args.first() else { return };
    for letter in 'A'..='Z' {
        let p = PathBuf::from(format!("{letter}:\\{rel}"));
        if p.is_file() {
            let mut c = std::process::Command::new(&p);
            c.args(&args[1..]);
            if let Some(d) = p.parent() { c.current_dir(d); }
            if let Ok(mut child) = c.spawn() {
                let _ = child.wait();
                return;
            }
        }
    }
    rfd::MessageDialog::new().set_title("NX2 Launcher").set_description(crate::i18n::t("Dossier de NX2 Launcher introuvable. Branche le disque, ou ouvre NX2 Launcher depuis son nouvel emplacement et clique sur Enregistrer dans Steam.")).set_level(rfd::MessageLevel::Warning).show();
}

fn exe_path() -> PathBuf { std::env::current_exe().unwrap_or_default() }

fn relative_exe() -> String {
    let p = exe_path().to_string_lossy().replace('/', "\\");
    match p.find(":\\") { Some(i) => p[i + 2..].to_string(), None => p }
}

// On Windows the drive letter is not stored: the launcher looks for it each time.
fn exe_and_options(file: &str, launcher: &Path) -> (String, String) {
    if cfg!(windows) {
        (format!("\"{}\"", launcher.display()), format!("--find \"{}\" --launch \"{}\"", relative_exe(), game_rel(file)))
    } else {
        (format!("\"{}\"", launcher.display()), format!("--launch '{}'", game_rel(file)))
    }
}

fn tool_exe_and_options(launcher: &Path) -> (String, String) {
    if cfg!(windows) {
        (format!("\"{}\"", launcher.display()), format!("--find \"{}\" {TOOL_FLAG}", relative_exe()))
    } else {
        (format!("\"{}\"", launcher.display()), TOOL_FLAG.to_string())
    }
}

fn is_current(e: &Node) -> bool {
    if cfg!(windows) { text_field(e, "LaunchOptions").contains(&format!("--find \"{}\"", relative_exe())) && Path::new(text_field(e, "Exe").trim_matches('"')).is_file() } else { text_field(e, "Exe") == format!("\"{}\"", exe_path().display()) }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc ^= *b as u32;
        for _ in 0..8 { crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 }; }
    }
    !crc
}

fn icon_for(file: &str) -> String {
    if let Some(a) = crate::art::find_art(file, "icon") { return a.to_string_lossy().into_owned(); }
    let emu = game_emulator(&load_settings(), file);
    crate::emu::find_exe(&root().join(emu)).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

fn new_entry(file: &str, launcher: &Path) -> Node {
    let (exe, opts) = exe_and_options(file, launcher);
    build_entry(display_name(file), exe, opts, icon_for(file))
}

fn tool_entry(launcher: &Path) -> Node {
    let (exe, opts) = tool_exe_and_options(launcher);
    build_entry("NX2 Launcher".to_string(), exe, opts, exe_path().to_string_lossy().into_owned())
}

fn build_entry(name: String, exe: String, opts: String, icon: String) -> Node {
    let appid = (crc32(format!("{exe}{name}").as_bytes()) | 0x8000_0000) as i32;
    let start = Path::new(exe.trim_matches('"')).parent().map(|p| format!("{}{}", p.display(), std::path::MAIN_SEPARATOR)).unwrap_or_default();
    let t = |k: &str, v: String| Node { key: k.into(), val: Val::Text(v) };
    let i = |k: &str, v: i32| Node { key: k.into(), val: Val::Int(v) };
    Node { key: "0".into(), val: Val::Map(vec![
        i("appid", appid), t("AppName", name), t("Exe", exe), t("StartDir", start), t("icon", icon), t("ShortcutPath", String::new()), t("LaunchOptions", opts),
        i("IsHidden", 0), i("AllowDesktopConfig", 1), i("AllowOverlay", 1), i("OpenVR", 0), i("Devkit", 0), t("DevkitGameID", String::new()), i("DevkitOverrideAppID", 0), i("LastPlayTime", 0), t("FlatpakAppID", String::new()),
        Node { key: "tags".into(), val: Val::Map(vec![]) },
    ]) }
}

/// Adds, removes and refreshes this tool's games. Other shortcuts do not change.
pub fn save_shortcuts(path: &Path, wanted: &[String], tool: bool) -> Result<()> {
    let entries = read_shortcuts(path)?;
    let launcher = prepare_launcher(path)?;
    let mut result = vec![];
    let mut done: Vec<String> = vec![];
    let mut tool_done = false;
    for mut e in entries {
        if is_tool(&e) {
            if !tool { continue; }
            let (exe, opts) = tool_exe_and_options(&launcher);
            set_text(&mut e, "Exe", &exe);
            set_text(&mut e, "LaunchOptions", &opts);
            set_text(&mut e, "icon", &exe_path().to_string_lossy());
            result.push(e);
            tool_done = true;
            continue;
        }
        let Some(game) = managed_game(&e) else { result.push(e); continue };
        let file = game.trim_start_matches("Roms/").trim_start_matches("Roms\\").to_string();
        if !wanted.contains(&file) { continue; }
        let (exe, opts) = exe_and_options(&file, &launcher);
        set_text(&mut e, "Exe", &exe);
        set_text(&mut e, "LaunchOptions", &opts);
        set_text(&mut e, "icon", &icon_for(&file));
        result.push(e);
        done.push(file);
    }
    for f in wanted { if !done.contains(f) { result.push(new_entry(f, &launcher)); } }
    if tool && !tool_done { result.push(tool_entry(&launcher)); }
    for (i, e) in result.iter_mut().enumerate() { e.key = i.to_string(); }
    let mut bytes = vec![];
    write_map(&mut bytes, &[Node { key: "shortcuts".into(), val: Val::Map(result) }]);
    if path.is_file() {
        let backup = PathBuf::from(format!("{}.before-forkbay", path.display()));
        if !backup.exists() { fs::copy(path, backup)?; }
    } else {
        fs::create_dir_all(path.parent().unwrap())?;
    }
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    fs::write(&tmp, &bytes)?;
    read_shortcuts(&tmp)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn steam_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FORKBAY_STEAM") { return Some(p.into()); }
    #[cfg(windows)]
    {
        let k = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam").ok()?;
        let p: String = k.get_value("SteamPath").ok()?;
        Some(p.replace('/', "\\").into())
    }
    #[cfg(not(windows))]
    {
        let h = home();
        [h.join(".steam/steam"), h.join(".local/share/Steam"), h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam")].into_iter().find(|p| p.join("userdata").is_dir())
    }
}

pub fn vdf_path(steam: &Path, account: &str) -> PathBuf { steam.join("userdata").join(account).join("config").join("shortcuts.vdf") }

fn active_account() -> String {
    #[cfg(windows)]
    {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam\\ActiveProcess").ok()
            .and_then(|k| k.get_value::<u32, _>("ActiveUser").ok()).map(|v| v.to_string()).unwrap_or_default()
    }
    #[cfg(not(windows))]
    { String::new() }
}

pub fn accounts(steam: &Path) -> Vec<Value> {
    let mut names = std::collections::HashMap::new();
    if let Ok(text) = fs::read_to_string(steam.join("config/loginusers.vdf")) {
        for c in Regex::new(r#""(\d{17})"\s*\{[^}]*?"PersonaName"\s*"([^"]*)""#).unwrap().captures_iter(&text) {
            if let Ok(id) = c[1].parse::<u64>() { names.insert((id - 76561197960265728).to_string(), c[2].to_string()); }
        }
    }
    let active = active_account();
    let mut list: Vec<Value> = fs::read_dir(steam.join("userdata")).map(|rd| rd.flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "0" && n.chars().all(|c| c.is_ascii_digit()))
        .map(|id| json!({ "id": id, "label": names.get(&id).cloned().unwrap_or_else(|| id.clone()), "active": id == active }))
        .collect()).unwrap_or_default();
    list.sort_by_key(|a| !a["active"].as_bool().unwrap_or(false));
    list
}

/// Current state of one account, for the page.
pub fn load(account: &str) -> Result<Value> {
    let steam = steam_dir().ok_or_else(|| anyhow::anyhow!("Steam introuvable sur ce PC."))?;
    let entries = read_shortcuts(&vdf_path(&steam, account))?;
    let mut managed = vec![];
    let mut stale = 0;
    let mut tool = false;
    for e in &entries {
        if is_tool(e) { tool = true; if !is_current(e) || !text_field(e, "LaunchOptions").contains(TOOL_FLAG) { stale += 1; } continue; }
        if let Some(g) = managed_game(e) {
            managed.push(g.trim_start_matches("Roms/").trim_start_matches("Roms\\").to_string());
            if !is_current(e) { stale += 1; }
        }
    }
    Ok(json!({ "managed": managed, "stale": stale, "tool": tool }))
}

fn steam_running() -> bool { running_processes().iter().any(|p| p == "steam") }

/// Closes Steam when needed (it would overwrite the change), then reopens it.
pub fn save(account: &str, games: &[String], tool: bool) -> Result<String> {
    let steam = steam_dir().ok_or_else(|| anyhow::anyhow!("Steam introuvable sur ce PC."))?;
    let bin = if cfg!(windows) { steam.join("steam.exe") } else { PathBuf::from("steam") };
    let test_mode = std::env::var("FORKBAY_STEAM").is_ok();
    let mut restart = false;
    if !test_mode && steam_running() {
        let _ = std::process::Command::new(&bin).arg("-shutdown").spawn();
        for _ in 0..90 {
            if !steam_running() { break; }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if steam_running() { bail!("Steam ne s'est pas fermé : rien n'a été modifié."); }
        restart = true;
    }
    let r = save_shortcuts(&vdf_path(&steam, account), games, tool).and_then(|_| crate::art::update_steam_art(&vdf_path(&steam, account)));
    if restart { let _ = std::process::Command::new(&bin).spawn(); }
    let images = r?;
    Ok(format!("Enregistré : {} jeu(x) dans Steam, {images} image(s).", games.len()))
}

pub fn app_id(e: &Node) -> u32 { if let Some(Val::Int(v)) = field(e, "appid") { *v as u32 } else { 0 } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_file_is_rewritten_byte_for_byte() {
        let entry = build_entry("Game".into(), "\"/apps/forkbay\"".into(), "--launch x".into(), String::new());
        let Val::Int(appid) = field(&entry, "appid").unwrap().clone() else { panic!("no appid") };
        assert!(appid < 0, "Steam needs the high bit of a shortcut id");
        let root = vec![Node { key: "shortcuts".into(), val: Val::Map(vec![entry]) }];
        let mut bytes = vec![];
        write_map(&mut bytes, &root);
        let mut pos = 0;
        let read = read_map(&bytes, &mut pos).unwrap();
        assert_eq!(pos, bytes.len());
        let mut again = vec![];
        write_map(&mut again, &read);
        assert_eq!(again, bytes);
        assert!(read_map(&bytes[..bytes.len() - 3], &mut 0).is_err());
    }

    #[test]
    fn crc32_matches_the_standard() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
