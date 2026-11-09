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

fn common_path() -> PathBuf {
    data_path().join("config").join("qt-config.ini")
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
    game_ini(file).ok_or_else(|| anyhow!("Le nom du fichier de ce jeu ne contient pas son identifiant, comme [0100000000010000] : Forkbay ne sait pas où l'émulateur range ses réglages."))
}

fn section_range(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let head = format!("[{section}]");
    let start = lines.iter().position(|l| l.trim() == head)?;
    let end = lines[start + 1..].iter().position(|l| l.starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    Some((start, end))
}

fn read(lines: &[String], section: &str, key: &str) -> Option<String> {
    let (start, end) = section_range(lines, section)?;
    let prefix = format!("{key}=");
    lines[start..end].iter().find_map(|l| l.strip_prefix(prefix.as_str()).map(|v| v.trim().to_string()))
}

/// Replaces the line that starts with `prefix`, or adds it at the top of the section.
fn put(lines: &mut Vec<String>, section: &str, prefix: &str, line: String) {
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

fn drop_line(lines: &mut Vec<String>, section: &str, prefix: &str) {
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

/// Forkbay updates the emulators itself: their own update prompt is turned off before each start,
/// so that a version is never replaced without being checked and kept.
pub fn turn_off_emulator_updates() -> Result<()> {
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    if read(&lines, "UI", "check_for_updates").as_deref() == Some("false") {
        return Ok(());
    }
    write(&mut lines, "UI", "check_for_updates", "false", false);
    save(&path, &lines, crlf)
}

fn load(path: &Path) -> (Vec<String>, bool) {
    let text = fs::read_to_string(path).unwrap_or_default();
    let crlf = text.contains("\r\n") || !path.exists();
    (text.trim_start_matches('\u{feff}').lines().map(str::to_string).collect(), crlf)
}

fn save(path: &Path, lines: &[String], crlf: bool) -> Result<()> {
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
    Ok(json!({ "own": own, "common": get(), "others": others(&lines) }))
}

/// Settings changed for a game in the emulator that the page does not show.
fn others(lines: &[String]) -> usize {
    let shown: Vec<String> = keys().iter().flat_map(|k| lines_of(k)).map(|(_, k)| format!("{k}\\use_global=false")).collect();
    lines.iter().filter(|l| l.ends_with("\\use_global=false") && !shown.contains(l)).count()
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
    path.is_file() && { let (lines, _) = load(&path); keys().iter().any(|k| is_own(&lines, k)) || others(&lines) > 0 }
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

/// The emulators rewrite these files when they close, so they must not be open.
fn not_running() -> Result<()> {
    if crate::emu::any_running() { bail!("Ferme d'abord l'émulateur : il écraserait ce réglage en se fermant."); }
    Ok(())
}

/// Changes one common setting.
pub fn set(key: &str, value: u64) -> Result<()> {
    not_running()?;
    let value = u32::try_from(value).map_err(|_| anyhow!("Valeur refusée."))?;
    check(key, value)?;
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    apply(&mut lines, key, value, false);
    save(&path, &lines, crlf)
}

/// Gives a game its own value, or makes it follow the common one again (None).
pub fn game_set(file: &str, key: &str, value: Option<u64>) -> Result<()> {
    not_running()?;
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
    save(&path, &lines, crlf)
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
    }
}
