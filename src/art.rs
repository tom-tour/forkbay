// Game art. Without a key: public Nintendo catalogue. With a free SteamGridDB key: Steam formats too.
use crate::core::*;
use anyhow::{anyhow, Result};
use base64::Engine;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const EXTS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "ico"];

pub fn cover_dir() -> PathBuf { root().join("Covers") }

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

fn save_image(url: &str, file: &str, kind: &str) -> Result<()> {
    if !url.starts_with("https://") { return Err(anyhow!("Adresse refusée.")); }
    let path = url.split('?').next().unwrap_or(url);
    let mut ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    if !EXTS.contains(&ext.as_str()) { ext = "png".into(); }
    let mut reader = agent().get(url).call()?.into_reader();
    fs::create_dir_all(cover_dir())?;
    let mut out = fs::File::create(cover_dir().join(format!("{}-{kind}.{ext}", safe(file))))?;
    std::io::copy(&mut reader, &mut out)?;
    Ok(())
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase().chars().map(|c| match c { 'é' | 'è' | 'ê' => 'e', 'à' => 'a', c if c.is_alphanumeric() => c, _ => ' ' }).collect::<String>().split_whitespace().map(str::to_string).collect()
}

fn similarity(a: &str, b: &str) -> usize {
    let (a, b) = (words(a), words(b));
    let hit = a.iter().filter(|w| b.iter().any(|x| x.starts_with(w.as_str()) || w.starts_with(x.as_str()))).count();
    if a.is_empty() { 0 } else { hit * 100 / a.len() }
}

fn nintendo(file: &str) -> Result<u32> {
    let name = display_name(file);
    let q: String = words(&name).join(" ");
    let url = "https://searching.nintendo-europe.com/en/select";
    let v: Value = agent().get(url).query("q", &q).query("fq", "type:GAME").query("rows", "20").query("wt", "json").query("fl", "title,image_url,image_url_sq_s,image_url_h2x1_s,image_url_h16x9_s,system_type").call()?.into_json()?;
    let docs = v["response"]["docs"].as_array().cloned().unwrap_or_default();
    let best = docs.iter().filter(|d| d["system_type"].as_array().is_some_and(|s| s.iter().any(|x| x.as_str().is_some_and(|t| t.starts_with("nintendoswitch2")))))
        .max_by_key(|d| similarity(&name, d["title"].as_str().unwrap_or("")))
        .filter(|d| similarity(&name, d["title"].as_str().unwrap_or("")) >= 60);
    let Some(d) = best else { return Ok(0) };
    let mut n = 0;
    for (fields, kind) in [(&["image_url_sq_s"][..], "grid"), (&["image_url_h2x1_s", "image_url_h16x9_s", "image_url"][..], "wide")] {
        if find_art(file, kind).is_none() {
            if let Some(u) = fields.iter().find_map(|f| d[*f].as_str()) {
                let u = if u.starts_with("//") { format!("https:{u}") } else { u.to_string() };
                if save_image(&u, file, kind).is_ok() { n += 1; }
            }
        }
    }
    Ok(n)
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

/// Downloads what is missing. An image already present is never replaced.
pub fn fetch_all() -> Result<String> {
    let s = load_settings();
    let key = unprotect(&s.steam_grid_key);
    let (mut got, mut missing) = (0, vec![]);
    for file in rom_files() {
        crate::emit_status(&format!("Images : {}...", display_name(&file)));
        let mut n = 0;
        if !key.is_empty() { n += steamgrid(&key, &file).unwrap_or(0); }
        n += nintendo(&file).unwrap_or(0);
        got += n;
        if find_art(&file, "grid").is_none() { missing.push(display_name(&file)); }
    }
    let mut msg = format!("{got} image(s) téléchargée(s).");
    if !missing.is_empty() { msg += &format!(" Sans jaquette : {}.", missing.join(", ")); }
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
