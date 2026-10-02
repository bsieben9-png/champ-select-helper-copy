//! Settings persistence (JSON in the app config dir).

use std::path::Path;

use crate::model::Settings;
use crate::ugg::{RANKS, REGIONS};

/// Most champions the pool can hold (League has ~170).
const POOL_MAX: usize = 300;
/// Highest champion id accepted (real ids are below 1000).
const CHAMPION_ID_MAX: u32 = 99_999;
/// Highest "minimum games" accepted (the UI allows up to this).
const MIN_GAMES_MAX: u32 = 100_000;

/// Load settings; missing or unreadable file → defaults.
pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .map(sanitize)
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(settings)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Settings as the app uses them, whatever the UI or the file says: rank
/// and region are known u.gg keys (else the default), the champion pool has
/// valid, unique ids and a sane size, numbers are in range.
pub fn sanitize(mut s: Settings) -> Settings {
    let defaults = Settings::default();
    s.rank = known_key(RANKS, &s.rank).unwrap_or(defaults.rank);
    s.region = known_key(REGIONS, &s.region).unwrap_or(defaults.region);
    s.min_games = s.min_games.min(MIN_GAMES_MAX);
    let mut pool: Vec<u32> = Vec::with_capacity(s.champion_pool.len().min(POOL_MAX));
    for id in s.champion_pool {
        if (1..=CHAMPION_ID_MAX).contains(&id) && !pool.contains(&id) && pool.len() < POOL_MAX {
            pool.push(id);
        }
    }
    s.champion_pool = pool;
    s
}

/// The table's own spelling of `key` (case/whitespace-insensitive), if known.
fn known_key(table: &[(&str, u8)], key: &str) -> Option<String> {
    let key = key.trim();
    table
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(k, _)| (*k).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_settings_are_unchanged() {
        let s = Settings {
            rank: "master_plus".into(),
            region: "euw1".into(),
            min_games: 250,
            champion_pool: vec![83, 887, 122],
            ..Settings::auto_on()
        };
        assert_eq!(sanitize(s.clone()), s);
        assert_eq!(sanitize(Settings::default()), Settings::default());
    }

    #[test]
    fn bad_values_are_fixed() {
        let s = sanitize(Settings {
            rank: "../../etc".into(),
            region: " EUW1 ".into(),
            min_games: u32::MAX,
            champion_pool: [vec![0, 83, 83, 4_000_000_000], (1..2000).collect()].concat(),
            ..Settings::default()
        });
        assert_eq!(s.rank, "emerald_plus");
        assert_eq!(s.region, "euw1");
        assert_eq!(s.min_games, MIN_GAMES_MAX);
        assert_eq!(s.champion_pool.len(), POOL_MAX);
        assert_eq!(&s.champion_pool[..3], &[83, 1, 2]);
    }

    #[test]
    fn a_tampered_file_loads_sanitized() {
        let dir = crate::http_cache::test_util::temp_dir("settings-load");
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"rank": "nonsense", "champion_pool": [0, 83, 83]}"#).unwrap();
        let s = load(&path);
        assert_eq!(s.rank, "emerald_plus");
        assert_eq!(s.champion_pool, vec![83]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
