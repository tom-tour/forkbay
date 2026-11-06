// Themes that can be shared as files. Only colours are accepted: a theme can never carry code.
use crate::core::*;
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const MAX_SIZE: u64 = 16 * 1024;

fn hex(v: &Value) -> Option<String> {
    v.as_str().filter(|s| Regex::new("^#[0-9a-fA-F]{6}$").unwrap().is_match(s)).map(str::to_lowercase)
}

/// Keeps only known fields with valid values, and refuses the rest.
pub fn clean(v: &Value) -> Result<Value> {
    let name: String = v["name"].as_str().unwrap_or("").chars().filter(|c| c.is_alphanumeric() || " ._-'".contains(*c)).take(40).collect();
    let name = name.trim().to_string();
    if name.is_empty() {
        bail!("Ce thème n'a pas de nom valide.");
    }
    let (Some(a), Some(b)) = (hex(&v["a"]), hex(&v["b"])) else { bail!("Ce fichier ne contient pas de couleurs valides.") };
    let mut out = json!({ "forkbayTheme": 1, "name": name, "a": a, "b": b, "gradient": v["gradient"].as_bool().unwrap_or(true) });
    if v.get("bg").is_some() || v.get("card").is_some() || v.get("text").is_some() {
        let (Some(bg), Some(card), Some(text)) = (hex(&v["bg"]), hex(&v["card"]), hex(&v["text"])) else { bail!("Ce fichier ne contient pas de couleurs valides.") };
        out["bg"] = json!(bg);
        out["card"] = json!(card);
        out["text"] = json!(text);
    }
    Ok(out)
}

pub fn import(path: &Path) -> Result<Value> {
    let meta = fs::metadata(path).map_err(|_| anyhow!("Fichier introuvable : {}", path.display()))?;
    if meta.len() > MAX_SIZE {
        bail!("Ce fichier ne ressemble pas à un thème Forkbay.");
    }
    let raw: Value = fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or_else(|| anyhow!("Ce fichier ne ressemble pas à un thème Forkbay."))?;
    if raw["forkbayTheme"].as_u64() != Some(1) {
        bail!("Ce fichier ne ressemble pas à un thème Forkbay.");
    }
    let theme = clean(&raw)?;
    let mut s = load_settings();
    let name = theme["name"].clone();
    s.user_themes.retain(|t| t["name"] != name);
    s.user_themes.push(theme.clone());
    s.theme = format!("user:{}", name.as_str().unwrap_or(""));
    save_settings(&s)?;
    Ok(theme)
}

pub fn export(path: &Path, theme: &Value) -> Result<String> {
    let clean_theme = clean(theme)?;
    fs::write(path, serde_json::to_vec_pretty(&clean_theme)?)?;
    Ok(format!("Thème exporté : {}", clean_theme["name"].as_str().unwrap_or("")))
}

pub fn remove(name: &str) -> Result<()> {
    let mut s = load_settings();
    s.user_themes.retain(|t| t["name"].as_str() != Some(name));
    if s.theme == format!("user:{name}") {
        s.theme = "aurora".into();
    }
    save_settings(&s)
}

/// Older settings stored the mode in the theme field.
pub fn migrate(s: &mut Settings) {
    if ["dark", "light", "black"].contains(&s.theme.as_str()) {
        s.mode = s.theme.clone();
        s.theme = "aurora".into();
    }
    if s.mode.is_empty() {
        s.mode = "dark".into();
    }
}

pub fn set(a: &Value) -> Result<()> {
    let mut s = load_settings();
    if let Some(m) = a["mode"].as_str().filter(|m| ["dark", "light", "black"].contains(m)) {
        s.mode = m.to_string();
    }
    if let Some(t) = a["theme"].as_str() {
        s.theme = t.chars().take(60).collect();
    }
    if let Some(g) = a["gradient"].as_bool() {
        s.gradient = g;
    }
    if let Some(u) = a["surfaces"].as_bool() {
        s.custom_surfaces = u;
    }
    if let Some(c) = a["custom"].as_object() {
        s.custom_colors = c.iter().filter_map(|(k, v)| hex(v).map(|x| (k.clone(), x))).collect();
    }
    save_settings(&s)
}
