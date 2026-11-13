//! What the mod keeps on disk: one JSON file a player, under the game's shared `config/` folder.
//!
//! The game's own player file would hold it too, but a native mod's per-player compound is a
//! hook point Horizon has not numbered yet (`@API:player_persistent_data@`), so the mod keeps its
//! own files: `config/pufferfish_skills/players/<uuid>.json`.

use std::path::{Path, PathBuf};

use horizon_client::mod_api::serde_json::{self, Value};

use crate::state::{PlayerState, Progress};

/// The folder the mod's player files live in.
pub fn folder(config: &Path) -> PathBuf {
    config.join("pufferfish_skills").join("players")
}

/// The file of player `uuid`.
pub fn path(config: &Path, uuid: u128) -> PathBuf {
    folder(config).join(format!("{uuid:032x}.json"))
}

/// A player's progress; nothing earned yet when the file is missing or unreadable.
pub fn read(config: &Path, uuid: u128) -> PlayerState {
    match std::fs::read_to_string(path(config, uuid)) {
        Ok(text) => match parse(&text) {
            Ok(state) => state,
            Err(e) => {
                eprintln!("pufferfish_skills: {uuid:032x}.json: {e}; starting a fresh tree");
                PlayerState::default()
            }
        },
        Err(_) => PlayerState::default(),
    }
}

/// Writes a player's progress, making the folder first. A failed write is logged, not lost
/// silently: the player keeps playing with what the server holds in memory.
pub fn write(config: &Path, uuid: u128, state: &PlayerState) {
    let file = path(config, uuid);
    let text = encode(state);
    let result = std::fs::create_dir_all(folder(config))
        .and_then(|()| std::fs::write(&file, text));
    if let Err(e) = result {
        eprintln!("pufferfish_skills: could not write {}: {e}", file.display());
    }
}

/// The file's JSON.
pub fn encode(state: &PlayerState) -> String {
    let mut categories = serde_json::Map::new();
    for (id, progress) in &state.categories {
        categories.insert(
            id.clone(),
            Value::Object(serde_json::Map::from_iter([
                ("xp".to_owned(), Value::from(progress.xp)),
                ("spent".to_owned(), Value::from(progress.spent)),
            ])),
        );
    }
    let unlocked: Vec<Value> = state
        .unlocked
        .iter()
        .map(|key| Value::from(key.as_str()))
        .collect();
    let root = Value::Object(serde_json::Map::from_iter([
        ("version".to_owned(), Value::from(1)),
        ("categories".to_owned(), Value::Object(categories)),
        ("unlocked".to_owned(), Value::Array(unlocked)),
    ]));
    let mut text = serde_json::to_string_pretty(&root).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads `encode`'s JSON.
pub fn parse(text: &str) -> Result<PlayerState, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let mut state = PlayerState::default();
    if let Some(Value::Object(categories)) = root.get("categories") {
        for (id, value) in categories {
            let xp = value.get("xp").and_then(Value::as_i64).unwrap_or(0);
            let spent = value.get("spent").and_then(Value::as_i64).unwrap_or(0);
            state.categories.insert(
                id.clone(),
                Progress {
                    xp: xp.max(0),
                    spent: spent.max(0),
                },
            );
        }
    }
    if let Some(Value::Array(unlocked)) = root.get("unlocked") {
        for key in unlocked {
            if let Some(key) = key.as_str() {
                state.unlocked.insert(key.to_owned());
            }
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_survives_a_round_trip() {
        let mut state = PlayerState::default();
        state.categories.insert(
            "adventure".into(),
            Progress {
                xp: 240,
                spent: 1,
            },
        );
        state.unlocked.insert("adventure/vitality_1".into());
        state.unlocked.insert("gathering/miner_1".into());
        let text = encode(&state);
        assert!(text.contains("vitality_1"));
        assert_eq!(parse(&text), Ok(state));
    }

    #[test]
    fn a_missing_or_empty_file_is_a_fresh_tree() {
        assert_eq!(parse("{}"), Ok(PlayerState::default()));
        assert!(parse("nonsense").is_err());
    }

    #[test]
    fn the_path_is_the_uuid_in_hex() {
        let path = path(Path::new("/srv"), 0xab);
        assert!(path.ends_with("pufferfish_skills/players/000000000000000000000000000000ab.json"));
    }
}
