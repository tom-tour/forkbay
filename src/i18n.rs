// Interface languages. French is the source text, written in the code.
// Other languages are plain files: locales/*.json are built in, and a file dropped in
// Files/Manager/locales works without rebuilding. Missing texts fall back to English.
use crate::core::*;
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

struct Locale {
    code: String,
    raw: Value,
    messages: Vec<(Regex, String)>,
}

fn parse(code: &str, text: &str) -> Option<Locale> {
    let raw: Value = serde_json::from_str(text).ok()?;
    let messages = raw["messages"].as_array().map(|list| list.iter().filter_map(|m| Some((Regex::new(m[0].as_str()?).ok()?, m[1].as_str()?.to_string()))).collect()).unwrap_or_default();
    Some(Locale { code: code.to_lowercase(), raw, messages })
}

fn all() -> &'static Vec<Locale> {
    static LOCALES: OnceLock<Vec<Locale>> = OnceLock::new();
    LOCALES.get_or_init(|| {
        let mut list: Vec<Locale> = EMBEDDED.iter().filter_map(|(c, t)| parse(c, t)).collect();
        if let Ok(rd) = fs::read_dir(manager_dir().join("locales")) {
            for f in rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")) {
                let code = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
                if let Some(l) = fs::read_to_string(&f).ok().and_then(|t| parse(&code, &t)) {
                    list.retain(|x| x.code != l.code);
                    list.push(l);
                }
            }
        }
        list
    })
}

fn find(code: &str) -> Option<&'static Locale> {
    all().iter().find(|l| l.code == code)
}

/// The system language, lowercase (FORKBAY_LANG overrides it for tests).
pub fn system() -> String {
    if let Ok(l) = std::env::var("FORKBAY_LANG") {
        return l.to_lowercase();
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85];
        let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if n > 1 {
            return String::from_utf16_lossy(&buf[..n as usize - 1]).to_lowercase();
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty())).unwrap_or_default().to_lowercase()
    }
}

/// The setting when it names a known language, otherwise the system language, otherwise English.
pub fn resolve(setting: &str) -> String {
    let known = |c: &str| c == "fr" || find(c).is_some();
    if known(setting) {
        return setting.to_string();
    }
    let sys = system();
    let code: String = sys.chars().take_while(|c| c.is_alphabetic()).collect();
    if known(&code) { code } else { "en".to_string() }
}

pub fn current() -> String {
    resolve(&load_settings().language)
}

pub fn is_fr() -> bool {
    current() == "fr"
}

/// The chain to search: the language, then English.
fn chain(code: &str) -> Vec<&'static Locale> {
    let mut out = vec![];
    if code != "fr" {
        if let Some(l) = find(code) { out.push(l); }
        if code != "en" { if let Some(l) = find("en") { out.push(l); } }
    }
    out
}

/// Translates a message written in French. Anything unknown stays as it is.
pub fn t(text: &str) -> String {
    let code = current();
    let locales = chain(&code);
    if locales.is_empty() {
        return text.to_string();
    }
    if let Some(list) = text.strip_prefix("Importé : ").and_then(|s| s.strip_suffix('.')) {
        let mut out = list.to_string();
        for l in locales.iter().rev() {
            if let Some(words) = l.raw["importWords"].as_object() {
                for (fr, tr) in words { out = out.replace(fr.as_str(), tr.as_str().unwrap_or(fr)); }
            }
        }
        let head = locales.iter().find_map(|l| l.raw["importHeading"].as_str()).unwrap_or("Imported");
        return format!("{head}: {out}.");
    }
    for l in &locales {
        for (re, rep) in &l.messages {
            if re.is_match(text) {
                return re.replace(text, rep.as_str()).into_owned();
            }
        }
    }
    text.to_string()
}

/// Import category label and hint.
pub fn category(label: &str, hint: &str) -> (String, String) {
    for l in chain(&current()) {
        let c = &l.raw["categories"][label];
        if let (Some(a), Some(b)) = (c[0].as_str(), c[1].as_str()) {
            return (a.to_string(), b.to_string());
        }
    }
    (label.to_string(), hint.to_string())
}

/// Languages to offer in the settings, with how much of the English text each one covers.
pub fn languages() -> Value {
    let total = find("en").and_then(|l| l.raw["ui"].as_object().map(|o| o.len())).unwrap_or(0);
    let mut list = vec![json!({ "code": "fr", "name": "Français" })];
    for l in all() {
        let done = l.raw["ui"].as_object().map(|o| o.len()).unwrap_or(0);
        list.push(json!({ "code": l.code, "name": l.raw["name"].as_str().unwrap_or(&l.code), "coverage": (done * 100).checked_div(total).unwrap_or(100) }));
    }
    json!(list)
}

/// What the page needs to translate itself.
pub fn page_locale() -> Value {
    let code = current();
    let tables: Vec<Value> = chain(&code).iter().map(|l| json!({ "ui": l.raw["ui"], "patterns": l.raw["uiPatterns"] })).collect();
    json!({ "code": code, "tables": tables })
}
