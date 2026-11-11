// Time played and last game, per game: Forkbay counts from the launch to the emulator closing.
// Kept in Manager/playtime.json as { "<program id, or file>": { "seconds": n, "last": unix time } }.

use crate::core::manager_dir;
use anyhow::Result;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Shorter than this, the emulator was only opened and closed: it is not a game played.
pub const MIN_SECONDS: u64 = 30;

fn path() -> PathBuf {
    manager_dir().join("playtime.json")
}

/// A game is known by its program id, so that renaming or moving its file keeps its time.
fn key(file: &str) -> String {
    crate::config::game_id(file).unwrap_or_else(|| file.to_string())
}

fn load() -> Map<String, Value> {
    fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn add_to(all: &mut Map<String, Value>, file: &str, seconds: u64, end: u64) {
    let e = all.entry(key(file)).or_insert_with(|| json!({ "seconds": 0, "last": 0 }));
    let total = e["seconds"].as_u64().unwrap_or(0) + seconds;
    *e = json!({ "seconds": total, "last": end });
}

/// Counts a game session. Too short a session is not counted.
pub fn add(file: &str, seconds: u64) -> Result<()> {
    if file.is_empty() || seconds < MIN_SECONDS { return Ok(()); }
    let mut all = load();
    add_to(&mut all, file, seconds, now());
    fs::create_dir_all(manager_dir())?;
    let tmp = path().with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&Value::Object(all))?)?;
    fs::rename(&tmp, path())?;
    Ok(())
}

/// Time and last game of every game, read once for the whole list.
pub struct Times(Map<String, Value>);

impl Times {
    pub fn read() -> Self {
        Times(load())
    }

    /// (seconds played, unix time of the last game), zero when never played.
    pub fn of(&self, file: &str) -> (u64, u64) {
        self.0.get(&key(file)).map_or((0, 0), |e| (e["seconds"].as_u64().unwrap_or(0), e["last"].as_u64().unwrap_or(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_add_up_by_program_id() {
        let mut all = Map::new();
        add_to(&mut all, "Zelda [0100000000010000].nsp", 600, 100);
        add_to(&mut all, "Zelda renamed [0100000000010000].nsp", 60, 200);
        add_to(&mut all, "Homebrew.nro", 90, 150);
        let t = Times(all);
        assert_eq!(t.of("D:/Games/Zelda [0100000000010000].xci"), (660, 200));
        assert_eq!(t.of("Homebrew.nro"), (90, 150));
        assert_eq!(t.of("Other.nro"), (0, 0));
    }
}
