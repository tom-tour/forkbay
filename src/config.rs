// Settings every emulator reads. The emulators share one data folder: the common settings are
// in config/qt-config.ini, and a game's own settings in config/custom/<program id>.ini.
// The values were checked against the source of each emulator in emulators.json: their tables
// are identical.
use crate::core::*;
use crate::games::*;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Name, section, allowed values and the emulators' default.
const FIELDS: [(&str, &str, &[u32], u32); 8] = [
    ("language_index", "System", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19], 1),
    ("region_index", "System", &[0, 1, 2, 3, 4, 5, 6], 1),
    ("use_docked_mode", "System", &[0, 1], 0),
    ("resolution_setup", "Renderer", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], 3),
    ("use_vsync", "Renderer", &[0, 1, 2, 3], 2),
    ("fullscreen_mode", "Renderer", &[0, 1], 0),
    // 2 is the renderer that draws nothing, only meant for debugging.
    ("backend", "Renderer", &[0, 1, 3, 4], 1),
    ("vulkan_device", "Renderer", &[0, 1, 2, 3, 4, 5, 6, 7], 0),
];

/// The lines of the emulators' files that each setting of the page uses.
fn lines_of(key: &str) -> Vec<(&'static str, &'static str)> {
    match key {
        "speed" => vec![("Core", "use_speed_limit"), ("Core", "speed_limit")],
        "volume" => vec![("Audio", "volume")],
        _ => FIELDS.iter().filter(|f| f.0 == key).map(|f| (f.1, f.0)).collect(),
    }
}

fn keys() -> Vec<&'static str> {
    FIELDS.iter().map(|f| f.0).chain(["speed", "volume"]).collect()
}

fn real_common_path() -> PathBuf {
    data_path().join("config").join("qt-config.ini")
}

/// Where a change made while an emulator runs waits: the file as wanted, and the file as it was when the wait began.
fn pending_dir() -> PathBuf {
    manager_dir().join("pending")
}

/// The common settings file to read and write. While an emulator runs and a change waits, it is the waiting copy,
/// so that the pages show what will be applied. Once every emulator is closed, the change is applied first.
pub(crate) fn common_path() -> PathBuf {
    let want = pending_dir().join("qt-config.ini");
    if want.exists() {
        if crate::emu::any_running() { return want; }
        if let Err(e) = apply_pending() { crate::log::error(&format!("waiting settings: {e}")); return want; }
    }
    real_common_path()
}

/// The lines of a settings file that hold a value, by section and name.
fn values(lines: &[String]) -> std::collections::BTreeMap<(String, String), String> {
    let mut out = std::collections::BTreeMap::new();
    let mut section = String::new();
    for l in lines {
        let t = l.trim();
        if t.starts_with('[') && t.ends_with(']') { section = t[1..t.len() - 1].to_string(); continue; }
        if t.starts_with(';') || t.starts_with('#') { continue; }
        if let Some((k, _)) = l.split_once('=') { out.insert((section.clone(), k.to_string()), l.clone()); }
    }
    out
}

/// The end of a game, the watch of NX2 Launcher and a change during a game may all write the emulators' file: one at a time.
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Applies what changed while an emulator ran onto the file it wrote when it closed: only those values,
/// so that what was changed in the emulator meanwhile stays.
pub(crate) fn apply_pending() -> Result<()> {
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let dir = pending_dir();
    let want_path = dir.join("qt-config.ini");
    if !want_path.exists() { return Ok(()); }
    let (base, _) = load(&dir.join("base.ini"));
    let (want, _) = load(&want_path);
    let (base, want) = (values(&base), values(&want));
    merge_into_real(&base, &want)?;
    fs::remove_dir_all(&dir)?;
    crate::log::info("waiting settings applied");
    Ok(())
}

/// Writes onto the emulators' own file only the values that changed in the waiting copy.
fn merge_into_real(base: &std::collections::BTreeMap<(String, String), String>, want: &std::collections::BTreeMap<(String, String), String>) -> Result<()> {
    let real = real_common_path();
    let (mut lines, crlf) = load(&real);
    for ((s, k), line) in want {
        if base.get(&(s.clone(), k.clone())) != Some(line) { put(&mut lines, s, &format!("{k}="), line.clone()); }
    }
    for (s, k) in base.keys().filter(|key| !want.contains_key(*key)) {
        if let Some((start, end)) = section_range(&lines, s) {
            let prefix = format!("{k}=");
            if let Some(i) = lines[start..end].iter().position(|l| l.starts_with(&prefix)) { lines.remove(start + i); }
        }
    }
    save(&real, &lines, crlf)
}

/// Emulators that know the "reload-controls" request apply new controls during the game: the players' controls
/// go to their file at once, and the request asks them to read it again. They remove the request once done.
/// The waiting copy stays: it still brings the other settings when they close. Returns whether it was applied.
pub(crate) fn push_controls_live() -> bool {
    if !pending() || !crate::emu::any_running() { return false; }
    // Asked once already during this game and not taken: the emulator does not know the request.
    let unknown = pending_dir().join("no-reload-request");
    if unknown.exists() { return false; }
    {
        let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
        let dir = pending_dir();
        let (base, _) = load(&dir.join("base.ini"));
        let (want, _) = load(&dir.join("qt-config.ini"));
        let player = |k: &(String, String)| k.0 == "Controls" && k.1.starts_with("player_");
        let base = values(&base);
        let want: std::collections::BTreeMap<_, _> = values(&want).into_iter().filter(|(k, _)| player(k)).collect();
        let base = base.into_iter().filter(|(k, _)| player(k)).collect();
        if let Err(e) = merge_into_real(&base, &want) { crate::log::error(&format!("controls during the game: {e}")); return false; }
    }
    let request = data_path().join("config").join("reload-controls");
    if fs::write(&request, b"").is_err() { return false; }
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !request.exists() { return true; }
    }
    // This emulator does not know the request yet: the controls wait for it to close, as the other settings.
    let _ = fs::remove_file(&request);
    let _ = fs::write(&unknown, b"");
    false
}

/// The program id in the file name, such as "[0100000000010000]".
pub fn game_id(file: &str) -> Option<String> {
    Regex::new(r"\[([0-9A-Fa-f]{16})\]").unwrap().captures(file).map(|c| c[1].to_uppercase())
}

fn game_ini(file: &str) -> Option<PathBuf> {
    game_id(file).map(|id| data_path().join("config").join("custom").join(format!("{id}.ini")))
}

fn game_path(file: &str) -> Result<PathBuf> {
    if !rom_files().iter().any(|f| f == file) { bail!("Jeu introuvable : {file}"); }
    game_ini(file).ok_or_else(|| anyhow!("Le nom du fichier de ce jeu ne contient pas son identifiant, comme [0100000000010000] : NX2 Launcher ne sait pas où l'émulateur range ses réglages."))
}

pub(crate) fn section_range(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let head = format!("[{section}]");
    let start = lines.iter().position(|l| l.trim() == head)?;
    let end = lines[start + 1..].iter().position(|l| l.starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    Some((start, end))
}

pub(crate) fn read(lines: &[String], section: &str, key: &str) -> Option<String> {
    let (start, end) = section_range(lines, section)?;
    let prefix = format!("{key}=");
    lines[start..end].iter().find_map(|l| l.strip_prefix(prefix.as_str()).map(|v| v.trim().to_string()))
}

/// Replaces the line that starts with `prefix`, or adds it at the top of the section.
pub(crate) fn put(lines: &mut Vec<String>, section: &str, prefix: &str, line: String) {
    let start = match section_range(lines, section) {
        Some(r) => r.0,
        None => {
            if lines.last().is_some_and(|l| !l.is_empty()) { lines.push(String::new()); }
            lines.push(format!("[{section}]"));
            lines.len() - 1
        }
    };
    let end = section_range(lines, section).map_or(lines.len(), |r| r.1);
    match lines[start..end].iter().position(|l| l.starts_with(prefix)) {
        Some(i) => lines[start + i] = line,
        None => lines.insert(start + 1, line),
    }
}

pub(crate) fn drop_line(lines: &mut Vec<String>, section: &str, prefix: &str) {
    if let Some((start, end)) = section_range(lines, section) {
        if let Some(i) = lines[start..end].iter().position(|l| l.starts_with(prefix)) { lines.remove(start + i); }
    }
}

/// Keeps the lines of one setting together, in the emulators' own order: use_global (games
/// only), then default, then the value.
fn group(lines: &mut Vec<String>, section: &str, key: &str) {
    let Some((start, end)) = section_range(lines, section) else { return };
    let mine = |l: &String| l.starts_with(&format!("{key}=")) || l.starts_with(&format!("{key}\\"));
    let Some(first) = lines[start..end].iter().position(mine).map(|i| start + i) else { return };
    let mut own: Vec<String> = vec![];
    let mut i = start;
    let mut end = end;
    while i < end { if mine(&lines[i]) { own.push(lines.remove(i)); end -= 1; } else { i += 1; } }
    let rank = |l: &String| if l.contains("\\use_global=") { 0 } else if l.contains("\\default=") { 1 } else { 2 };
    own.sort_by_key(rank);
    for (n, l) in own.into_iter().enumerate() { lines.insert(first + n, l); }
}

/// An emulator ignores a value still marked as default, so both lines are written.
fn write(lines: &mut Vec<String>, section: &str, key: &str, value: &str, game: bool) {
    put(lines, section, &format!("{key}="), format!("{key}={value}"));
    put(lines, section, &format!("{key}\\default="), format!("{key}\\default=false"));
    if game { put(lines, section, &format!("{key}\\use_global="), format!("{key}\\use_global=false")); }
    group(lines, section, key);
}

/// What an emulator writes for a game setting that follows the common one.
fn follow_common(lines: &mut Vec<String>, section: &str, key: &str) {
    drop_line(lines, section, &format!("{key}="));
    drop_line(lines, section, &format!("{key}\\default="));
    put(lines, section, &format!("{key}\\use_global="), format!("{key}\\use_global=true"));
    group(lines, section, key);
}

/// What NX2 Launcher sets in the emulators' file while none runs, as they read it only when they start:
/// - their own update prompt is off, since NX2 Launcher updates them itself and checks each version it keeps;
/// - on Windows, their "Configure Current Game" shortcut is F13, a key no keyboard has: NX2 Launcher presses it
///   to apply a setting during a game (see live.rs).
pub fn prepare_emulators() -> Result<()> {
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    let updates = read(&lines, "UI", "check_for_updates").as_deref() != Some("false");
    let shortcut = cfg!(windows) && !live_shortcut_in(&lines);
    if !updates && !shortcut { return Ok(()); }
    if updates { write(&mut lines, "UI", "check_for_updates", "false", false); }
    if shortcut {
        put(&mut lines, "UI", &format!("{LIVE_SHORTCUT}="), format!("{LIVE_SHORTCUT}={LIVE_KEY}"));
        put(&mut lines, "UI", &format!("{LIVE_SHORTCUT}\\default="), format!("{LIVE_SHORTCUT}\\default=false"));
    }
    save(&path, &lines, crlf)
}

const LIVE_SHORTCUT: &str = "Shortcuts\\Main%20Window\\Configure%20Current%20Game\\KeySeq";
const LIVE_KEY: &str = "F13";

fn live_shortcut_in(lines: &[String]) -> bool {
    read(lines, "UI", LIVE_SHORTCUT).is_some_and(|v| v.trim_matches('"') == LIVE_KEY)
}

/// Whether the running emulators have the shortcut: they read it from their own file when they started,
/// and NX2 Launcher writes it there only while none runs.
pub(crate) fn live_shortcut_ready() -> bool {
    live_shortcut_in(&load(&real_common_path()).0)
}

/// The settings of the page that an emulator takes during a game. The others (resolution, graphics API and card,
/// language, region, full screen mode) are read when a game starts.
pub(crate) const LIVE: [&str; 3] = ["speed", "volume", "use_docked_mode"];

/// During a game, an emulator reads again only that game's own file, from its "Configure Current Game" window.
/// So the common settings changed since it started, that this game follows, are written in it as the game's own
/// for that moment. Returns what the file held before, to put back once the emulator has read it (None: nothing
/// was added); the common file gets them when the emulator closes, as before.
pub(crate) fn game_file_for_live(id: &str) -> Result<Option<Option<Vec<u8>>>> {
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let path = game_ini_of(id);
    let before = fs::read(&path).ok();
    let (mut lines, crlf) = load(&path);
    let (started, _) = load(&real_common_path());
    let (wanted, _) = load(&common_path());
    let mut added = false;
    for key in LIVE {
        let v = value(&wanted, key);
        if !is_own(&lines, key) && v != value(&started, key) {
            apply(&mut lines, key, v, true);
            added = true;
        }
    }
    if !added { return Ok(None); }
    save(&path, &lines, crlf)?;
    Ok(Some(before))
}

/// Puts a game's own file back as it was before game_file_for_live.
pub(crate) fn put_back_game_file(id: &str, before: Option<Vec<u8>>) -> Result<()> {
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let path = game_ini_of(id);
    match before {
        Some(bytes) => fs::write(&path, bytes)?,
        None => { let _ = fs::remove_file(&path); }
    }
    Ok(())
}

/// A game's own settings file, by program id.
pub(crate) fn game_ini_of(id: &str) -> PathBuf {
    data_path().join("config").join("custom").join(format!("{id}.ini"))
}

pub(crate) fn load(path: &Path) -> (Vec<String>, bool) {
    let text = fs::read_to_string(path).unwrap_or_default();
    let crlf = text.contains("\r\n") || !path.exists();
    (text.trim_start_matches('\u{feff}').lines().map(str::to_string).collect(), crlf)
}

pub(crate) fn save(path: &Path, lines: &[String], crlf: bool) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or_else(|| anyhow!("config"))?)?;
    if path.is_file() { fs::copy(path, path.with_extension("ini.forkbay-backup"))?; }
    let eol = if crlf { "\r\n" } else { "\n" };
    let tmp = path.with_extension("ini.tmp");
    fs::write(&tmp, lines.join(eol) + eol)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn value(lines: &[String], key: &str) -> u32 {
    let num = |s: &str, k: &str| read(lines, s, k).and_then(|v| v.parse::<u32>().ok());
    match key {
        "speed" => {
            let limited = read(lines, "Core", "use_speed_limit").is_none_or(|v| v != "false");
            if limited { num("Core", "speed_limit").unwrap_or(100) } else { 0 }
        }
        "volume" => num("Audio", "volume").unwrap_or(100),
        _ => FIELDS.iter().find(|f| f.0 == key).map_or(0, |f| num(f.1, f.0).unwrap_or(f.3)),
    }
}

/// A game setting is its own when any of its lines no longer follows the common file.
fn is_own(lines: &[String], key: &str) -> bool {
    lines_of(key).iter().any(|(s, k)| read(lines, s, &format!("{k}\\use_global")).is_some_and(|v| v == "false"))
}

fn gpus() -> Vec<String> {
    crate::sys::gpu_name().split("; ").filter(|g| !g.is_empty()).map(|g| g.split(" (driver").next().unwrap_or(g).to_string()).collect()
}

/// Current common values, for the page. A value missing from the file is the emulators' default.
pub fn get() -> Value {
    let (lines, _) = load(&common_path());
    let mut out: serde_json::Map<String, Value> = keys().into_iter().map(|k| (k.to_string(), json!(value(&lines, k)))).collect();
    out.insert("gpus".to_string(), json!(gpus()));
    Value::Object(out)
}

/// One game: its own values, null where it follows the common ones, and how many other
/// settings it changes in the emulator.
pub fn game_get(file: &str) -> Result<Value> {
    let path = game_path(file)?;
    let (lines, _) = load(&path);
    let own: serde_json::Map<String, Value> = keys().into_iter().map(|k| (k.to_string(), if is_own(&lines, k) { json!(value(&lines, k)) } else { Value::Null })).collect();
    let list: Vec<Value> = other_list(&lines).into_iter().map(|(section, key)| {
        let name = OTHER_NAMES.iter().find(|n| n.0 == key).map(|n| n.1);
        json!({ "section": section, "key": key, "name": name })
    }).collect();
    Ok(json!({ "own": own, "common": get(), "others": list.len(), "otherList": list }))
}

/// Names of settings the page cannot change, for the list of those a game has of its own.
const OTHER_NAMES: [(&str, &str); 16] = [
    ("memory_layout_mode", "Mémoire émulée"),
    ("use_multi_core", "Processeur multicœur"),
    ("cpu_accuracy", "Précision du processeur"),
    ("gpu_accuracy", "Précision du GPU"),
    ("aspect_ratio", "Format d'image"),
    ("scaling_filter", "Filtre de mise à l'échelle"),
    ("anti_aliasing", "Anticrénelage"),
    ("max_anisotropy", "Filtrage anisotrope"),
    ("shader_backend", "Moteur de shaders"),
    ("use_asynchronous_shaders", "Shaders asynchrones"),
    ("use_disk_shader_cache", "Cache de shaders sur le disque"),
    ("astc_decode_mode", "Décodage ASTC"),
    ("nvdec_emulation", "Décodage vidéo"),
    ("fsr_sharpening_slider", "Netteté FSR"),
    ("output_engine", "Sortie audio"),
    ("sync_core_speed", "Vitesse des cœurs synchronisée"),
];

/// Settings changed for a game in the emulator that the page does not show: section and name.
fn other_list(lines: &[String]) -> Vec<(String, String)> {
    let shown: Vec<&str> = keys().iter().flat_map(|k| lines_of(k)).map(|(_, k)| k).collect();
    let mut section = String::new();
    let mut out = vec![];
    for l in lines {
        if let Some(s) = l.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = s.to_string();
        } else if let Some(k) = l.trim().strip_suffix("\\use_global=false") {
            if !shown.contains(&k) { out.push((section.clone(), k.to_string())); }
        }
    }
    out
}

fn others(lines: &[String]) -> usize {
    other_list(lines).len()
}

/// Makes a game follow the common value again for a setting the page does not show.
pub fn game_other_common(file: &str, section: &str, key: &str) -> Result<()> {
    // A game's own file is written by the emulators only from their own game properties window: it can change while they run.
    let path = game_path(file)?;
    let (mut lines, crlf) = load(&path);
    if !other_list(&lines).iter().any(|(s, k)| s == section && k == key) { bail!("Réglage inconnu."); }
    follow_common(&mut lines, section, key);
    save(&path, &lines, crlf)
}

/// Games with settings of their own, for the list on the emulators page.
pub fn custom_games() -> Value {
    let list: Vec<Value> = rom_files().iter().filter_map(|f| {
        let path = game_ini(f).filter(|p| p.is_file())?;
        let (lines, _) = load(&path);
        let own: Vec<&str> = keys().into_iter().filter(|k| is_own(&lines, k)).collect();
        let others = others(&lines);
        (!own.is_empty() || others > 0).then(|| json!({ "file": f, "name": display_name(f), "own": own, "others": others }))
    }).collect();
    json!(list)
}

pub fn has_own(file: &str) -> bool {
    let Some(path) = game_ini(file) else { return false };
    // A controller profile given to one of its players counts too.
    let profile = |lines: &[String]| lines.iter().any(|l| l.contains("_profile_name=") && !l.trim_end().ends_with('='));
    path.is_file() && { let (lines, _) = load(&path); keys().iter().any(|k| is_own(&lines, k)) || others(&lines) > 0 || profile(&lines) }
}

fn check(key: &str, value: u32) -> Result<()> {
    let ok = match key {
        "speed" => value <= 9999,
        "volume" => value <= 100,
        _ => FIELDS.iter().find(|f| f.0 == key).ok_or_else(|| anyhow!("Réglage inconnu."))?.2.contains(&value),
    };
    if ok { Ok(()) } else { bail!("Valeur refusée.") }
}

fn apply(lines: &mut Vec<String>, key: &str, value: u32, game: bool) {
    match key {
        "speed" => {
            write(lines, "Core", "use_speed_limit", if value == 0 { "false" } else { "true" }, game);
            // A game without limit still gets its own limit line, kept at the usual value.
            if value > 0 || game { write(lines, "Core", "speed_limit", &(if value == 0 { 100 } else { value }).to_string(), game); }
        }
        _ => for (s, k) in lines_of(key) { write(lines, s, k, &value.to_string(), game) },
    }
}

/// Docked (TV) or handheld, read and written like the page's own setting.
pub(crate) fn docked(lines: &[String]) -> bool {
    value(lines, "use_docked_mode") == 1
}

pub(crate) fn set_docked(lines: &mut Vec<String>, docked: bool) {
    apply(lines, "use_docked_mode", u32::from(docked), false);
}

/// Where the emulators keep a game's own settings.
pub(crate) fn game_ini_path(file: &str) -> Result<PathBuf> {
    game_path(file)
}

/// The emulators rewrite their common settings file when a game starts and when they close, from what they hold:
/// a change made while one runs waits in a copy, and is applied once they are all closed.
pub(crate) fn prepare_common_edit() -> Result<()> {
    if !crate::emu::any_running() { return Ok(()); }
    let dir = pending_dir();
    if !dir.join("qt-config.ini").exists() {
        fs::create_dir_all(&dir)?;
        let real = real_common_path();
        let text = fs::read(&real).unwrap_or_default();
        fs::write(dir.join("base.ini"), &text)?;
        fs::write(dir.join("qt-config.ini"), &text)?;
    }
    // A setting the game takes at once says so itself (live.rs), just after this one.
    crate::emit_status("Réglage gardé : il sera appliqué à la fermeture de l'émulateur, pour sa prochaine partie.");
    Ok(())
}

/// Whether a change waits for the emulators to close.
pub fn pending() -> bool {
    pending_dir().join("qt-config.ini").exists()
}

/// Changes one common setting.
pub fn set(key: &str, value: u64) -> Result<()> {
    prepare_common_edit()?;
    let value = u32::try_from(value).map_err(|_| anyhow!("Valeur refusée."))?;
    check(key, value)?;
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    apply(&mut lines, key, value, false);
    save(&path, &lines, crlf)?;
    if LIVE.contains(&key) { crate::live::request(None); }
    Ok(())
}

/// Gives a game its own value, or makes it follow the common one again (None).
pub fn game_set(file: &str, key: &str, value: Option<u64>) -> Result<()> {
    // A game's own file is written by the emulators only from their own game properties window: it can change while they run.
    let path = game_path(file)?;
    let (mut lines, crlf) = load(&path);
    match value {
        Some(v) => {
            let v = u32::try_from(v).map_err(|_| anyhow!("Valeur refusée."))?;
            check(key, v)?;
            apply(&mut lines, key, v, true);
        }
        None => {
            check(key, 0).or_else(|_| check(key, 1))?;
            for (s, k) in lines_of(key) { follow_common(&mut lines, s, k); }
        }
    }
    save(&path, &lines, crlf)?;
    if LIVE.contains(&key) { crate::live::request(Some(file)); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[String]) -> String { lines.join("\n") }

    #[test]
    fn game_value_then_common_again() {
        let mut lines: Vec<String> = "[System]\nuse_docked_mode\\use_global=true\nlanguage_index\\use_global=true".lines().map(str::to_string).collect();
        apply(&mut lines, "use_docked_mode", 1, true);
        assert!(text(&lines).contains("use_docked_mode\\use_global=false\nuse_docked_mode\\default=false\nuse_docked_mode=1"));
        assert!(is_own(&lines, "use_docked_mode"));
        assert_eq!(value(&lines, "use_docked_mode"), 1);
        for (s, k) in lines_of("use_docked_mode") { follow_common(&mut lines, s, k); }
        assert_eq!(text(&lines), "[System]\nuse_docked_mode\\use_global=true\nlanguage_index\\use_global=true");
    }

    #[test]
    fn common_file_has_no_use_global() {
        let mut lines: Vec<String> = vec![];
        apply(&mut lines, "speed", 0, false);
        assert_eq!(text(&lines), "[Core]\nuse_speed_limit\\default=false\nuse_speed_limit=false");
        assert_eq!(value(&lines, "speed"), 0);
    }

    #[test]
    fn other_settings_are_counted() {
        let lines: Vec<String> = "[Core]\nmemory_layout_mode\\use_global=false\nmemory_layout_mode=2\n[System]\nuse_docked_mode\\use_global=false".lines().map(str::to_string).collect();
        assert_eq!(others(&lines), 1);
        assert_eq!(other_list(&lines), vec![("Core".to_string(), "memory_layout_mode".to_string())]);
    }

    #[test]
    fn other_setting_follows_common_again() {
        let mut lines: Vec<String> = "[Core]\nuse_multi_core\\use_global=true\nmemory_layout_mode\\use_global=false\nmemory_layout_mode\\default=true\nmemory_layout_mode=2\nuse_speed_limit\\use_global=true".lines().map(str::to_string).collect();
        follow_common(&mut lines, "Core", "memory_layout_mode");
        assert_eq!(text(&lines), "[Core]\nuse_multi_core\\use_global=true\nmemory_layout_mode\\use_global=true\nuse_speed_limit\\use_global=true");
        assert_eq!(others(&lines), 0);
    }
}
