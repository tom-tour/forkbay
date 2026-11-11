// The emulators' shortcuts, shared by all of them like the controls: in config/qt-config.ini,
// [UI] Shortcuts\Main%20Window\<name>\KeySeq for the keyboard and \Controller_KeySeq for a
// combination on player 1's controller (QtConfig::SaveShortcutValues, ControllerShortcut::SetKey).

use crate::config::{load, not_running, put, read, save};
use crate::controls::{common_path, quote};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

const SECTION: &str = "UI";

/// The shortcuts that matter during a game, with the emulators' defaults (UISettings::default_hotkeys).
/// Some have another name in some emulators: each name is written.
const LIST: [(&[&str], &str, &str); 18] = [
    (&["Continue/Pause Emulation"], "F4", "Home+Plus"),
    (&["Change Docked Mode"], "F10", "Home+X"),
    (&["Audio Mute/Unmute"], "Ctrl+M", "Home+Dpad_Right"),
    (&["Audio Volume Down"], "-", "Home+Dpad_Down"),
    (&["Audio Volume Up"], "=", "Home+Dpad_Up"),
    (&["Toggle Framerate Limit"], "Ctrl+U", "Home+Y"),
    (&["Toggle Turbo Speed"], "Ctrl+Z", ""),
    (&["Toggle Slow Speed"], "Ctrl+X", ""),
    (&["Capture Screenshot"], "Ctrl+P", "Screenshot"),
    (&["Fullscreen"], "F11", "Home+B"),
    (&["Exit Fullscreen"], "Esc", ""),
    (&["Toggle Performance Overlay"], "Ctrl+V", ""),
    (&["Change Adapting Filter"], "F8", "Home+L"),
    (&["Change GPU Mode"], "F9", "Home+R"),
    (&["Load/Remove Amiibo"], "F2", "Home+A"),
    (&["Restart Emulation"], "F6", "R+Plus+Minus"),
    (&["Stop Emulation"], "F5", "L+Plus+Minus"),
    (&["Exit Eglantine", "Exit Orchid"], "Ctrl+Q", "Home+Minus"),
];

/// The buttons a controller combination may use, as ControllerShortcut::SetKey reads them.
pub const BUTTONS: [&str; 18] = [
    "A", "B", "X", "Y", "L", "R", "ZL", "ZR", "Dpad_Left", "Dpad_Right", "Dpad_Up", "Dpad_Down", "Left_Stick", "Right_Stick", "Minus", "Plus",
    "Home", "Screenshot",
];

fn key(name: &str, field: &str) -> String {
    format!("Shortcuts\\Main%20Window\\{}\\{field}", name.replace(' ', "%20").replace('/', "\\"))
}

fn value(lines: &[String], names: &[&str], field: &str, default: &str) -> String {
    names.iter().find_map(|n| read(lines, SECTION, &key(n, field))).map(|v| v.trim().trim_matches('"').to_string()).unwrap_or_else(|| default.to_string())
}

fn write(lines: &mut Vec<String>, names: &[&str], field: &str, v: &str, default: &str) {
    for n in names {
        let k = key(n, field);
        put(lines, SECTION, &format!("{k}="), format!("{k}={}", quote(v)));
        put(lines, SECTION, &format!("{k}\\default="), format!("{k}\\default={}", v == default));
    }
}

pub fn get() -> Value {
    let (lines, _) = load(&common_path());
    let list: Vec<Value> = LIST
        .iter()
        .map(|(names, k, p)| json!({ "id": names[0], "key": value(&lines, names, "KeySeq", k), "pad": value(&lines, names, "Controller_KeySeq", p), "keyDefault": k, "padDefault": p }))
        .collect();
    json!({ "list": list, "buttons": BUTTONS, "running": crate::emu::any_running() })
}

/// A combination the emulators can read back: known buttons joined by "+", each once.
fn check_pad(v: &str) -> Result<()> {
    if v.is_empty() { return Ok(()); }
    let parts: Vec<&str> = v.split('+').collect();
    if parts.len() > 4 || parts.iter().any(|b| !BUTTONS.contains(b)) { bail!("Combinaison de boutons inconnue."); }
    if (1..parts.len()).any(|i| parts[..i].contains(&parts[i])) { bail!("Combinaison de boutons inconnue."); }
    Ok(())
}

fn check_key(v: &str) -> Result<()> {
    if v.len() > 40 || v.chars().any(|c| c.is_control()) { bail!("Touche inconnue."); }
    Ok(())
}

/// Sets the keyboard key ("key") or the controller combination ("pad") of one shortcut.
/// A key or combination already used by another shortcut is refused, as the emulators do.
pub fn set(id: &str, field: &str, v: &str) -> Result<()> {
    let v = v.trim();
    let (names, k, p) = LIST.iter().find(|(names, ..)| names[0] == id).ok_or_else(|| anyhow!("Raccourci inconnu."))?;
    let (ini_field, default) = match field {
        "key" => { check_key(v)?; ("KeySeq", *k) }
        "pad" => { check_pad(v)?; ("Controller_KeySeq", *p) }
        _ => bail!("Raccourci inconnu."),
    };
    not_running()?;
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    if !v.is_empty() {
        let same = |a: &str| if field == "pad" { sorted(a) == sorted(v) } else { a.eq_ignore_ascii_case(v) };
        if LIST.iter().any(|(other, k2, p2)| other[0] != id && same(&value(&lines, other, ini_field, if field == "key" { k2 } else { p2 }))) {
            bail!("Ce raccourci sert déjà à une autre action.");
        }
    }
    write(&mut lines, names, ini_field, v, default);
    save(&path, &lines, crlf)
}

fn sorted(v: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = v.split('+').filter(|s| !s.is_empty()).collect();
    parts.sort_unstable();
    parts
}

/// Puts every shortcut of the list back to the emulators' defaults.
pub fn reset() -> Result<()> {
    not_running()?;
    let path = common_path();
    let (mut lines, crlf) = load(&path);
    for (names, k, p) in LIST.iter() {
        write(&mut lines, names, "KeySeq", k, k);
        write(&mut lines, names, "Controller_KeySeq", p, p);
    }
    save(&path, &lines, crlf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_as_the_emulators_write_them() {
        assert_eq!(key("Audio Mute/Unmute", "KeySeq"), "Shortcuts\\Main%20Window\\Audio%20Mute\\Unmute\\KeySeq");
        assert_eq!(key("Fullscreen", "Controller_KeySeq"), "Shortcuts\\Main%20Window\\Fullscreen\\Controller_KeySeq");
    }

    #[test]
    fn values_read_and_written() {
        let mut lines = vec!["[UI]".to_string(), "Shortcuts\\Main%20Window\\Audio%20Volume%20Up\\KeySeq=\"=\"".to_string()];
        assert_eq!(value(&lines, &["Audio Volume Up"], "KeySeq", "x"), "=");
        assert_eq!(value(&lines, &["Fullscreen"], "KeySeq", "F11"), "F11");
        write(&mut lines, &["Exit Eglantine", "Exit Orchid"], "Controller_KeySeq", "Home+Minus", "Home+Minus");
        assert!(lines.contains(&"Shortcuts\\Main%20Window\\Exit%20Orchid\\Controller_KeySeq=Home+Minus".to_string()));
        assert!(lines.contains(&"Shortcuts\\Main%20Window\\Exit%20Eglantine\\Controller_KeySeq\\default=true".to_string()));
        write(&mut lines, &["Configure"], "KeySeq", "Ctrl+,", "x");
        assert!(lines.contains(&"Shortcuts\\Main%20Window\\Configure\\KeySeq=\"Ctrl+,\"".to_string()));
    }

    #[test]
    fn combinations_checked() {
        assert!(check_pad("Home+Plus").is_ok());
        assert!(check_pad("").is_ok());
        assert!(check_pad("Home+Start").is_err());
        assert!(check_pad("Home+Home").is_err());
        assert_eq!(sorted("Plus+Home"), sorted("Home+Plus"));
    }
}
