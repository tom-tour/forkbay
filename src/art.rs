// Game art: the user's own images in the Covers folder, SteamGridDB with their free key,
// or image sources they add themselves. The project ships no other source.
use crate::core::*;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
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
    let p = find_art(file, "grid")?;
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

fn save_image(url: &str, file: &str, kind: &str) -> Result<()> {
    if !url.starts_with("https://") { return Err(anyhow!("Adresse refusée.")); }
    let mut bytes = vec![];
    agent().get(url).call()?.into_reader().take(MAX_IMAGE + 1).read_to_end(&mut bytes)?;
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

fn steamgrid(key: &str, file: &str) -> Result<u32> {
    let get = |path: &str| -> Result<Value> {
        let v: Value = agent().get(&format!("https://www.steamgriddb.com/api/v2/{path}")).set("Authorization", &format!("Bearer {key}")).call()?.into_json()?;
        Ok(v["data"].clone())
    };
    let name = display_name(file);
    let found = get(&format!("search/autocomplete/{}", words(&name).join("%20")))?;
    let Some(id) = found[0]["id"].as_u64() else { return Ok(0) };
    let mut n = 0;
    for (kind, path) in [("grid", "grids/game/{}?dimensions=600x900&types=static"), ("wide", "grids/game/{}?dimensions=920x430,460x215&types=static"), ("hero", "heroes/game/{}?types=static"), ("logo", "logos/game/{}?types=static"), ("icon", "icons/game/{}?types=static")] {
        if find_art(file, kind).is_some() { continue; }
        let items = get(&path.replace("{}", &id.to_string()))?;
        if let Some(u) = items[0]["url"].as_str() { if save_image(u, file, kind).is_ok() { n += 1; } }
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

/// Readable sources, and the names of the files that could not be read.
fn sources() -> (Vec<Source>, Vec<String>) {
    let (mut good, mut bad) = (vec![], vec![]);
    let Ok(rd) = fs::read_dir(sources_dir()) else { return (good, bad) };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for f in files {
        let name = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
        match fs::read_to_string(&f).map_err(anyhow::Error::from).and_then(|t| Ok(serde_json::from_str::<Source>(t.trim_start_matches('\u{feff}'))?)) {
            Ok(mut s) if !s.url.is_empty() || !s.direct.is_empty() => { if s.name.is_empty() { s.name = name; } good.push(s); }
            Ok(_) => { crate::log::error(&format!("image source {name}: no url or direct")); bad.push(name); }
            Err(e) => { crate::log::error(&format!("image source {name}: {e}")); bad.push(name); }
        }
    }
    (good, bad)
}

pub fn source_names() -> Vec<String> {
    sources().0.into_iter().map(|s| s.name).collect()
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn fill(template: &str, file: &str) -> String {
    let id = Regex::new(r"\[([0-9A-Fa-f]{16})\]").unwrap().captures(file).map(|c| c[1].to_string()).unwrap_or_default();
    template.replace("{name}", &encode(&words(&display_name(file)).join(" "))).replace("{id}", &id)
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

fn from_source(src: &Source, file: &str) -> Result<u32> {
    let mut n = 0;
    for (kind, template) in &src.direct {
        if KINDS.contains(&kind.as_str()) && find_art(file, kind).is_none() && save_image(&fill(template, file), file, kind).is_ok() { n += 1; }
    }
    if src.url.is_empty() { return Ok(n); }
    let url = fill(&src.url, file);
    if !url.starts_with("https://") { bail!("Adresse refusée."); }
    let mut text = String::new();
    agent().get(&url).call()?.into_reader().take(MAX_ANSWER).read_to_string(&mut text)?;
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
        if !KINDS.contains(&kind.as_str()) || find_art(file, kind).is_some() { continue; }
        let Some(u) = fields.iter().find_map(|f| walk(best, f).as_str()) else { continue };
        let u = if u.starts_with("//") { format!("https:{u}") } else { u.to_string() };
        if save_image(&u, file, kind).is_ok() { n += 1; }
    }
    Ok(n)
}

/// Downloads what is missing. An image already present is never replaced.
pub fn fetch_all() -> Result<String> {
    let s = load_settings();
    let key = unprotect(&s.steam_grid_key);
    let (list, bad) = sources();
    if key.is_empty() && list.is_empty() { bail!("Renseigne d'abord ta clé SteamGridDB dans la page Réglages."); }
    let (mut got, mut missing, mut failed) = (0, vec![], 0);
    for file in rom_files() {
        crate::emit_status(&format!("Images : {}...", display_name(&file)));
        for src in &list {
            match from_source(src, &file) {
                Ok(n) => got += n,
                Err(e) => { failed += 1; crate::log::error(&format!("image source {}: {e}", src.name)); }
            }
        }
        if !key.is_empty() { got += steamgrid(&key, &file)?; }
        if find_art(&file, "grid").is_none() { missing.push(display_name(&file)); }
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
