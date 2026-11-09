// Recommendations per game, from the community guide riyadhHDA/wichforks (MIT).
// They are suggestions: nothing is changed unless the user picks one.
use crate::core::*;
use crate::games::*;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::fs;

const BASE: &str = "https://raw.githubusercontent.com/riyadhHDA/wichforks/main/";
pub const GUIDE_URL: &str = "https://github.com/riyadhHDA/wichforks";

fn get(path: &str) -> Result<String> {
    crate::net::get_text(&format!("{BASE}{path}"), 10 * 1024 * 1024).map_err(|e| anyhow!("Réseau : {e}"))
}

fn cache_path() -> std::path::PathBuf {
    manager_dir().join("wichforks-cache.json")
}

/// Downloads the guide, and falls back on the last copy when offline.
fn load() -> Result<(Value, Value, String, bool)> {
    let fresh = (|| -> Result<Value> {
        let forks: Value = serde_json::from_str(&get("data/forks.json")?)?;
        let reports: Value = serde_json::from_str(&get("data/performance-reports.json")?)?;
        Ok(json!({ "forks": forks, "reports": reports, "readme": get("README.md")? }))
    })();
    let (data, offline) = match fresh {
        Ok(d) => {
            fs::create_dir_all(manager_dir())?;
            let _ = fs::write(cache_path(), d.to_string());
            (d, false)
        }
        Err(e) => {
            let cached = fs::read_to_string(cache_path()).map_err(|_| e)?;
            (serde_json::from_str(&cached)?, true)
        }
    };
    let readme = data["readme"].as_str().unwrap_or("").to_string();
    Ok((data["forks"].clone(), data["reports"].clone(), readme, offline))
}

fn words(s: &str) -> Vec<String> {
    let lower = s.to_lowercase().replace(['\'', '’'], "").replace("ocarina of time", "oot").replace("the legend of ", "").replace(" remake", "");
    lower.chars().map(|c| match c { 'é' | 'è' | 'ê' => 'e', c if c.is_alphanumeric() => c, _ => ' ' }).collect::<String>().split_whitespace().map(str::to_string).collect()
}

/// True when every word of the guide's game name is in our game name.
fn same_game(guide: &str, ours: &str) -> bool {
    let (g, o) = (words(guide), words(ours));
    !g.is_empty() && g.iter().all(|w| o.iter().any(|x| x == w))
}

/// The "Game | First pick | Also try | Evidence" table of the guide. Online-only rows are left out.
fn parse_table(readme: &str) -> Vec<[String; 4]> {
    let mut rows = vec![];
    let mut inside = false;
    for line in readme.lines() {
        let t = line.trim();
        if !t.starts_with('|') { inside = false; continue; }
        let cells: Vec<String> = t.trim_matches('|').split('|').map(|c| c.trim().replace("**", "")).collect();
        if cells.len() >= 4 && cells[0] == "Game" && cells[1] == "First pick" { inside = true; continue; }
        if !inside || cells.len() < 4 || cells[0].starts_with("--") || cells[0].contains('(') { continue; }
        rows.push([cells[0].clone(), cells[1].clone(), cells[2].clone(), cells[3].clone()]);
    }
    rows
}

fn fork_key(f: &Value) -> Vec<String> {
    let id = f["id"].as_str().unwrap_or("").to_lowercase();
    let repo = f["repo"].as_str().unwrap_or("").to_lowercase();
    let owner = repo.trim_start_matches("https://").split('/').nth(1).unwrap_or("").to_string();
    let mut keys = vec![owner];
    if id == "lichii" || id == "orchid" || id == "pokedinner" || id == "droplet" { keys.push(id.clone()); } else if let Some(first) = id.split('-').next() { if first != "eglantine" { keys.push(first.to_string()); } }
    keys
}

fn norm_repo(u: &str) -> String {
    u.to_lowercase().trim_end_matches('/').trim_end_matches(".git").to_string()
}

fn repo_of(e: &crate::emu::Emu) -> String {
    if e.source == "forgejo" { norm_repo(&format!("{}/{}", e.base, e.repo)) } else { norm_repo(&format!("https://github.com/{}", e.repo)) }
}

/// The repository and the ones it was published under before: the guide may still use an old address.
fn repos_of(e: &crate::emu::Emu) -> Vec<String> {
    std::iter::once(repo_of(e)).chain(e.old_repos.iter().map(|r| norm_repo(&format!("https://github.com/{r}")))).collect()
}

/// Reads the fork names written in a table cell.
fn picks(cell: &str, forks: &[Value], vendor: &str) -> Vec<Value> {
    let lower = cell.to_lowercase();
    let list = crate::emu::list();
    let mut out: Vec<Value> = vec![];
    for f in forks {
        if !fork_key(f).iter().any(|k| !k.is_empty() && has_word(&lower, k)) { continue; }
        let repo = norm_repo(f["repo"].as_str().unwrap_or(""));
        let known = list.iter().find(|e| !e.repo.is_empty() && repos_of(e).contains(&repo)).map(|e| e.name.clone());
        let installed = known.as_ref().is_some_and(|n| crate::emu::find_exe(&root().join(n)).is_some());
        let gpu_ok = vendor.is_empty() || f["gpu"].as_array().is_none_or(|g| g.iter().any(|x| x.as_str().unwrap_or("").to_lowercase().contains(&vendor.to_lowercase())));
        let platform = if cfg!(windows) { "windows" } else { "linux" };
        let os_ok = f["platforms"].as_array().is_none_or(|p| p.iter().any(|x| x.as_str().unwrap_or("").to_lowercase().contains(platform)));
        let can_add = repo.starts_with("https://github.com/");
        out.push(json!({ "id": f["id"], "name": f["name"], "repo": f["repo"], "known": known, "installed": installed, "gpuOk": gpu_ok, "osOk": os_ok, "canAdd": can_add, "confidence": f["confidence"] }));
    }
    out
}

fn has_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == word)
}

/// Graphics card maker, to filter the suggestions.
pub fn gpu_vendor() -> String {
    let name = crate::sys::gpu_name().to_lowercase();
    for (k, v) in [("nvidia", "NVIDIA"), ("geforce", "NVIDIA"), ("radeon", "AMD"), ("amd", "AMD"), ("intel", "Intel")] {
        if name.contains(k) { return v.to_string(); }
    }
    String::new()
}

pub fn recommendations() -> Result<Value> {
    let (forks_doc, reports, readme, offline) = load()?;
    let forks = forks_doc["forks"].as_array().cloned().unwrap_or_default();
    let vendor = gpu_vendor();
    let rows = parse_table(&readme);
    let empty = vec![];
    let all_reports = reports["reports"].as_array().unwrap_or(&empty);
    let mut games = serde_json::Map::new();
    for file in rom_files() {
        let name = display_name(&file);
        let Some(row) = rows.iter().find(|r| same_game(&r[0], &name)) else { continue };
        let related: Vec<&Value> = all_reports.iter().filter(|r| same_game(&r["game"].as_str().unwrap_or("").replace(" remake", ""), &name)).collect();
        let same_gpu = related.iter().filter(|r| r["gpu_vendor"].as_str() == Some(vendor.as_str())).count();
        games.insert(file, json!({ "guideName": row[0], "first": picks(&row[1], &forks, &vendor), "also": picks(&row[2], &forks, &vendor), "evidence": row[3], "reports": related.len(), "reportsSameGpu": same_gpu }));
    }
    let catalogue: Vec<Value> = {
        let known: Vec<String> = crate::emu::list().iter().filter(|e| !e.repo.is_empty()).flat_map(repos_of).collect();
        forks.iter().filter(|f| { let r = norm_repo(f["repo"].as_str().unwrap_or("")); !known.contains(&r) && r.starts_with("https://github.com/") })
            .filter(|f| { let p = if cfg!(windows) { "windows" } else { "linux" }; f["platforms"].as_array().is_some_and(|l| l.iter().any(|x| x.as_str().unwrap_or("").to_lowercase().contains(p))) })
            .map(|f| json!({ "name": f["name"], "repo": f["repo"], "bestFor": f["best_for"], "notes": f["notes"], "confidence": f["confidence"] })).collect()
    };
    Ok(json!({ "updated": forks_doc["updated"], "offline": offline, "gpu": vendor, "games": games, "catalogue": catalogue, "guide": GUIDE_URL }))
}
