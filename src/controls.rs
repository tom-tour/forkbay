// The controls every emulator reads: [Controls] in config/qt-config.ini, input profiles in
// config/input/<name>.ini, and a game's own profile in config/custom/<program id>.ini.
// Read and written the way the emulators do it (src/frontend_common/config.cpp,
// src/qt_common/config/qt_config.cpp and src/yuzu/configuration/configure_input*.cpp in Eden and
// its forks), so that a change made here is what any of them would have written.
use crate::config::{load, prepare_common_edit, put, save, section_range};
use crate::core::*;
use crate::params::{float, Params};
use crate::sdl;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const SECTION: &str = "Controls";

/// The Switch buttons, in the emulators' order.
pub const BUTTONS: [&str; 22] = [
    "button_a", "button_b", "button_x", "button_y", "button_lstick", "button_rstick", "button_l", "button_r", "button_zl", "button_zr", "button_plus",
    "button_minus", "button_dleft", "button_dup", "button_dright", "button_ddown", "button_slleft", "button_srleft", "button_home", "button_screenshot",
    "button_slright", "button_srright",
];
pub const STICKS: [&str; 2] = ["lstick", "rstick"];
pub const MOTIONS: [&str; 2] = ["motionleft", "motionright"];

// The keys a new player gets, as Qt key codes: QtConfig::default_buttons and its neighbours.
const KEY_LEFT: i32 = 0x0100_0012;
const KEY_UP: i32 = 0x0100_0013;
const KEY_RIGHT: i32 = 0x0100_0014;
const KEY_DOWN: i32 = 0x0100_0015;
const KEY_SHIFT: i32 = 0x0100_0020;
const DEFAULT_KEYS: [i32; 22] = [
    0x43, 0x58, 0x56, 0x5a, 0x46, 0x47, 0x51, 0x45, 0x52, 0x54, 0x4d, 0x4e, KEY_LEFT, KEY_UP, KEY_RIGHT, KEY_DOWN, 0x51, 0x45, 0, 0, 0x51, 0x45,
];
const DEFAULT_STICK_KEYS: [[i32; 4]; 2] = [[0x57, 0x53, 0x41, 0x44], [0x49, 0x4b, 0x4a, 0x4c]];
const DEFAULT_STICK_MOD: [i32; 2] = [KEY_SHIFT, 0];
const DEFAULT_MOTION_KEYS: [i32; 2] = [0x37, 0x38];

/// Player 1 in handheld mode uses this player's controls.
const HANDHELD: usize = 8;
const TYPE_HANDHELD: u32 = 4;
const PLAYERS: usize = 10;

/// Characters that make the emulators put a value between quotes.
const SPECIAL: [char; 18] = ['!', '#', '$', '%', '^', '&', '*', '|', ';', '\'', '"', ',', '<', '>', '?', '`', '~', '='];

fn keyboard(code: i32) -> String {
    Params::new(&[("engine", "keyboard".into()), ("code", code.to_string()), ("toggle", "0".into())]).serialize()
}

fn keyboard_stick(i: usize) -> String {
    let k = DEFAULT_STICK_KEYS[i];
    Params::new(&[
        ("engine", "analog_from_button".into()),
        ("up", keyboard(k[0])),
        ("down", keyboard(k[1])),
        ("left", keyboard(k[2])),
        ("right", keyboard(k[3])),
        ("modifier", keyboard(DEFAULT_STICK_MOD[i])),
        ("modifier_scale", float(0.5)),
    ])
    .serialize()
}

fn default_of(name: &str) -> String {
    if let Some(i) = BUTTONS.iter().position(|b| *b == name) { return keyboard(DEFAULT_KEYS[i]); }
    if let Some(i) = STICKS.iter().position(|b| *b == name) { return keyboard_stick(i); }
    if let Some(i) = MOTIONS.iter().position(|b| *b == name) { return keyboard(DEFAULT_MOTION_KEYS[i]); }
    String::new()
}

pub(crate) fn common_path() -> PathBuf {
    crate::config::common_path()
}

fn profiles_dir() -> PathBuf {
    data_path().join("config").join("input")
}

fn raw(lines: &[String], key: &str) -> Option<String> {
    let (start, end) = section_range(lines, SECTION)?;
    let prefix = format!("{key}=");
    lines[start..end].iter().find_map(|l| l.strip_prefix(prefix.as_str()).map(|v| v.trim().to_string()))
}

/// A value as the emulators read it: the default unless "\default" says otherwise.
fn setting(lines: &[String], key: &str) -> Option<String> {
    if raw(lines, &format!("{key}\\default")).is_none_or(|d| d != "false") { return None; }
    raw(lines, key).map(|v| v.replace('"', "").replace("//", "/"))
}

fn flag(lines: &[String], key: &str, default: bool) -> bool {
    setting(lines, key).map_or(default, |v| v == "true" || v == "1")
}

fn number(lines: &[String], key: &str, default: u32) -> u32 {
    setting(lines, key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn binding(lines: &[String], key: &str, name: &str) -> String {
    setting(lines, key).filter(|v| !v.is_empty()).unwrap_or_else(|| default_of(name))
}

pub(crate) fn quote(v: &str) -> String {
    if v.contains(SPECIAL) { format!("\"{v}\"") } else { v.to_string() }
}

/// The value line first, then "\default" above it, as the emulators order them.
fn write(lines: &mut Vec<String>, key: &str, value: String, is_default: bool) {
    put(lines, SECTION, &format!("{key}="), format!("{key}={value}"));
    put(lines, SECTION, &format!("{key}\\default="), format!("{key}\\default={is_default}"));
}

/// Controls are written with "\default=false", whatever their value: the emulators do the same.
fn write_binding(lines: &mut Vec<String>, key: &str, value: &str) {
    write(lines, key, quote(value), false);
}

fn prefix(player: Option<usize>) -> String {
    player.map(|p| format!("player_{p}_")).unwrap_or_default()
}

fn bindings_of(lines: &[String], player: Option<usize>) -> Value {
    let pre = prefix(player);
    let read = |names: &[&str]| -> serde_json::Map<String, Value> { names.iter().map(|n| (n.to_string(), json!(binding(lines, &format!("{pre}{n}"), n)))).collect() };
    json!({ "buttons": read(&BUTTONS), "sticks": read(&STICKS), "motions": read(&MOTIONS) })
}

fn player_of(lines: &[String], p: usize) -> Value {
    let pre = prefix(Some(p));
    json!({
        "index": p,
        "connected": flag(lines, &format!("{pre}connected"), p == 0),
        "type": number(lines, &format!("{pre}type"), 0),
        "vibration": flag(lines, &format!("{pre}vibration_enabled"), true),
        "vibrationStrength": number(lines, &format!("{pre}vibration_strength"), 100),
        "controls": bindings_of(lines, Some(p)),
    })
}

/// The emulators' general input settings shown on the page, with their defaults.
const ADVANCED: [(&str, bool); 7] = [
    ("vibration_enabled", true),
    ("enable_accurate_vibrations", false),
    ("motion_enabled", true),
    ("enable_joycon_driver", true),
    ("enable_procon_driver", false),
    ("enable_raw_input", false),
    ("controller_navigation", true),
];

/// The emulators' settings that change what SDL sees.
pub fn options() -> sdl::Options {
    let (lines, _) = load(&common_path());
    sdl::Options {
        raw_input: flag(&lines, "enable_raw_input", false),
        disable_wgi_xinput: flag(&lines, "disable_wgi_xinput", false),
        joycon_driver: flag(&lines, "enable_joycon_driver", true),
        procon_driver: flag(&lines, "enable_procon_driver", false),
    }
}

pub fn profile_names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(profiles_dir()).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ini")).filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())).filter(|n| valid_name(n)).collect()).unwrap_or_default();
    names.sort();
    names
}

/// Everything the page shows.
pub fn get() -> Value {
    let (lines, _) = load(&common_path());
    let players: Vec<Value> = (0..PLAYERS).map(|p| player_of(&lines, p)).collect();
    let advanced: serde_json::Map<String, Value> = ADVANCED.iter().map(|(k, d)| (k.to_string(), json!(flag(&lines, k, *d)))).collect();
    let docked = crate::config::docked(&lines);
    json!({ "players": players, "profiles": profile_names(), "advanced": advanced, "docked": docked, "running": crate::emu::any_running(), "pending": crate::config::pending(), "nav": crate::core::load_settings().nav_controls })
}

fn change(f: impl FnOnce(&mut Vec<String>) -> Result<()>) -> Result<()> {
    prepare_common_edit()?;
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    f(&mut lines)?;
    save(&path, &lines, crlf)
}

/// A change to the players' controls: during a game, emulators that can read them again apply it at once.
fn change_players(f: impl FnOnce(&mut Vec<String>) -> Result<()>) -> Result<()> {
    change(f)?;
    if crate::config::push_controls_live() {
        crate::emit_status("Touches appliquées tout de suite dans la partie.");
    }
    Ok(())
}

fn check_player(p: usize) -> Result<()> {
    if p >= PLAYERS { bail!("Joueur inconnu."); }
    Ok(())
}

fn check_value(v: &str) -> Result<()> {
    if v.contains(['\n', '\r']) { bail!("Valeur refusée."); }
    Ok(())
}

fn is_control(name: &str) -> bool {
    BUTTONS.contains(&name) || STICKS.contains(&name) || MOTIONS.contains(&name)
}

/// One control of a player. An empty value clears it.
pub fn set_control(player: usize, name: &str, value: &str) -> Result<()> {
    check_player(player)?;
    if !is_control(name) { bail!("Réglage inconnu."); }
    check_value(value)?;
    let v = Params::parse(value).serialize();
    change_players(|lines| { write_binding(lines, &format!("player_{player}_{name}"), &v); Ok(()) })
}

/// Connects a player, with every player before it, or disconnects it with every player after it,
/// as the emulators keep players in order.
pub fn set_connected(player: usize, on: bool) -> Result<()> {
    check_player(player)?;
    change_players(|lines| {
        let handheld = number(lines, "player_0_type", 0) == TYPE_HANDHELD;
        let range: Vec<usize> = if on { (0..=player).collect() } else { (player..8).collect() };
        for p in range {
            // Player 1 in handheld mode is the handheld controller.
            let target = if p == 0 && handheld { HANDHELD } else { p };
            write(lines, &format!("player_{target}_connected"), on.to_string(), on == (target == 0));
        }
        Ok(())
    })
}

/// The kind of controller a player is. Handheld is only for player 1, and moves its controls to the
/// handheld controller, as the emulators do.
pub fn set_type(player: usize, kind: u32) -> Result<()> {
    check_player(player)?;
    if kind > 10 || (kind == TYPE_HANDHELD && player != 0) { bail!("Valeur refusée."); }
    change_players(|lines| {
        write(lines, &format!("player_{player}_type"), kind.to_string(), kind == 0);
        if player == 0 {
            let was = number(lines, "player_8_type", 0) == TYPE_HANDHELD || number(lines, "player_0_type", 0) == TYPE_HANDHELD;
            write(lines, "player_8_type", kind.to_string(), kind == 0);
            let connected = flag(lines, "player_0_connected", true) || flag(lines, "player_8_connected", false);
            if kind == TYPE_HANDHELD && connected {
                write(lines, "player_0_connected", "false".into(), false);
                write(lines, "player_8_connected", "true".into(), false);
                crate::config::set_docked(lines, false);
            } else if was && kind != TYPE_HANDHELD && connected {
                write(lines, "player_8_connected", "false".into(), true);
                write(lines, "player_0_connected", "true".into(), true);
            }
        }
        Ok(())
    })
}

pub fn set_vibration(player: usize, on: Option<bool>, strength: Option<u32>) -> Result<()> {
    check_player(player)?;
    change_players(|lines| {
        if let Some(on) = on { write(lines, &format!("player_{player}_vibration_enabled"), on.to_string(), on); }
        if let Some(s) = strength {
            if s > 100 { bail!("Valeur refusée."); }
            write(lines, &format!("player_{player}_vibration_strength"), s.to_string(), s == 100);
        }
        Ok(())
    })
}

pub fn set_advanced(key: &str, on: bool) -> Result<()> {
    let (_, default) = ADVANCED.iter().find(|(k, _)| *k == key).ok_or_else(|| anyhow!("Réglage inconnu."))?;
    let default = *default;
    change(|lines| { write(lines, key, on.to_string(), on == default); Ok(()) })
}

pub fn set_docked(docked: bool) -> Result<()> {
    change(|lines| { crate::config::set_docked(lines, docked); Ok(()) })
}

/// Clears every control of a player, as "Clear" does.
pub fn clear(player: usize) -> Result<()> {
    check_player(player)?;
    change_players(|lines| {
        for n in BUTTONS.iter().chain(&STICKS).chain(&MOTIONS) { write_binding(lines, &format!("player_{player}_{n}"), "[empty]"); }
        Ok(())
    })
}

/// The controls a player gets when a device is chosen: the keyboard's, or the controller's as SDL
/// maps it. Everything else is cleared first, as in the emulators.
pub fn use_device(player: usize, device: &Value) -> Result<()> {
    check_player(player)?;
    let mut values: Vec<(String, String)> = BUTTONS.iter().chain(&STICKS).chain(&MOTIONS).map(|n| (n.to_string(), "[empty]".to_string())).collect();
    let mut set = |name: &str, v: String| { if let Some(e) = values.iter_mut().find(|(n, _)| n == name) { e.1 = v; } };
    if device["kind"] == "keyboard" {
        for n in BUTTONS.iter().chain(&STICKS).chain(&MOTIONS) { set(n, default_of(n)); }
    } else {
        let guid = device["guid"].as_str().ok_or_else(|| anyhow!("Manette inconnue."))?.to_string();
        let port = device["port"].as_u64().unwrap_or(0) as usize;
        let guid2 = device["guid2"].as_str().map(str::to_string);
        let mapping = sdl::call(options(), move |s| s.defaults(&guid, port, guid2.as_deref()))??;
        for group in ["buttons", "sticks", "motions"] {
            if let Some(m) = mapping[group].as_object() { for (n, v) in m { set(n, v.as_str().unwrap_or("[empty]").to_string()); } }
        }
    }
    change(|lines| {
        for (n, v) in &values { write_binding(lines, &format!("player_{player}_{n}"), v); }
        Ok(())
    })
}

// --- Input profiles ---

/// The names the emulators accept for a profile.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= 30 && !name.contains(['<', '>', ':', ';', '"', '/', '\\', '|', ',', '.', '!', '?', '*']) && !name.trim().is_empty()
}

fn profile_path(name: &str) -> Result<PathBuf> {
    if !valid_name(name) { bail!("Ce nom de profil n'est pas valide : évite < > : ; \" / \\ | , . ! ? *"); }
    Ok(profiles_dir().join(format!("{name}.ini")))
}

fn write_profile(path: &Path, lines_from: &[String], player: usize) -> Result<()> {
    let (mut lines, crlf) = load(path);
    if section_range(&lines, SECTION).is_none() { lines.push(format!("[{SECTION}]")); }
    let kind = number(lines_from, &format!("player_{player}_type"), 0);
    write(&mut lines, "type", kind.to_string(), kind == 0);
    for n in BUTTONS.iter().chain(&STICKS).chain(&MOTIONS) {
        write_binding(&mut lines, n, &binding(lines_from, &format!("player_{player}_{n}"), n));
    }
    fs::create_dir_all(profiles_dir())?;
    save(path, &lines, crlf)
}

/// Saves a player's controls in a profile, new or existing. Player 1 in handheld mode saves the
/// handheld controls, as the emulators do.
pub fn save_profile(name: &str, player: usize, new: bool) -> Result<()> {
    check_player(player)?;
    let path = profile_path(name)?;
    if new && path.exists() { bail!("Un profil porte déjà ce nom."); }
    if !new && !path.exists() { bail!("Ce profil n'existe plus."); }
    let (lines, _) = load(&common_path());
    let source = if player == 0 && number(&lines, "player_0_type", 0) == TYPE_HANDHELD { HANDHELD } else { player };
    write_profile(&path, &lines, source)
}

/// Copies a profile into a player's controls. Its kind of controller only follows for a single
/// Joy-Con, as in the emulators.
pub fn load_profile(name: &str, player: usize) -> Result<()> {
    check_player(player)?;
    let path = profile_path(name)?;
    if !path.is_file() { bail!("Ce profil n'existe plus."); }
    let (profile, _) = load(&path);
    change_players(|lines| {
        let target = if player == 0 && number(lines, "player_0_type", 0) == TYPE_HANDHELD { HANDHELD } else { player };
        for n in BUTTONS.iter().chain(&STICKS).chain(&MOTIONS) {
            write_binding(lines, &format!("player_{target}_{n}"), &binding(&profile, n, n));
        }
        let kind = number(&profile, "type", 0);
        if kind == 2 || kind == 3 { write(lines, &format!("player_{player}_type"), kind.to_string(), false); }
        Ok(())
    })
}

pub fn delete_profile(name: &str) -> Result<()> {
    let path = profile_path(name)?;
    if path.is_file() { fs::remove_file(&path)?; }
    let _ = fs::remove_file(path.with_extension("ini.forkbay-backup"));
    Ok(())
}

// --- A game's own controls ---

/// The profile each player uses for a game, empty for the common controls.
pub fn game_profiles(file: &str) -> Result<Value> {
    let path = crate::config::game_ini_path(file)?;
    let (lines, _) = load(&path);
    let list: Vec<String> = (0..8).map(|p| setting(&lines, &format!("player_{p}_profile_name")).unwrap_or_default()).collect();
    Ok(json!({ "players": list, "profiles": profile_names() }))
}

/// Gives a player of a game a profile, or the common controls again (empty name). The profile is
/// copied into the game's settings with the player connected, as the emulators do.
pub fn set_game_profile(file: &str, player: usize, name: &str) -> Result<()> {
    if player >= 8 { bail!("Joueur inconnu."); }
    // A game's own file: it can change while the emulators run.
    let path = crate::config::game_ini_path(file)?;
    let (mut lines, crlf) = load(&path);
    let pre = format!("player_{player}_");
    let mine = |l: &String| l.starts_with(&pre);
    if let Some((start, end)) = section_range(&lines, SECTION) {
        let keep: Vec<String> = lines[start + 1..end].iter().filter(|l| !mine(l)).cloned().collect();
        lines.splice(start + 1..end, keep);
    }
    // Player 1 also stands for the handheld controller.
    if player == 0 { lines.retain(|l| !l.starts_with("player_8_")); }
    if !name.is_empty() {
        let source = profile_path(name)?;
        if !source.is_file() { bail!("Ce profil n'existe plus."); }
        let (profile, _) = load(&source);
        if section_range(&lines, SECTION).is_none() {
            if lines.last().is_some_and(|l| !l.is_empty()) { lines.push(String::new()); }
            lines.push(format!("[{SECTION}]"));
        }
        let kind = number(&profile, "type", 0);
        write(&mut lines, &format!("{pre}profile_name"), name.to_string(), false);
        write(&mut lines, &format!("{pre}type"), kind.to_string(), kind == 0);
        write(&mut lines, &format!("{pre}connected"), "true".into(), player == 0);
        for n in BUTTONS.iter().chain(&STICKS).chain(&MOTIONS) {
            write_binding(&mut lines, &format!("{pre}{n}"), &binding(&profile, n, n));
        }
    }
    save(&path, &lines, crlf)
}

// --- Moving around NX2 Launcher ---

/// The controls NX2 Launcher itself reads: the D-pad, the left stick, and the buttons it gives a use to.
pub const NAV: [&str; 10] = ["button_dup", "button_ddown", "button_dleft", "button_dright", "lstick", "button_a", "button_b", "button_y", "button_l", "button_r"];

/// NX2 Launcher's own controls, kept with its settings: the emulators never see them. None goes back to player 1's.
pub fn set_nav(controls: &Value) -> Result<()> {
    let mut s = crate::core::load_settings();
    s.nav_controls = match controls.as_object() {
        None => None,
        Some(m) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in m {
                let v = v.as_str().ok_or_else(|| anyhow!("Valeur refusée."))?;
                if !NAV.contains(&k.as_str()) || v.len() > 300 || v.contains(['\n', '\r']) { anyhow::bail!("Valeur refusée."); }
                out.insert(k.clone(), v.to_string());
            }
            Some(out)
        }
    };
    crate::core::save_settings(&s)
}

/// NX2 Launcher's own controls filled from a controller, as choosing it fills a player's.
pub fn nav_device(device: &Value) -> Result<()> {
    let guid = device["guid"].as_str().ok_or_else(|| anyhow!("Manette inconnue."))?.to_string();
    let port = device["port"].as_u64().unwrap_or(0) as usize;
    let guid2 = device["guid2"].as_str().map(str::to_string);
    let mapping = sdl::call(options(), move |s| s.defaults(&guid, port, guid2.as_deref()))??;
    let controls: serde_json::Map<String, Value> = NAV.iter().map(|n| {
        let group = if *n == "lstick" { "sticks" } else { "buttons" };
        (n.to_string(), json!(mapping[group][*n].as_str().unwrap_or("[empty]")))
    }).collect();
    set_nav(&Value::Object(controls))
}

// --- Controllers ---

pub fn devices() -> Result<Value> {
    sdl::call(options(), |s| s.devices())
}

pub fn states() -> Result<Value> {
    sdl::call(options(), |s| s.states())
}

pub fn capture_begin(kind: &str, only: Option<(String, usize)>) -> Result<()> {
    let kind = match kind { "stick" => sdl::Kind::Stick, "motion" => sdl::Kind::Motion, _ => sdl::Kind::Button };
    sdl::call(options(), move |s| s.begin(kind, only))
}

pub fn capture_poll() -> Result<Value> {
    Ok(json!(sdl::call(options(), |s| s.poll())?))
}

pub fn capture_stop() -> Result<()> {
    sdl::call(options(), |s| s.stop())
}

/// The family of a controller: its id without the version it reports, which differs from one
/// controller of the same model to another.
fn family(guid: &str) -> Option<String> {
    (guid.len() == 32).then(|| format!("{}0000{}", &guid[..24], &guid[28..]))
}

/// The controls with each controller that is not plugged in replaced by a plugged-in one of the same
/// model, when there is exactly one such. Returns how many controls changed.
fn follow(lines: &mut [String], plugged: &[(String, usize)]) -> usize {
    let Some((start, end)) = section_range(lines, SECTION) else { return 0 };
    let id_re = regex::Regex::new(r"guid(\$0|:)([0-9a-f]{32})").unwrap();
    let port_re = regex::Regex::new(r"port(\$0|:)\d+").unwrap();
    let mut changed = 0;
    for line in &mut lines[start..end] {
        if !line.contains("engine") || !line.contains("sdl") { continue; }
        let mut out = line.clone();
        let ids: Vec<String> = id_re.captures_iter(line).map(|c| c[2].to_string()).collect();
        for id in ids {
            if plugged.iter().any(|(g, _)| *g == id) { continue; }
            let Some(fam) = family(&id) else { continue };
            let same: Vec<&(String, usize)> = plugged.iter().filter(|(g, _)| family(g).as_deref() == Some(fam.as_str())).collect();
            if let [(g, port)] = same.as_slice() {
                out = out.replace(&id, g);
                out = port_re.replace_all(&out, |c: &regex::Captures| format!("port{}{port}", &c[1])).into_owned();
                changed += 1;
            }
        }
        if out != *line { *line = out; }
    }
    changed
}

/// Before a game: controls made with a controller that is not plugged in now move to the one of
/// the same model that is, so that a controller swapped for its twin keeps working.
pub fn follow_plugged() -> Result<usize> {
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    if !lines.iter().any(|l| l.contains("engine:sdl")) { return Ok(0); }
    let plugged = sdl::call(options(), |s| s.plugged())?;
    sdl::stop();
    let n = follow(&mut lines, &plugged);
    if n > 0 {
        save(&path, &lines, crlf)?;
        crate::log::info(&format!("controls moved to the plugged-in controller: {n}"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(t: &str) -> Vec<String> {
        t.lines().map(str::to_string).collect()
    }

    #[test]
    fn controls_read_like_the_emulators() {
        let l = lines("[Controls]\nplayer_0_button_a\\default=false\nplayer_0_button_a=\"engine:sdl,button:0,guid:030000005e0400008e02000014017801,port:0\"\nplayer_0_button_b\\default=true\nplayer_0_button_b=\"engine:sdl,button:1\"\nplayer_0_motionleft\\default=false\nplayer_0_motionleft=[empty]");
        assert_eq!(binding(&l, "player_0_button_a", "button_a"), "engine:sdl,button:0,guid:030000005e0400008e02000014017801,port:0");
        // Marked as default: the keyboard key, whatever the line says.
        assert_eq!(binding(&l, "player_0_button_b", "button_b"), "engine:keyboard,code:88,toggle:0");
        assert_eq!(binding(&l, "player_0_motionleft", "motionleft"), "[empty]");
        assert_eq!(binding(&l, "player_0_button_x", "button_x"), "engine:keyboard,code:86,toggle:0");
        assert!(flag(&l, "player_0_connected", true));
    }

    #[test]
    fn controls_written_like_the_emulators() {
        let mut l = lines("[Controls]\nplayer_0_button_a\\default=true\nplayer_0_button_a=x\n[System]\nother=1");
        write_binding(&mut l, "player_0_button_a", "engine:sdl,button:1,port:0");
        write_binding(&mut l, "player_0_motionleft", "[empty]");
        let t = l.join("\n");
        assert!(t.contains("player_0_button_a\\default=false\nplayer_0_button_a=\"engine:sdl,button:1,port:0\""), "{t}");
        assert!(t.contains("[Controls]\nplayer_0_motionleft\\default=false\nplayer_0_motionleft=[empty]\n"), "{t}");
        assert!(t.ends_with("[System]\nother=1"));
        assert_eq!(keyboard_stick(0), "engine:analog_from_button,up:engine$0keyboard$1code$087$1toggle$00,down:engine$0keyboard$1code$083$1toggle$00,left:engine$0keyboard$1code$065$1toggle$00,right:engine$0keyboard$1code$068$1toggle$00,modifier:engine$0keyboard$1code$016777248$1toggle$00,modifier_scale:0.500000");
    }

    #[test]
    fn controls_follow_the_twin_controller_plugged_in() {
        let a = "030000005e0400008e02000010017801";
        let b = "030000005e0400008e02000014017801";
        let mut l = lines(&format!("[Controls]\nplayer_0_button_a=\"engine:sdl,button:0,guid:{a},port:1\"\nplayer_0_lstick=\"engine:analog_from_button,up:engine$0sdl$1guid$0{a}$1port$01$1button$00\"\nplayer_1_button_a=\"engine:keyboard,code:67\""));
        // Its twin is plugged in: the controls move to it.
        assert_eq!(follow(&mut l, &[(b.to_string(), 0)]), 2);
        let t = l.join("\n");
        assert!(t.contains(&format!("guid:{b},port:0")) && t.contains(&format!("guid$0{b}$1port$00")) && !t.contains(a), "{t}");
        // Plugged in: nothing to do. Two twins plugged in: no guess.
        assert_eq!(follow(&mut l, &[(b.to_string(), 0)]), 0);
        let mut l2 = lines(&format!("[Controls]\nplayer_0_button_a=\"engine:sdl,guid:{a},port:0\""));
        assert_eq!(follow(&mut l2, &[(b.to_string(), 0), (b.to_string(), 1)]), 0);
    }
}
