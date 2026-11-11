// Bug reports: gathers what is needed to reproduce a problem, without anything personal.
// Nothing leaves the computer here: the user reads the text and decides to send it.
use crate::core::*;
use crate::games::*;
use crate::setup::*;
use crate::sys::*;
use anyhow::Result;
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const LOG_LINES: usize = 200;

/// Removes paths, names, addresses and secrets from a text.
pub fn sanitize(text: &str) -> String {
    let mut out = text.to_string();
    let exe_dir = exe_dir();
    let mut known: Vec<(String, &str)> = vec![
        (data_path().to_string_lossy().into_owned(), "<Data>"),
        (root().to_string_lossy().into_owned(), "<Files>"),
        (exe_dir.to_string_lossy().into_owned(), "<App>"),
        (home().to_string_lossy().into_owned(), "<Home>"),
    ];
    known.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    for (path, label) in &known {
        if path.len() < 4 { continue; }
        for variant in [path.clone(), path.replace('\\', "/"), path.replace('/', "\\")] {
            // Only a whole folder name: C:\Users\S must not swallow the start of C:\Users\Sam.
            let re = Regex::new(&format!(r"(?i){}(\W|$)", regex::escape(&variant))).unwrap();
            out = re.replace_all(&out, |c: &regex::Captures| format!("{label}{}", &c[1])).into_owned();
        }
    }
    // Any other user folder, on any drive.
    out = Regex::new(r"(?i)([a-z]:[\\/]users[\\/])[^\\/\s]+").unwrap().replace_all(&out, "${1}<user>").into_owned();
    out = Regex::new(r"(/(?:home|Users)/)[^/\s]+").unwrap().replace_all(&out, "${1}<user>").into_owned();
    for var in ["USERNAME", "USER", "COMPUTERNAME", "HOSTNAME", "LOGNAME"] {
        if let Ok(v) = std::env::var(var) {
            if v.len() >= 3 {
                let re = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(&v))).unwrap();
                out = re.replace_all(&out, "<name>").into_owned();
            }
        }
    }
    out = Regex::new(r"[\w.+-]+@[\w-]+\.[\w.-]+").unwrap().replace_all(&out, "<email>").into_owned();
    // A driver version looks like an address, and is needed to debug.
    out = Regex::new(r"(driver\s+)?\b(?:\d{1,3}\.){3}\d{1,5}\b").unwrap().replace_all(&out, |c: &regex::Captures| if c.get(1).is_some() { c[0].to_string() } else { "<ip>".to_string() }).into_owned();
    // Lines that could carry a key or a token never stay.
    out = Regex::new(r"(?im)^.*(_key\b|prod\.keys\s*=|title\.keys\s*=|\btoken\b|\bsecret\b|\bpassword\b|refresh_token|client_secret|username=|nickname=).*$").unwrap().replace_all(&out, "<line removed>").into_owned();
    out
}

fn tail(path: &Path, lines: usize) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    Some(all[all.len().saturating_sub(lines)..].join("\n"))
}

/// The text of the report, already cleaned. The user can read and edit it before sending.
pub fn collect(game: &str, include_log: bool) -> Result<Value> {
    let s = load_settings();
    let data = data_path();
    let (os, cpu, gpu, ram) = system_info();
    let mut t = String::new();
    t += &format!("NX2 Launcher {}\nOS: {os}\nCPU: {cpu}\nGPU: {gpu}\nRAM: {ram}\nLanguage: {}\nData mode: {}\n", crate::update::version(), crate::i18n::current(), s.data_location);
    t += &format!("Keys present: {}\nFirmware files: {}\nCloud: {} ({})\n", has_keys(&data), firmware_count(&data), s.sync.enabled, s.sync.provider);
    t += "\nEmulators:\n";
    for e in crate::emu::list() {
        let folder = root().join(&e.name);
        if crate::emu::find_exe(&folder).is_some() {
            let v = fs::read_to_string(folder.join("version.txt")).unwrap_or_default();
            t += &format!("- {}: {}\n", e.name, if v.trim().is_empty() { "unknown version" } else { v.trim() });
        }
    }
    if !game.is_empty() {
        t += &format!("\nGame: {}\nEmulator chosen: {}\n", display_name(game), game_emulator(&s, game));
        if let Ok(meta) = fs::metadata(root().join("Roms").join(game)) {
            t += &format!("Game file: {} ({} MB)\n", Path::new(game).extension().and_then(|x| x.to_str()).unwrap_or("?"), meta.len() / 1_048_576);
        }
    }
    let log: PathBuf = data.join("log").join("eden_log.txt");
    if include_log {
        match tail(&log, LOG_LINES) {
            Some(l) if !l.trim().is_empty() => t += &format!("\nEmulator log (last {LOG_LINES} lines):\n{l}\n"),
            _ => t += "\nEmulator log: empty or missing\n",
        }
    }
    let journal = crate::log::tail(LOG_LINES);
    t += &format!("\nForkbay journal:\n{}\n", if journal.trim().is_empty() { "empty" } else { journal.as_str() });
    let clean = sanitize(&t);
    let out = manager_dir().join("bug-report.txt");
    fs::create_dir_all(manager_dir())?;
    fs::write(&out, &clean)?;
    Ok(json!({ "text": clean, "file": out.to_string_lossy() }))
}

/// Saves the text after the user edited it, so the file matches what they read.
pub fn save(text: &str) -> Result<()> {
    fs::create_dir_all(manager_dir())?;
    fs::write(manager_dir().join("bug-report.txt"), sanitize(text))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_lose_personal_details() {
        let text = "mail me@example.com\nheader_key = AABBCC\nrefresh_token=xyz\nC:\\Users\\Someone\\game.nsp\n/home/someone/game.nsp\ndriver 31.0.15.5222\nserver 192.168.1.20\nbuild_id=BCF5E512";
        let out = super::sanitize(text);
        for gone in ["example.com", "AABBCC", "xyz", "Someone", "someone", "192.168"] {
            assert!(!out.contains(gone), "{gone} stayed in: {out}");
        }
        for kept in ["<email>", "C:\\Users\\<user>\\game.nsp", "/home/<user>/game.nsp", "driver 31.0.15.5222", "<ip>", "BCF5E512"] {
            assert!(out.contains(kept), "{kept} missing from: {out}");
        }
    }
}
