// Settings every emulator reads from the shared qt-config.ini. The values were checked
// against the source of each emulator in emulators.json: their tables are identical.
use crate::core::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

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

fn path() -> PathBuf {
    data_path().join("config").join("qt-config.ini")
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

/// An emulator ignores a value still marked as default, so both lines are written.
fn write(lines: &mut Vec<String>, section: &str, key: &str, value: &str) {
    let (start, _) = match section_range(lines, section) {
        Some(r) => r,
        None => {
            if lines.last().is_some_and(|l| !l.is_empty()) { lines.push(String::new()); }
            lines.push(format!("[{section}]"));
            (lines.len() - 1, lines.len())
        }
    };
    let end = section_range(lines, section).map_or(lines.len(), |r| r.1);
    let (default_prefix, value_prefix) = (format!("{key}\\default="), format!("{key}="));
    let has_default = lines[start..end].iter().position(|l| l.starts_with(&default_prefix)).map(|i| start + i);
    let has_value = lines[start..end].iter().position(|l| l.starts_with(&value_prefix)).map(|i| start + i);
    match has_value {
        Some(i) => lines[i] = format!("{key}={value}"),
        None => lines.insert(start + 1, format!("{key}={value}")),
    }
    match has_default {
        Some(i) => lines[i] = format!("{key}\\default=false"),
        // The value line just moved or was added: keep the emulators' own order.
        None => {
            let i = lines[start..].iter().position(|l| l.starts_with(&value_prefix)).map_or(start + 1, |i| start + i);
            lines.insert(i, format!("{key}\\default=false"));
        }
    }
}

fn load() -> (Vec<String>, bool) {
    let text = fs::read_to_string(path()).unwrap_or_default();
    let crlf = text.contains("\r\n");
    (text.lines().map(str::to_string).collect(), crlf)
}

/// Current values, for the page. A value missing from the file is the emulators' default.
pub fn get() -> Value {
    let (lines, _) = load();
    let mut out = serde_json::Map::new();
    for (key, section, _, default) in FIELDS {
        let v = read(&lines, section, key).and_then(|v| v.parse::<u32>().ok()).unwrap_or(default);
        out.insert(key.to_string(), json!(v));
    }
    let limited = read(&lines, "Core", "use_speed_limit").is_none_or(|v| v != "false");
    let speed = read(&lines, "Core", "speed_limit").and_then(|v| v.parse::<u32>().ok()).unwrap_or(100);
    out.insert("speed".to_string(), json!(if limited { speed } else { 0 }));
    out.insert("volume".to_string(), json!(read(&lines, "Audio", "volume").and_then(|v| v.parse::<u32>().ok()).unwrap_or(100)));
    out.insert("gpus".to_string(), json!(crate::report::gpu_name().split("; ").filter(|g| !g.is_empty()).map(|g| g.split(" (driver").next().unwrap_or(g).to_string()).collect::<Vec<_>>()));
    Value::Object(out)
}

/// Changes one setting. The emulators rewrite this file when they close, so they must not be open.
pub fn set(key: &str, value: u64) -> Result<()> {
    if crate::emu::any_running() { bail!("Ferme d'abord l'émulateur : il écraserait ce réglage en se fermant."); }
    let value = u32::try_from(value).map_err(|_| anyhow!("Valeur refusée."))?;
    let (mut lines, crlf) = load();
    match key {
        "speed" => {
            if value > 9999 { bail!("Valeur refusée."); }
            write(&mut lines, "Core", "use_speed_limit", if value == 0 { "false" } else { "true" });
            if value > 0 { write(&mut lines, "Core", "speed_limit", &value.to_string()); }
        }
        "volume" => {
            if value > 100 { bail!("Valeur refusée."); }
            write(&mut lines, "Audio", "volume", &value.to_string());
        }
        _ => {
            let (_, section, allowed, _) = FIELDS.iter().find(|f| f.0 == key).ok_or_else(|| anyhow!("Réglage inconnu."))?;
            if !allowed.contains(&value) { bail!("Valeur refusée."); }
            write(&mut lines, section, key, &value.to_string());
        }
    }
    let file = path();
    fs::create_dir_all(file.parent().ok_or_else(|| anyhow!("config"))?)?;
    if file.is_file() { fs::copy(&file, file.with_extension("ini.forkbay-backup"))?; }
    let eol = if crlf { "\r\n" } else { "\n" };
    let tmp = file.with_extension("ini.tmp");
    fs::write(&tmp, lines.join(eol) + eol)?;
    fs::rename(&tmp, &file)?;
    Ok(())
}
