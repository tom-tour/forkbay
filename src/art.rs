// Game art: the user's own images in the Covers folder, SteamGridDB with their free key,
// or image sources they add themselves. The project ships no other source.
use crate::core::*;
use crate::games::*;
use crate::sys::*;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const EXTS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "ico"];
const KINDS: [&str; 5] = ["grid", "wide", "hero", "logo", "icon"];
const MAX_IMAGE: u64 = 25 * 1024 * 1024;
const MAX_ANSWER: u64 = 5 * 1024 * 1024;

pub fn cover_dir() -> PathBuf { root().join("Covers") }

pub fn sources_dir() -> PathBuf { manager_dir().join("cover-sources") }

fn safe(file: &str) -> String {
    display_name(file).chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).collect::<String>().trim().to_string()
}

/// An image dropped by hand ("Game Name-grid.png") works like a downloaded one.
pub fn find_art(file: &str, kind: &str) -> Option<PathBuf> {
    let base = format!("{}-{kind}", safe(file));
    EXTS.iter().map(|e| cover_dir().join(format!("{base}.{e}"))).find(|p| p.is_file())
}

pub fn cover_uri(file: &str) -> Option<String> {
    image_uri(file, "grid")
}

fn image_uri(file: &str, kind: &str) -> Option<String> {
    let p = find_art(file, kind)?;
    let bytes = fs::read(&p).ok()?;
    let mime = match p.extension()?.to_str()?.to_lowercase().as_str() { "png" => "image/png", "webp" => "image/webp", "ico" => "image/x-icon", _ => "image/jpeg" };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// The real type of a file, whatever its name says.
fn image_ext(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(&[0x89, b'P', b'N', b'G']) { return Some("png"); }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) { return Some("jpg"); }
    if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" { return Some("webp"); }
    if head.starts_with(&[0, 0, 1, 0]) { return Some("ico"); }
    None
}

/// Writes one image for a game, in place of any image of the same type.
fn store(bytes: &[u8], file: &str, kind: &str) -> Result<()> {
    let ext = image_ext(bytes).ok_or_else(|| anyhow!("Ce fichier n'est pas une image PNG, JPEG, WebP ou ICO."))?;
    fs::create_dir_all(cover_dir())?;
    let base = format!("{}-{kind}", safe(file));
    let tmp = cover_dir().join(format!("{base}.tmp"));
    fs::write(&tmp, bytes)?;
    for e in EXTS.iter().filter(|e| **e != ext) {
        let _ = fs::remove_file(cover_dir().join(format!("{base}.{e}")));
    }
    fs::rename(&tmp, cover_dir().join(format!("{base}.{ext}")))?;
    Ok(())
}

/// An image chosen by the user for one game.
pub fn set_image(file: &str, kind: &str, path: &Path) -> Result<()> {
    if !KINDS.contains(&kind) { bail!("Type d'image inconnu."); }
    if !rom_files().iter().any(|f| f == file) { bail!("Jeu introuvable : {file}"); }
    if fs::metadata(path)?.len() > MAX_IMAGE { bail!("Image trop lourde (25 Mo au maximum)."); }
    store(&fs::read(path)?, file, kind)
}

/// Every image of one game, for the page.
pub fn images(file: &str) -> Value {
    Value::Object(KINDS.iter().map(|k| (k.to_string(), image_uri(file, k).map_or(Value::Null, Value::String))).collect())
}

pub fn remove_image(file: &str, kind: &str) -> Result<()> {
    if !KINDS.contains(&kind) { bail!("Type d'image inconnu."); }
    if !rom_files().iter().any(|f| f == file) { bail!("Jeu introuvable : {file}"); }
    if let Some(p) = find_art(file, kind) { fs::remove_file(p)?; }
    Ok(())
}

/// What to download: chosen by the user, and remembered for next time.
pub struct Choice {
    pub kinds: Vec<String>,
    pub source: String,
    pub replace: bool,
    pub file: String,
}

fn needed(c: &Choice, file: &str, kind: &str, done: &[String]) -> bool {
    KINDS.contains(&kind) && c.kinds.iter().any(|k| k == kind) && !done.iter().any(|d| d == kind) && (c.replace || find_art(file, kind).is_none())
}

fn save_image(url: &str, file: &str, kind: &str) -> Result<()> {
    if !url.starts_with("https://") { return Err(anyhow!("Adresse refusée.")); }
    let bytes = crate::net::get_bytes(url, MAX_IMAGE)?;
    if bytes.len() as u64 > MAX_IMAGE { bail!("Image trop lourde (25 Mo au maximum)."); }
    store(&bytes, file, kind)
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase().chars().map(|c| match c { 'é' | 'è' | 'ê' => 'e', 'à' => 'a', c if c.is_alphanumeric() => c, _ => ' ' }).collect::<String>().split_whitespace().map(str::to_string).collect()
}

fn similarity(a: &str, b: &str) -> usize {
    let (a, b) = (words(a), words(b));
    let hit = a.iter().filter(|w| b.iter().any(|x| x.starts_with(w.as_str()) || w.starts_with(x.as_str()))).count();
    if a.is_empty() { 0 } else { hit * 100 / a.len() }
}

fn steamgrid(key: &str, file: &str, c: &Choice, done: &mut Vec<String>) -> Result<u32> {
    let get = |path: &str| -> Result<Value> {
        Ok(crate::net::get_json(&format!("https://www.steamgriddb.com/api/v2/{path}"), Some(key))?["data"].take())
    };
    let name = display_name(file);
    if !KINDS.iter().any(|k| needed(c, file, k, done)) { return Ok(0); }
    let found = get(&format!("search/autocomplete/{}", words(&name).join("%20")))?;
    let Some(id) = found[0]["id"].as_u64() else { return Ok(0) };
    let mut n = 0;
    for (kind, path) in [("grid", "grids/game/{}?dimensions=600x900&types=static"), ("wide", "grids/game/{}?dimensions=920x430,460x215&types=static"), ("hero", "heroes/game/{}?types=static"), ("logo", "logos/game/{}?types=static"), ("icon", "icons/game/{}?types=static")] {
        if !needed(c, file, kind, done) { continue; }
        let items = get(&path.replace("{}", &id.to_string()))?;
        if let Some(u) = items[0]["url"].as_str() { if save_image(u, file, kind).is_ok() { n += 1; done.push(kind.to_string()); } }
    }
    Ok(n)
}

// --- Sources added by the user: plain descriptions, never code ---
#[derive(Deserialize, Default)]
#[serde(default)]
struct Require { field: String, #[serde(rename = "startsWith")] starts_with: String }

#[derive(Deserialize, Default)]
#[serde(default)]
struct Source {
    name: String,
    url: String,
    results: String,
    title: String,
    require: Option<Require>,
    images: BTreeMap<String, Vec<String>>,
    direct: BTreeMap<String, String>,
}

fn read_source(f: &Path) -> Result<Source> {
    let name = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let text = fs::read_to_string(f)?;
    let mut s: Source = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    if s.url.is_empty() && s.direct.is_empty() { bail!("Ce fichier ne contient ni « url » ni « direct »."); }
    if s.name.is_empty() { s.name = name; }
    Ok(s)
}

fn source_files() -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(sources_dir()) else { return vec![] };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    files
}

/// Readable sources, and the names of the files that could not be read.
fn sources() -> (Vec<Source>, Vec<String>) {
    let (mut good, mut bad) = (vec![], vec![]);
    for f in source_files() {
        let name = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
        match read_source(&f) {
            Ok(s) => good.push(s),
            Err(e) => { crate::log::error(&format!("image source {name}: {e}")); bad.push(name); }
        }
    }
    (good, bad)
}

pub fn source_names() -> Vec<String> {
    sources().0.into_iter().map(|s| s.name).collect()
}

const STEAMGRID: &str = "SteamGridDB";

/// Everything the download panel can offer, in the order they are tried.
pub fn offered(s: &Settings) -> Vec<String> {
    let mut list: Vec<String> = source_names().into_iter().filter(|n| !s.art_disabled.contains(n)).collect();
    if !unprotect(&s.steam_grid_key).is_empty() && !s.art_disabled.iter().any(|n| n == STEAMGRID) { list.push(STEAMGRID.to_string()); }
    list
}

/// Every source, usable or not, for the page that manages them.
pub fn source_list(s: &Settings) -> Value {
    let mut list: Vec<Value> = source_files().iter().map(|f| {
        let file = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        match read_source(f) {
            Ok(src) => serde_json::json!({ "name": src.name, "file": file, "kind": "file", "ok": true, "enabled": !s.art_disabled.contains(&src.name) }),
            Err(e) => serde_json::json!({ "name": f.file_stem().unwrap_or_default().to_string_lossy(), "file": file, "kind": "file", "ok": false, "error": crate::i18n::t(&e.to_string()), "enabled": false }),
        }
    }).collect();
    let key = !unprotect(&s.steam_grid_key).is_empty();
    list.push(serde_json::json!({ "name": STEAMGRID, "kind": "steamgrid", "ok": key, "enabled": key && !s.art_disabled.iter().any(|n| n == STEAMGRID) }));
    Value::Array(list)
}

pub fn set_enabled(name: &str, on: bool) -> Result<()> {
    let mut s = load_settings();
    s.art_disabled.retain(|n| n != name);
    if !on { s.art_disabled.push(name.to_string()); }
    save_settings(&s)
}

/// Copies a source file chosen by the user into the sources folder, once it reads correctly.
pub fn add_source(path: &Path) -> Result<String> {
    if fs::metadata(path)?.len() > 1024 * 1024 { bail!("Ce fichier est trop lourd pour une source d'images."); }
    let src = read_source(path).map_err(|e| anyhow!("Cette source ne peut pas être lue : {e}"))?;
    if src.name == STEAMGRID || source_names().contains(&src.name) { bail!("Une source porte déjà ce nom : {}", src.name); }
    fs::create_dir_all(sources_dir())?;
    let stem: String = src.name.chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).collect::<String>().trim().to_string();
    let stem = if stem.is_empty() { "source".to_string() } else { stem };
    let target = (1..).map(|n| sources_dir().join(if n == 1 { format!("{stem}.json") } else { format!("{stem}-{n}.json") })).find(|p| !p.exists()).unwrap();
    fs::copy(path, &target)?;
    Ok(format!("Source ajoutée : {}", src.name))
}

/// Checks a SteamGridDB key with one search.
pub fn test_key(key: &str) -> Result<String> {
    let key = if key.trim().is_empty() { unprotect(&load_settings().steam_grid_key) } else { key.trim().to_string() };
    if key.is_empty() { bail!("Colle d'abord ta clé SteamGridDB."); }
    match crate::net::get_json("https://www.steamgriddb.com/api/v2/search/autocomplete/zelda", Some(&key)) {
        Ok(v) if v["success"].as_bool() == Some(true) => Ok("La clé fonctionne.".to_string()),
        Ok(_) => bail!("SteamGridDB refuse cette clé."),
        Err(e) if matches!(crate::net::status(&e), Some(401 | 403)) => bail!("SteamGridDB refuse cette clé."),
        Err(e) => bail!("SteamGridDB est injoignable : {e}"),
    }
}

fn fill(template: &str, file: &str) -> String {
    let id = Regex::new(r"\[([0-9A-Fa-f]{16})\]").unwrap().captures(file).map(|c| c[1].to_string()).unwrap_or_default();
    template.replace("{name}", &crate::net::encode(&words(&display_name(file)).join(" "))).replace("{id}", &id)
}

fn walk<'a>(v: &'a Value, path: &str) -> &'a Value {
    path.split('.').filter(|p| !p.is_empty()).fold(v, |cur, key| &cur[key])
}

fn field_starts_with(v: &Value, prefix: &str) -> bool {
    match v {
        Value::String(s) => s.starts_with(prefix),
        Value::Array(list) => list.iter().any(|x| x.as_str().is_some_and(|s| s.starts_with(prefix))),
        _ => false,
    }
}

fn from_source(src: &Source, file: &str, c: &Choice, done: &mut Vec<String>) -> Result<u32> {
    let mut n = 0;
    for (kind, template) in &src.direct {
        if needed(c, file, kind, done) && save_image(&fill(template, file), file, kind).is_ok() { n += 1; done.push(kind.clone()); }
    }
    if src.url.is_empty() || !src.images.keys().any(|k| needed(c, file, k, done)) { return Ok(n); }
    let url = fill(&src.url, file);
    if !url.starts_with("https://") { bail!("Adresse refusée."); }
    let text = crate::net::get_text(&url, MAX_ANSWER)?;
    let answer: Value = serde_json::from_str(&text)?;
    let list = walk(&answer, &src.results);
    let name = display_name(file);
    let title = |d: &Value| similarity(&name, walk(d, &src.title).as_str().unwrap_or(""));
    let best = match list.as_array() {
        Some(items) => items.iter().filter(|d| src.require.as_ref().is_none_or(|r| field_starts_with(walk(d, &r.field), &r.starts_with))).max_by_key(|d| title(d)).filter(|d| title(d) >= 60),
        None => Some(list),
    };
    let Some(best) = best else { return Ok(n) };
    for (kind, fields) in &src.images {
        if !needed(c, file, kind, done) { continue; }
        let Some(u) = fields.iter().find_map(|f| walk(best, f).as_str()) else { continue };
        let u = if u.starts_with("//") { format!("https:{u}") } else { u.to_string() };
        if save_image(&u, file, kind).is_ok() { n += 1; done.push(kind.clone()); }
    }
    Ok(n)
}

/// Downloads the chosen images. An image already present stays unless replacing was chosen.
/// Sources are tried in order: the first one to have an image provides it.
pub fn fetch(c: Choice) -> Result<String> {
    let mut s = load_settings();
    let usable = offered(&s);
    let key = if usable.iter().any(|n| n == STEAMGRID) { unprotect(&s.steam_grid_key) } else { String::new() };
    let (mut list, bad) = sources();
    list.retain(|x| usable.contains(&x.name));
    if usable.is_empty() { bail!("Aucune source active : ajoute ta clé SteamGridDB ou une source, dans Sources d'images."); }
    if c.kinds.is_empty() { bail!("Coche au moins un type d'image."); }
    if c.source != "all" && !usable.contains(&c.source) { bail!("Cette source n'existe plus ou est désactivée."); }
    s.art_kinds = c.kinds.clone();
    s.art_source = c.source.clone();
    s.art_replace = c.replace;
    save_settings(&s)?;
    let games: Vec<String> = rom_files().into_iter().filter(|f| c.file.is_empty() || *f == c.file).collect();
    if games.is_empty() { bail!("Jeu introuvable : {}", c.file); }
    let (mut got, mut missing, mut failed) = (0, vec![], 0);
    for file in games {
        crate::emit_status(&format!("Images : {}...", display_name(&file)));
        let mut done = vec![];
        for src in list.iter().filter(|x| c.source == "all" || c.source == x.name) {
            match from_source(src, &file, &c, &mut done) {
                Ok(n) => got += n,
                Err(e) => { failed += 1; crate::log::error(&format!("image source {}: {e}", src.name)); }
            }
        }
        if !key.is_empty() && (c.source == "all" || c.source == STEAMGRID) { got += steamgrid(&key, &file, &c, &mut done)?; }
        if c.kinds.iter().any(|k| k == "grid") && find_art(&file, "grid").is_none() { missing.push(display_name(&file)); }
    }
    let mut msg = format!("{got} image(s) téléchargée(s).");
    if !missing.is_empty() { msg += &format!(" Sans jaquette : {}.", missing.join(", ")); }
    if failed > 0 { msg += &format!(" Source injoignable : {failed} essai(s), voir le journal."); }
    if !bad.is_empty() { msg += &format!(" Source illisible : {}.", bad.join(", ")); }
    Ok(msg)
}

/// Copies art to Steam; file names depend on each shortcut's id.
pub fn update_steam_art(vdf: &Path) -> Result<usize> {
    let grid = vdf.parent().unwrap().join("grid");
    let mut count = 0;
    for e in crate::steam::read_shortcuts(vdf)? {
        let Some(game) = crate::steam::managed_game(&e) else { continue };
        let file = game.trim_start_matches("Roms/").trim_start_matches("Roms\\").to_string();
        let id = crate::steam::app_id(&e);
        for (kind, suffix) in [("grid", "p"), ("wide", ""), ("hero", "_hero"), ("logo", "_logo")] {
            let Some(art) = find_art(&file, kind) else { continue };
            fs::create_dir_all(&grid)?;
            let ext = art.extension().and_then(|x| x.to_str()).unwrap_or("png").to_string();
            fs::copy(&art, grid.join(format!("{id}{suffix}.{ext}")))?;
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn closest_name_wins() {
        assert_eq!(super::similarity("Mario Kart World", "MARIO KART WORLD Deluxe"), 100);
        assert_eq!(super::similarity("Pokémon Légendes", "Pokemon Legendes"), 100);
        assert!(super::similarity("Mario Kart World", "Mario Party") < 60);
    }

    #[test]
    fn source_fields_are_read_by_path() {
        let v = json!({ "response": { "items": [{ "title": "A", "platforms": ["switch2", "pc"] }] } });
        assert_eq!(super::walk(&v, "response.items")[0]["title"], "A");
        assert_eq!(super::walk(&v, ""), &v);
        assert!(super::field_starts_with(&v["response"]["items"][0]["platforms"], "switch"));
        assert!(!super::field_starts_with(&json!("ps5"), "switch"));
        assert!(!super::field_starts_with(&json!(3), "3"));
    }
}
