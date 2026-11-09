// Controllers across emulators.
// The emulators share one settings file, where each control names its controller by an SDL id.
// They look for that exact id, and the id holds the version the controller reports, which is not
// the same from one SDL build to another: an Xbox 360 controller is 0x0110 in one emulator and
// 0x0114 in the next. A controller set up in one emulator is then not found by the other.
// Forkbay notes the id each emulator writes when its controls are set, and puts that id back in
// the bindings before the emulator starts.
use crate::core::*;
use anyhow::Result;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Emulator, then controller without its version, then the id this emulator sees.
type Seen = BTreeMap<String, BTreeMap<String, String>>;

fn seen_file() -> PathBuf {
    manager_dir().join("controllers.json")
}

fn load_seen() -> Seen {
    fs::read_to_string(seen_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_seen(seen: &Seen) -> Result<()> {
    fs::create_dir_all(manager_dir())?;
    fs::write(seen_file(), serde_json::to_string_pretty(seen)?)?;
    Ok(())
}

fn id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("guid:([0-9a-f]{32})").unwrap())
}

/// The files where the emulators keep controls: the common settings, the input profiles and the
/// games' own settings.
fn files() -> Vec<PathBuf> {
    let cfg = data_path().join("config");
    let mut out = vec![cfg.join("qt-config.ini")];
    for dir in ["input", "custom"] {
        let mut found: Vec<PathBuf> = fs::read_dir(cfg.join(dir)).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ini")).collect()).unwrap_or_default();
        found.sort();
        out.extend(found);
    }
    out.into_iter().filter(|p| p.is_file()).collect()
}

/// A controller without the version it reports: what stays the same from one emulator to another.
/// SDL ids are 16 bytes: bus, name check (cleared by the emulators), vendor, product, version,
/// then the driver. The driver is kept: two drivers can number the buttons differently.
fn family(id: &str) -> String {
    format!("{}0000{}", &id[..24], &id[28..])
}

/// Each SDL control of a file, with the controller id it names.
fn controls(text: &str) -> BTreeMap<String, String> {
    text.lines().filter(|l| l.contains("engine:sdl")).filter_map(|l| {
        let (key, value) = l.split_once('=')?;
        Some((key.trim().to_string(), id_re().captures(value)?[1].to_string()))
    }).collect()
}

/// The controls of every file, read before an emulator starts.
pub struct Before(BTreeMap<String, String>);

fn all_controls() -> BTreeMap<String, String> {
    files().iter().flat_map(|f| {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        controls(&fs::read_to_string(f).unwrap_or_default()).into_iter().map(move |(k, id)| (format!("{name}|{k}"), id))
    }).collect()
}

pub fn before() -> Before {
    Before(all_controls())
}

/// The controls an emulator set while it ran. One that names another controller than before was
/// chosen in this emulator, so it names the controller as this emulator sees it. Controls only
/// written back as they were say nothing: the emulator keeps them even when it finds no controller.
fn learned(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    after.iter().filter(|(k, id)| before.get(*k) != Some(*id)).map(|(_, id)| (family(id), id.clone())).collect()
}

/// After an emulator closes.
pub fn learn(emu: &str, before: &Before) -> Result<()> {
    let found = learned(&before.0, &all_controls());
    if found.is_empty() { return Ok(()); }
    let mut seen = load_seen();
    let mine = seen.entry(emu.to_string()).or_default();
    let mut changed = false;
    for (f, id) in found {
        if mine.get(&f) != Some(&id) {
            crate::log::info(&format!("controller {id} seen by {emu}"));
            mine.insert(f, id);
            changed = true;
        }
    }
    if changed { save_seen(&seen)?; }
    Ok(())
}

/// The same text with each controller this emulator knows named as it sees it, and how many
/// controls changed.
fn adapted(text: &str, mine: &BTreeMap<String, String>) -> (String, usize) {
    let mut n = 0;
    let out: String = text.split_inclusive('\n').map(|l| {
        if !l.contains("engine:sdl") { return l.to_string(); }
        id_re().replace_all(l, |c: &regex::Captures| {
            match mine.get(&family(&c[1])) {
                Some(id) if id != &c[1] => { n += 1; format!("guid:{id}") }
                _ => c[0].to_string(),
            }
        }).into_owned()
    }).collect();
    (out, n)
}

/// Before an emulator starts: each controller it has seen gets back the id it knows it by.
pub fn adapt(emu: &str) -> Result<usize> {
    let seen = load_seen();
    let Some(mine) = seen.get(emu) else { return Ok(0) };
    let mut total = 0;
    for f in files() {
        let text = fs::read_to_string(&f)?;
        let (out, n) = adapted(&text, mine);
        if n == 0 { continue; }
        let tmp = f.with_extension("ini.tmp");
        fs::write(&tmp, out)?;
        fs::rename(&tmp, &f)?;
        total += n;
    }
    if total > 0 { crate::log::info(&format!("controls named for {emu}: {total}")); }
    Ok(total)
}

/// A name for a controller, from its vendor and product.
fn name(id: &str) -> String {
    let word = |at: usize| u16::from_str_radix(&format!("{}{}", &id[at + 2..at + 4], &id[at..at + 2]), 16).unwrap_or(0);
    let (vendor, product) = (word(8), word(16));
    let known = match (vendor, product) {
        (0x045e, 0x028e | 0x028f | 0x0719) => "Manette Xbox 360",
        (0x045e, _) => "Manette Xbox",
        (0x054c, 0x05c4 | 0x09cc) => "Manette DualShock 4",
        (0x054c, 0x0ce6 | 0x0df2) => "Manette DualSense",
        (0x057e, 0x2009) => "Manette Pro Switch",
        (0x057e, 0x2006..=0x2008) => "Joy-Con",
        (0x2dc8, _) => "Manette 8BitDo",
        _ => "",
    };
    if known.is_empty() { format!("Manette {vendor:04x}:{product:04x}") } else { known.to_string() }
}

/// For the page: the controllers the controls use, the players they control, and which installed
/// emulators already know them.
pub fn status() -> Value {
    let seen = load_seen();
    let text = fs::read_to_string(data_path().join("config").join("qt-config.ini")).unwrap_or_default();
    let player = Regex::new(r"^player_(\d+)_").unwrap();
    let mut pads: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for (key, id) in controls(&text) {
        if let Some(c) = player.captures(&key) {
            pads.entry(family(&id)).or_default().insert(c[1].parse::<u32>().unwrap_or(0) + 1);
        }
    }
    let installed: Vec<String> = crate::emu::list().into_iter().map(|e| e.name).filter(|n| crate::emu::find_exe(&root().join(n)).is_some()).collect();
    let list: Vec<Value> = pads.iter().map(|(f, players)| {
        let (known, unknown): (Vec<&String>, Vec<&String>) = installed.iter().partition(|e| seen.get(*e).is_some_and(|m| m.contains_key(f)));
        json!({ "name": name(f), "players": players, "known": known, "unknown": unknown })
    }).collect();
    json!({ "controllers": list, "emulators": installed })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "030000005e0400008e02000010017801";
    const B: &str = "030000005e0400008e02000014017801";

    #[test]
    fn same_controller_whatever_its_version() {
        assert_eq!(family(A), family(B));
        assert_ne!(family(A), family("030000005e0400008e02000014016801"));
        assert_eq!(name(A), "Manette Xbox 360");
        assert_eq!(name("03000000341200007856000000000000"), "Manette 1234:5678");
    }

    #[test]
    fn only_controls_set_in_the_emulator_teach_its_id() {
        let before = controls(&format!("[Controls]\nplayer_0_button_a=\"engine:sdl,pad:0,button:0,guid:{A},port:0\"\nplayer_0_lstick=\"engine:sdl,axis_x:0,guid:{A},port:0\""));
        // Written back as they were, in another order: nothing learned.
        let same = controls(&format!("[Controls]\nplayer_0_button_a=\"engine:sdl,guid:{A},pad:0,button:0,port:0\"\nplayer_0_lstick=\"engine:sdl,axis_x:0,guid:{A},port:0\""));
        assert!(learned(&before, &same).is_empty());
        let set = controls(&format!("[Controls]\nplayer_0_button_a=\"engine:sdl,pad:0,button:0,guid:{B},port:0\"\nplayer_0_lstick=\"engine:sdl,axis_x:0,guid:{A},port:0\""));
        assert_eq!(learned(&before, &set), BTreeMap::from([(family(B), B.to_string())]));
    }

    #[test]
    fn controls_get_the_id_the_emulator_knows() {
        let mine = BTreeMap::from([(family(B), B.to_string())]);
        let text = format!("[Controls]\r\nplayer_0_button_a=\"engine:sdl,pad:0,button:0,guid:{A},port:0\"\r\nplayer_0_button_b=\"engine:keyboard,code:67\"\r\nplayer_1_button_a=\"engine:sdl,button:0,guid:030000004c050000cc09000000006800,port:0\"\r\n");
        let (out, n) = adapted(&text, &mine);
        assert_eq!(n, 1);
        assert_eq!(out, text.replace(A, B));
        assert_eq!(adapted(&out, &mine).1, 0);
    }
}
