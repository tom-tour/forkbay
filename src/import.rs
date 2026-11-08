// Finds emulator data already on this PC and imports only what the user ticks.
// The source is never modified, and files already present are never overwritten.
use crate::core::*;
use crate::files::*;
use crate::setup::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const NAMES: [&str; 9] = ["eden", "eglantine", "lichii", "orchid", "yuzu", "sudachi", "citron", "suyu", "torzu"];

struct Cat {
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    dirs: &'static [&'static str],
    sections: &'static [&'static str],
    on: bool,
}

const CATS: [Cat; 12] = [
    Cat { id: "keys", label: "Clés", hint: "prod.keys et title.keys", dirs: &["keys"], sections: &[], on: true },
    Cat { id: "firmware", label: "Firmware", hint: "Logiciel système de la console", dirs: &["nand/system/Contents/registered"], sections: &[], on: true },
    Cat { id: "saves", label: "Sauvegardes", hint: "Ta progression dans tes jeux", dirs: &["nand/user/save"], sections: &[], on: true },
    Cat { id: "profiles", label: "Profils", hint: "Comptes utilisateur et avatars", dirs: &["nand/system/save/8000000000000010"], sections: &[], on: true },
    Cat { id: "controls", label: "Manettes", hint: "Boutons et ports", dirs: &[], sections: &["Controls"], on: true },
    Cat { id: "network", label: "Réseau", hint: "Réseau, compte en ligne, multijoueur", dirs: &[], sections: &["Network", "WebService", "Multiplayer"], on: false },
    Cat { id: "graphics", label: "Graphismes et audio", hint: "Rendu, processeur, son", dirs: &[], sections: &["Renderer", "Core", "Cpu", "Audio"], on: false },
    Cat { id: "gamecfg", label: "Réglages par jeu", hint: "Fichiers config/custom", dirs: &["config/custom"], sections: &[], on: false },
    Cat { id: "mods", label: "Mods", hint: "Dossier load", dirs: &["load"], sections: &[], on: false },
    Cat { id: "shaders", label: "Cache de shaders", hint: "Évite certaines saccades. Seulement pour l'émulateur d'où il vient", dirs: &["cache/shader", "shader"], sections: &[], on: false },
    Cat { id: "shots", label: "Captures d'écran", hint: "Dossier screenshots", dirs: &["screenshots"], sections: &[], on: false },
    Cat { id: "sdmc", label: "Carte SD virtuelle", hint: "Dossier sdmc", dirs: &["sdmc"], sections: &[], on: false },
];

fn config_file(src: &Path) -> Option<PathBuf> {
    [src.join("config/qt-config.ini"), config_side(src).join("qt-config.ini")].into_iter().find(|p| p.is_file())
}

// On Linux, the configuration lives in ~/.config/<name> and the data in ~/.local/share/<name>.
fn config_side(src: &Path) -> PathBuf {
    let name = src.file_name().unwrap_or_default();
    home().join(".config").join(name)
}

fn section_lines(text: &str, name: &str) -> Option<String> {
    let head = format!("[{name}]");
    let mut out = String::new();
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if inside { break; }
            inside = t == head;
        }
        if inside { out.push_str(line); }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn category_info(src: &Path, c: &Cat) -> Option<Value> {
    let mut files = 0;
    let mut size = 0;
    for d in c.dirs {
        let mut dir = src.join(d);
        if !dir.exists() && d.starts_with("config/") {
            dir = config_side(src).join(d.trim_start_matches("config/"));
        }
        let (f, s) = tree_stats(&dir);
        files += f;
        size += s;
    }
    let mut has_sections = false;
    if !c.sections.is_empty() {
        if let Some(text) = config_file(src).and_then(|p| fs::read_to_string(p).ok()) {
            has_sections = c.sections.iter().any(|s| section_lines(&text, s).is_some());
        }
    }
    if files == 0 && !has_sections {
        return None;
    }
    use crate::i18n::t;
    let detail = if has_sections { t("dans les réglages") }
        else if size > 1_048_576 { t(&format!("{} fichier(s), {} Mo", files, size / 1_048_576)) }
        else { t(&format!("{files} fichier(s)")) };
    let (label, hint) = crate::i18n::category(c.label, c.hint);
    Some(json!({ "id": c.id, "label": label, "hint": hint, "detail": detail, "on": c.on }))
}

/// The emulator of this list that a data folder belongs to, from the folder's name or its parent's
/// (a portable "user" folder). Shader caches only go to that emulator: another fork could get a black screen.
fn origin_emulator(path: &Path, emulators: &[String]) -> Option<String> {
    [path.file_name(), path.parent().and_then(Path::file_name)].into_iter().flatten()
        .find_map(|n| emulators.iter().find(|e| e.eq_ignore_ascii_case(&n.to_string_lossy())).cloned())
}

fn emulator_names() -> Vec<String> {
    crate::emu::list().into_iter().map(|e| e.name).collect()
}

fn describe(path: &Path, name: &str) -> Option<Value> {
    let origin = origin_emulator(path, &emulator_names());
    let cats: Vec<Value> = CATS.iter().filter(|c| c.id != "shaders" || origin.is_some()).filter_map(|c| category_info(path, c)).collect();
    if cats.is_empty() {
        return None;
    }
    Some(json!({ "name": name, "path": path.to_string_lossy(), "categories": cats }))
}

pub fn sources() -> Value {
    let mut bases: Vec<PathBuf> = vec![];
    if let Ok(b) = std::env::var("FORKBAY_SEARCH") {
        bases.push(b.into());
    } else if cfg!(windows) {
        for v in ["APPDATA", "LOCALAPPDATA"] {
            if let Some(p) = std::env::var_os(v) { bases.push(p.into()); }
        }
    } else {
        bases.push(home().join(".local/share"));
        for id in ["org.yuzu_emu.yuzu", "dev.eden_emu.eden"] {
            bases.push(home().join(".var/app").join(id).join("data"));
        }
    }
    let current = fs::canonicalize(data_path()).ok();
    let mut found = vec![];
    for b in &bases {
        for n in NAMES {
            let p = b.join(n);
            if p.is_dir() && fs::canonicalize(&p).ok() != current {
                if let Some(d) = describe(&p, n) { found.push(d); }
            }
        }
    }
    json!(found)
}

pub fn inspect(path: &Path) -> Result<Value> {
    if !path.is_dir() {
        bail!("Dossier introuvable.");
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    describe(path, &name).ok_or_else(|| anyhow::anyhow!("Aucune donnée d'émulateur reconnue dans ce dossier."))
}

/// Replaces only the ticked sections of the current configuration, after a backup.
fn merge_sections(src_text: &str, target: &Path, names: &[&str]) -> Result<usize> {
    let mut text = fs::read_to_string(target).unwrap_or_default();
    let mut n = 0;
    for name in names {
        let Some(block) = section_lines(src_text, name) else { continue };
        if let Some(old) = section_lines(&text, name) {
            text = text.replacen(&old, &block, 1);
        } else {
            if !text.is_empty() && !text.ends_with('\n') { text.push('\n'); }
            text.push_str(&block);
        }
        n += 1;
    }
    if n > 0 {
        if target.is_file() {
            let backup = target.with_extension("ini.before-import");
            if !backup.exists() { fs::copy(target, backup)?; }
        }
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(target, text)?;
    }
    Ok(n)
}

pub fn import(source: &Path, wanted: &[String]) -> Result<String> {
    if crate::emu::any_running() {
        bail!("Ferme les émulateurs avant d'importer.");
    }
    if !source.is_dir() {
        bail!("Dossier introuvable : {}", source.display());
    }
    let data = repair_setup()?;
    let origin = origin_emulator(source, &emulator_names());
    let mut done = vec![];
    for c in CATS.iter().filter(|c| wanted.iter().any(|w| w == c.id)) {
        if c.id == "shaders" && origin.is_none() { continue; }
        let mut copied = 0;
        for d in c.dirs {
            let mut dir = source.join(d);
            if !dir.exists() && d.starts_with("config/") {
                dir = config_side(source).join(d.trim_start_matches("config/"));
            }
            // Shaders go to the cache of the emulator they came from, never to the shared folder.
            let target = match (&origin, c.id) {
                (Some(e), "shaders") => data.join("cache").join("shaders").join(e),
                _ => data.join(d),
            };
            copied += copy_tree(&dir, &target, false)?;
        }
        let mut merged = 0;
        if !c.sections.is_empty() {
            if let Some(text) = config_file(source).and_then(|p| fs::read_to_string(p).ok()) {
                merged = merge_sections(&text, &data.join("config/qt-config.ini"), c.sections)?;
            }
        }
        if copied > 0 { done.push(format!("{} ({copied})", c.label.to_lowercase())); } else if merged > 0 { done.push(c.label.to_lowercase()); }
    }
    set_config_paths(&data)?;
    if done.is_empty() {
        return Ok("Rien de nouveau à importer : ce qui existe déjà n'est jamais écrasé.".into());
    }
    Ok(format!("Importé : {}.", done.join(", ")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn shaders_only_go_to_the_emulator_they_came_from() {
        use std::path::Path;
        let emus = ["Lichii".to_string(), "Eglantine".to_string()];
        assert_eq!(super::origin_emulator(Path::new("/home/a/.local/share/eglantine"), &emus).as_deref(), Some("Eglantine"));
        assert_eq!(super::origin_emulator(Path::new("C:/Emus/Lichii/user"), &emus).as_deref(), Some("Lichii"));
        assert_eq!(super::origin_emulator(Path::new("C:/Users/x/AppData/Roaming/eden"), &emus), None);
    }

    #[test]
    fn one_section_of_an_ini_file() {
        let text = "[Controls]\nvibration=1\n[Network]\nnet=2\n";
        assert_eq!(super::section_lines(text, "Network").as_deref(), Some("[Network]\nnet=2\n"));
        assert_eq!(super::section_lines(text, "Controls").as_deref(), Some("[Controls]\nvibration=1\n"));
        assert_eq!(super::section_lines(text, "Audio"), None);
    }
}
