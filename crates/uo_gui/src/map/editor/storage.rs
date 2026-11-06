//! Where the editor keeps what is not map data: the border sets, as JSON next to the app's
//! settings (`~/.config/uofiddler/map_borders.json`). The first run starts with the three sets
//! UOIddler shipped (`types::seed_border_sets`), with the same tiles in the same order.

use super::types::{seed_border_sets, BorderSet};
use std::path::PathBuf;

/// The app's settings folder, created when missing.
pub fn config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let dir = PathBuf::from(home).join(".config/uofiddler");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn borders_file() -> PathBuf {
    config_dir().join("map_borders.json")
}

/// The saved sets, sorted by name; the seed when nothing was saved yet.
pub fn load_border_sets() -> Vec<BorderSet> {
    let mut sets = match std::fs::read_to_string(borders_file()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| seed_border_sets()),
        Err(_) => seed_border_sets(),
    };
    sets.sort_by(|a, b| a.name.cmp(&b.name));
    sets
}

/// Writes every set.
pub fn save_border_sets(sets: &[BorderSet]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(sets).map_err(|e| e.to_string())?;
    std::fs::write(borders_file(), text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::editor::terrain::ALL;

    #[test]
    fn the_seed_has_a_core_in_every_set_and_survives_json() {
        let seed = seed_border_sets();
        assert_eq!(seed.len(), 3);
        for set in &seed {
            assert!(!set.tiles(ALL).is_empty(), "{}", set.name);
        }
        let text = serde_json::to_string(&seed).unwrap();
        let back: Vec<BorderSet> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, seed);
    }
}
