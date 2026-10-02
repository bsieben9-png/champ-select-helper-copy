//! u.gg data source. See DESIGN.md "u.gg data" for the file formats.
//! OWNER: data agent. Public signatures are a contract — don't change them.
//!
//! Files are downloaded through [`HttpCache`] (disk + memory cache) and parsed
//! once into compact typed structures ([`OverviewFile`], [`MatchupsFile`]);
//! everything after that is pure functions over those structures.
//!
//! Data "levels": every stats file is keyed `region → rank → role`. When the
//! settings' region/rank has too little data we widen step by step:
//! (region, rank) → (region, overall) → (world, rank) → (world, overall).
//! `Build.rank` / `Build.region` always say which level was actually used.
//!
//! ARAM Mayhem = the normal ARAM build (exactly what u.gg's Mayhem page
//! shows) + u.gg's per-champion augment ranking (`static.bigbrain.gg/
//! custom-aram-mayhem/…`). See DESIGN.md "ARAM Mayhem".

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use serde_json::Value;

use crate::http_cache::{HttpCache, Parser};
use crate::model::*;

const BASE: &str = "https://stats2.u.gg/lol/1.5";
const VERSIONS_URL: &str =
    "https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json";
/// Endpoint version used when the versions file doesn't list one.
const DEFAULT_API_VERSION: &str = "1.5.0";
/// u.gg's ARAM Mayhem files (augment rankings + names), keyed by u.gg patch.
const MAYHEM_BASE: &str = "https://static.bigbrain.gg/custom-aram-mayhem";
/// Augment icons: `{AUGMENT_ICON_BASE}/{patch}/augments/{id}.webp`.
const AUGMENT_ICON_BASE: &str = "https://static.bigbrain.gg/cdragon-custom";
/// u.gg augment rarity key → `AugmentOption.rarity`, in display order.
const AUGMENT_RARITIES: [(&str, &str); 3] = [
    ("kPrismatic", "prismatic"),
    ("kGold", "gold"),
    ("kSilver", "silver"),
];

const VERSIONS_TTL: Duration = Duration::from_secs(60 * 60);
const STATS_TTL: Duration = Duration::from_secs(12 * 60 * 60);
/// Parsed files kept in memory (parsed overview ≈ 0.5 MB, matchups ≈ 1 MB).
const MEMORY_ENTRIES: usize = 32;
/// Disk cache files not refreshed for this long are deleted at startup.
const DISK_MAX_AGE: Duration = Duration::from_secs(21 * 24 * 60 * 60);
/// On patch day the new patch is listed before its files exist (403), so a
/// missing champion file is retried on the previous patch.
const PATCHES_TO_TRY: usize = 2;

/// General build: if the chosen region/rank has fewer games than this for the
/// role, use the next level (overall rank, then world) that has enough.
pub const GENERAL_MIN_GAMES: u32 = 100;
/// Matchup build: games needed at the settings rank to use it…
pub const MATCHUP_MIN_GAMES_RANK: u32 = 50;
/// …or at rank "overall". Below both → general build, `fell_back_to_general`.
pub const MATCHUP_MIN_GAMES_OVERALL: u32 = 15;
/// Counters: if fewer than this many pass `min_games`, relax the threshold
/// to `min_games / 4`, but never below [`COUNTERS_RELAX_FLOOR`] games.
pub const COUNTERS_MIN_RESULTS: usize = 5;
pub const COUNTERS_RELAX_FLOOR: u32 = 10;
/// Counters returned at most.
pub const COUNTERS_MAX: usize = 30;
/// 4th/5th/6th item options kept (most games first).
pub const ITEM_OPTIONS_MAX: usize = 5;
/// Tier list: champions need this pick rate in the role (pool champions are
/// exempt) in addition to `settings.min_games` games. 0.5% keeps ~40–80
/// champions per role at Emerald+ World.
pub const TIER_MIN_PICK_RATE: f64 = 0.005;
/// ARAM Mayhem augments kept per rarity (u.gg ranks ~30–50 of each).
pub const AUGMENTS_PER_RARITY: usize = 10;

/// u.gg region id of "world".
pub const WORLD: u8 = 12;
/// u.gg rank id of "overall" (all ranks).
pub const OVERALL: u8 = 8;
/// u.gg rank id of "emerald_plus" (the default).
pub const EMERALD_PLUS: u8 = 17;
/// u.gg role id used by ARAM files.
pub const ARAM_ROLE: u8 = 6;

/// Settings region key → u.gg region id.
pub const REGIONS: &[(&str, u8)] = &[
    ("world", 12),
    ("na1", 1),
    ("euw1", 2),
    ("kr", 3),
    ("eun1", 4),
    ("br1", 5),
    ("la1", 6),
    ("la2", 7),
    ("oc1", 8),
    ("ru", 9),
    ("tr1", 10),
    ("jp1", 11),
    ("ph2", 13),
    ("sg2", 14),
    ("th2", 15),
    ("tw2", 16),
    ("vn2", 17),
    ("me1", 18),
];

/// Settings rank key → u.gg rank id.
pub const RANKS: &[(&str, u8)] = &[
    ("emerald_plus", 17),
    ("platinum_plus", 10),
    ("diamond_plus", 11),
    ("diamond_2_plus", 15),
    ("master_plus", 14),
    ("overall", 8),
    ("challenger", 1),
    ("grandmaster", 13),
    ("master", 2),
    ("diamond", 3),
    ("emerald", 16),
    ("platinum", 4),
    ("gold", 5),
    ("silver", 6),
    ("bronze", 7),
    ("iron", 12),
];

/// Region key → id; unknown → world.
pub fn region_id(name: &str) -> u8 {
    lookup_id(REGIONS, name).unwrap_or(WORLD)
}

/// Rank key → id; unknown → emerald_plus.
pub fn rank_id(name: &str) -> u8 {
    lookup_id(RANKS, name).unwrap_or(EMERALD_PLUS)
}

fn lookup_id(table: &[(&str, u8)], name: &str) -> Option<u8> {
    let name = name.trim();
    table
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|&(_, id)| id)
}

fn region_name(id: u8) -> &'static str {
    REGIONS.iter().find(|r| r.1 == id).map_or("world", |r| r.0)
}

fn rank_name(id: u8) -> &'static str {
    RANKS.iter().find(|r| r.1 == id).map_or("overall", |r| r.0)
}

// ---------------------------------------------------------------------------
// Parsed file types
// ---------------------------------------------------------------------------

/// (region id, rank id, role id).
type Key = (u8, u8, u8);
/// (region id, rank id).
type Level = (u8, u8);

#[derive(Debug)]
struct Versions {
    /// Patches, newest first, e.g. ["16_19", "16_18", …].
    patches: Vec<String>,
    /// patch → endpoint → version, e.g. "16_19" → "overview" → "1.5.0".
    endpoints: HashMap<String, HashMap<String, String>>,
}

impl Versions {
    fn version(&self, patch: &str, endpoint: &str) -> &str {
        self.endpoints
            .get(patch)
            .and_then(|e| e.get(endpoint))
            .map_or(DEFAULT_API_VERSION, String::as_str)
    }
}

/// One `data[region][rank][role]` entry of an overview file.
#[derive(Debug, Clone, PartialEq)]
struct OverviewEntry {
    wins: u32,
    games: u32,
    runes: RunePage,
    spells: Spells,
    starting_items: ItemGroup,
    core_items: ItemGroup,
    /// 4th, 5th, 6th item options, most games first.
    item_options: [Vec<ItemOption>; 3],
    /// Skill letters concatenated ("QEWQQR…"); expanded into `Build`.
    skill_order: String,
    skill_priority: String,
}

#[derive(Debug, Default)]
struct OverviewFile {
    entries: HashMap<Key, OverviewEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MatchupRow {
    opponent_id: u32,
    /// This champion's wins vs the opponent.
    wins: u32,
    games: u32,
}

#[derive(Debug, Default)]
struct MatchupsFile {
    /// Rows with games > 0, most games first.
    rows: HashMap<Key, Vec<MatchupRow>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RankingRow {
    champion_id: u32,
    wins: u32,
    games: u32,
}

/// A `champion_ranking` (tier list) file for one region/rank.
#[derive(Debug, Default)]
struct RankingFile {
    /// Matches in the sample (each has 2 champions per role on SR, 10 in ARAM).
    total_matches: u64,
    /// Role key ("top", "jungle", "mid", "adc", "supp"; "none" in ARAM) → rows.
    roles: HashMap<String, Vec<RankingRow>>,
    /// Champion id → matches in which it was banned.
    bans: HashMap<u32, u32>,
}

/// u.gg's per-champion ARAM Mayhem augment ranking. Only an order — u.gg
/// publishes no per-augment games/win rates.
#[derive(Debug, Default)]
struct AugmentRanking {
    /// Rarity key ("kPrismatic", "kGold", "kSilver") → augment ids, best first.
    rarities: HashMap<String, Vec<u32>>,
}

impl RankingFile {
    fn rows(&self, role: Option<Role>) -> Option<&[RankingRow]> {
        let keys: &[&str] = match role {
            None => &["none"],
            Some(Role::Top) => &["top"],
            Some(Role::Jungle) => &["jungle"],
            Some(Role::Mid) => &["mid", "middle"],
            Some(Role::Adc) => &["adc", "bottom"],
            Some(Role::Support) => &["supp", "support", "utility"],
        };
        keys.iter()
            .find_map(|k| self.roles.get(*k))
            .filter(|rows| !rows.is_empty())
            .map(Vec::as_slice)
    }
}

// ---------------------------------------------------------------------------
// Parsing (pure)
// ---------------------------------------------------------------------------

/// A non-negative number (JSON number or numeric string).
fn number(v: &Value) -> Option<f64> {
    let x = match v {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    (x.is_finite() && x >= 0.0).then_some(x)
}

/// A count; missing/invalid → 0.
fn count(v: Option<&Value>) -> u32 {
    v.and_then(number).map_or(0, |x| x as u32)
}

/// A positive integer id (number or numeric string like "5008").
fn id(v: &Value) -> Option<u32> {
    let x = number(v)?;
    (x >= 1.0 && x.fract() == 0.0 && x <= u32::MAX as f64).then_some(x as u32)
}

/// An array of ids; non-ids (null, "2-3170", …) are skipped.
fn ids(v: Option<&Value>) -> Vec<u32> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(id).collect())
        .unwrap_or_default()
}

fn win_rate(wins: u32, games: u32) -> f64 {
    if games == 0 {
        0.0
    } else {
        f64::from(wins) / f64::from(games)
    }
}

/// `[games, wins, [ids]]` (starting / core items).
fn item_group(v: Option<&Value>) -> ItemGroup {
    let games = count(v.and_then(|v| v.get(0)));
    ItemGroup {
        items: ids(v.and_then(|v| v.get(2))),
        games,
        win_rate: win_rate(count(v.and_then(|v| v.get(1))), games),
    }
}

/// Rows `[itemId, wins, games]` → options, most games first, top N.
fn item_options(v: Option<&Value>) -> Vec<ItemOption> {
    let mut out: Vec<ItemOption> = v
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let item_id = id(row.get(0)?)?;
            let games = count(row.get(2));
            (games > 0).then(|| ItemOption {
                item_id,
                games,
                win_rate: win_rate(count(row.get(1)), games),
            })
        })
        .collect();
    out.sort_by_key(|o| Reverse(o.games));
    out.truncate(ITEM_OPTIONS_MAX);
    out
}

/// Parse one overview entry: `[stats, "timestamp"]` (see DESIGN.md for the
/// layout of `stats`). Field orders differ per index — careful:
/// runes/spells/items/skills/shards are `[games, wins, …]`, item option rows
/// are `[id, wins, games]`, overall (index 6) is `[wins, games]`.
/// Returns None if the entry is malformed or has no games.
fn parse_overview_entry(v: &Value) -> Option<OverviewEntry> {
    let stats = v.get(0)?.as_array()?;
    let overall = stats.get(6);
    let wins = count(overall.and_then(|o| o.get(0)));
    let games = count(overall.and_then(|o| o.get(1)));
    if games == 0 {
        return None;
    }

    let r = stats.first();
    let rune_games = count(r.and_then(|r| r.get(0)));
    let runes = RunePage {
        primary_style: count(r.and_then(|r| r.get(2))),
        sub_style: count(r.and_then(|r| r.get(3))),
        perks: ids(r.and_then(|r| r.get(4))),
        shards: ids(stats.get(8).and_then(|s| s.get(2))),
        games: rune_games,
        win_rate: win_rate(count(r.and_then(|r| r.get(1))), rune_games),
    };

    let s = stats.get(1);
    let spell_games = count(s.and_then(|s| s.get(0)));
    let spell_ids = ids(s.and_then(|s| s.get(2)));
    let spells = Spells {
        ids: [
            spell_ids.first().copied().unwrap_or(0),
            spell_ids.get(1).copied().unwrap_or(0),
        ],
        games: spell_games,
        win_rate: win_rate(count(s.and_then(|s| s.get(1))), spell_games),
    };

    let sk = stats.get(4);
    let skill_order: String = sk
        .and_then(|s| s.get(2))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|l| l.as_str()?.trim().chars().next())
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| matches!(c, 'Q' | 'W' | 'E' | 'R'))
        .collect();
    let skill_priority = sk
        .and_then(|s| s.get(3))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let opts = stats.get(5);
    let item_options = [0, 1, 2].map(|i| item_options(opts.and_then(|o| o.get(i))));

    Some(OverviewEntry {
        wins,
        games,
        runes,
        spells,
        starting_items: item_group(stats.get(2)),
        core_items: item_group(stats.get(3)),
        item_options,
        skill_order,
        skill_priority,
    })
}

/// Call `f` for every `data[region][rank][role]` value with numeric keys.
fn for_each_entry(v: &Value, mut f: impl FnMut(Key, &Value)) {
    let key = |k: &String| k.parse::<u8>().ok();
    let Some(regions) = v.as_object() else {
        return;
    };
    for (region, ranks) in regions {
        let (Some(region), Some(ranks)) = (key(region), ranks.as_object()) else {
            continue;
        };
        for (rank, roles) in ranks {
            let (Some(rank), Some(roles)) = (key(rank), roles.as_object()) else {
                continue;
            };
            for (role, entry) in roles {
                if let Some(role) = key(role) {
                    f((region, rank, role), entry);
                }
            }
        }
    }
}

fn json(body: &[u8]) -> anyhow::Result<Value> {
    let v: Value = serde_json::from_slice(body)?;
    if !v.is_object() {
        anyhow::bail!("expected a JSON object");
    }
    Ok(v)
}

fn parse_overview(body: &[u8]) -> anyhow::Result<OverviewFile> {
    let v = json(body)?;
    let mut file = OverviewFile::default();
    for_each_entry(&v, |key, entry| {
        if let Some(e) = parse_overview_entry(entry) {
            file.entries.insert(key, e);
        }
    });
    Ok(file)
}

/// Rows of one matchups entry `[[row…], "timestamp"]`, row =
/// `[opponentId, wins, games, …lane stats]`. Drops games == 0; most games first.
fn parse_matchup_rows(v: &Value) -> Vec<MatchupRow> {
    let mut rows: Vec<MatchupRow> = v
        .get(0)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let games = count(row.get(2));
            Some(MatchupRow {
                opponent_id: id(row.get(0)?)?,
                wins: count(row.get(1)).min(games),
                games,
            })
        })
        .filter(|r| r.games > 0)
        .collect();
    rows.sort_by_key(|r| Reverse(r.games));
    rows
}

fn parse_matchups(body: &[u8]) -> anyhow::Result<MatchupsFile> {
    let v = json(body)?;
    let mut file = MatchupsFile::default();
    for_each_entry(&v, |key, entry| {
        let rows = parse_matchup_rows(entry);
        if !rows.is_empty() {
            file.rows.insert(key, rows);
        }
    });
    Ok(file)
}

/// `{ "16_19": { "overview": "1.5.0", … }, … }`; patches sorted numerically.
fn parse_versions(body: &[u8]) -> anyhow::Result<Versions> {
    let v = json(body)?;
    let obj = v.as_object().expect("checked by json()");
    let mut patches: Vec<((u32, u32), String)> = obj
        .keys()
        .filter_map(|k| {
            let (major, minor) = k.split_once('_')?;
            Some(((major.parse().ok()?, minor.parse().ok()?), k.clone()))
        })
        .collect();
    patches.sort_by_key(|p| Reverse(p.0));
    if patches.is_empty() {
        anyhow::bail!("u.gg versions file lists no patches");
    }
    let endpoints = obj
        .iter()
        .map(|(patch, eps)| {
            let eps = eps
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect();
            (patch.clone(), eps)
        })
        .collect();
    Ok(Versions {
        patches: patches.into_iter().map(|p| p.1).collect(),
        endpoints,
    })
}

/// `{ "champId": [roleIds…] }` → roles, most played first.
fn parse_primary_roles(body: &[u8]) -> anyhow::Result<HashMap<u32, Vec<Role>>> {
    let v = json(body)?;
    let obj = v.as_object().expect("checked by json()");
    Ok(obj
        .iter()
        .filter_map(|(champ, roles)| {
            let champ = champ.parse::<u32>().ok()?;
            let mut out: Vec<Role> = Vec::new();
            for role in roles.as_array()?.iter().filter_map(id) {
                if let Some(role) = u8::try_from(role).ok().and_then(Role::from_ugg_id) {
                    if !out.contains(&role) {
                        out.push(role);
                    }
                }
            }
            Some((champ, out))
        })
        .collect())
}

/// `[ {roleKey: [row…]}, {champId: banCount}, "timestamp", totalMatches ]`,
/// row = `["champId", [[oppId, wins, games]…], wins, games, …6 more totals]`.
/// (The trailing totals look like damage/gold/kills/deaths/assists/cs sums;
/// unused.) Empty samples have `{"total_matches": 0}` as the ban map and 0.0.
fn parse_ranking(body: &[u8]) -> anyhow::Result<RankingFile> {
    let v: Value = serde_json::from_slice(body)?;
    let arr = v
        .as_array()
        .ok_or_else(|| anyhow!("expected a JSON array"))?;
    let roles = arr
        .first()
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(role, rows)| {
            let rows = rows
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| {
                    let games = count(row.get(3));
                    Some(RankingRow {
                        champion_id: id(row.get(0)?)?,
                        wins: count(row.get(2)).min(games),
                        games,
                    })
                })
                .filter(|r| r.games > 0)
                .collect();
            (role.to_ascii_lowercase(), rows)
        })
        .collect();
    let bans = arr
        .get(1)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(champ, bans)| Some((champ.parse().ok()?, count(Some(bans)))))
        .collect();
    Ok(RankingFile {
        total_matches: arr.get(3).and_then(number).map_or(0, |x| x as u64),
        roles,
        bans,
    })
}

/// `{"rarities": {"kPrismatic": [id…], "kGold": […], "kSilver": […]},
/// "lastUpdated": "…"}`, ids best first.
fn parse_augment_ranking(body: &[u8]) -> anyhow::Result<AugmentRanking> {
    let v = json(body)?;
    let rarities = v
        .get("rarities")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("augment ranking has no \"rarities\""))?
        .iter()
        .map(|(rarity, list)| (rarity.clone(), ids(Some(list))))
        .collect();
    Ok(AugmentRanking { rarities })
}

/// Augment manifest `{"augmentId": "Name", …}` → id → name.
fn parse_augment_names(body: &[u8]) -> anyhow::Result<HashMap<u32, String>> {
    let v = json(body)?;
    let obj = v.as_object().expect("checked by json()");
    Ok(obj
        .iter()
        .filter_map(|(id, name)| {
            let name = name.as_str()?.trim();
            let id = id.trim().parse().ok()?;
            (!name.is_empty()).then(|| (id, name.to_string()))
        })
        .collect())
}

fn augment_ranking_url(patch: &str, champion_id: u32) -> String {
    format!(
        "{MAYHEM_BASE}/{patch}/tierlist-per-champion-augments-rarity-{patch}/\
         tierlist-augments-{champion_id}-{patch}.json"
    )
}

fn augment_names_url(patch: &str) -> String {
    format!("{MAYHEM_BASE}/{patch}/aram-mayhem-augment-manifest-{patch}.json")
}

fn augment_icon_url(patch: &str, id: u32) -> String {
    format!("{AUGMENT_ICON_BASE}/{patch}/augments/{id}.webp")
}

// ---------------------------------------------------------------------------
// Selection logic (pure)
// ---------------------------------------------------------------------------

/// Augments to show: prismatic, then gold, then silver, each in u.gg's
/// order (best first), at most [`AUGMENTS_PER_RARITY`] per rarity. Ids
/// without a name (not in the manifest) and repeats are skipped. `patch` is
/// the ranking's patch (for icon URLs). Stats stay 0: u.gg has none.
fn compute_augments(
    ranking: &AugmentRanking,
    names: &HashMap<u32, String>,
    patch: &str,
) -> Vec<AugmentOption> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (key, rarity) in AUGMENT_RARITIES {
        let Some(ids) = ranking.rarities.get(key) else {
            continue;
        };
        out.extend(
            ids.iter()
                .filter_map(|&id| Some((id, names.get(&id)?)))
                .filter(|&(id, _)| seen.insert(id))
                .take(AUGMENTS_PER_RARITY)
                .map(|(id, name)| AugmentOption {
                    id,
                    name: name.clone(),
                    icon: augment_icon_url(patch, id),
                    rarity: rarity.to_string(),
                    description: String::new(),
                    games: 0,
                    win_rate: 0.0,
                    pick_rate: 0.0,
                }),
        );
    }
    out
}

/// Levels to try, most specific first, without duplicates.
fn levels(region: u8, rank: u8) -> Vec<Level> {
    let mut out = Vec::with_capacity(4);
    for r in [region, WORLD] {
        for k in [rank, OVERALL] {
            if !out.contains(&(r, k)) {
                out.push((r, k));
            }
        }
    }
    out
}

/// Summoner's Rift roles with data at one level, most games first.
fn roles_at(file: &OverviewFile, (region, rank): Level) -> Vec<(Role, u32)> {
    let mut roles: Vec<(Role, u32)> = Role::ALL
        .iter()
        .filter_map(|&role| {
            let e = file.entries.get(&(region, rank, role.ugg_id()))?;
            Some((role, e.games))
        })
        .collect();
    roles.sort_by_key(|r| Reverse(r.1));
    roles
}

/// Roles (most games first) at the first level with a meaningful sample,
/// or at the level with the most games if none has one.
fn available_roles(file: &OverviewFile, levels: &[Level]) -> Vec<(Role, u32)> {
    let mut best: Vec<(Role, u32)> = Vec::new();
    let mut best_total = 0u64;
    for &level in levels {
        let roles = roles_at(file, level);
        let total: u64 = roles.iter().map(|r| u64::from(r.1)).sum();
        if total >= u64::from(GENERAL_MIN_GAMES) {
            return roles;
        }
        if total > best_total {
            best_total = total;
            best = roles;
        }
    }
    best
}

/// General build entry for `role`: the first level with at least
/// [`GENERAL_MIN_GAMES`], else the level with the most games.
fn pick_general<'a>(
    file: &'a OverviewFile,
    levels: &[Level],
    role: u8,
) -> Option<(Level, &'a OverviewEntry)> {
    let mut best: Option<(Level, &OverviewEntry)> = None;
    for &(region, rank) in levels {
        let Some(e) = file.entries.get(&(region, rank, role)) else {
            continue;
        };
        if e.games >= GENERAL_MIN_GAMES {
            return Some(((region, rank), e));
        }
        if best.is_none_or(|(_, b)| e.games > b.games) {
            best = Some(((region, rank), e));
        }
    }
    best
}

/// Matchup build entry for `role`: a specific rank needs
/// [`MATCHUP_MIN_GAMES_RANK`] games, rank "overall" needs
/// [`MATCHUP_MIN_GAMES_OVERALL`]. None → sample too small.
fn pick_matchup<'a>(
    file: &'a OverviewFile,
    levels: &[Level],
    role: u8,
) -> Option<(Level, &'a OverviewEntry)> {
    levels.iter().find_map(|&(region, rank)| {
        let e = file.entries.get(&(region, rank, role))?;
        let min = if rank == OVERALL {
            MATCHUP_MIN_GAMES_OVERALL
        } else {
            MATCHUP_MIN_GAMES_RANK
        };
        (e.games >= min).then_some(((region, rank), e))
    })
}

/// Matchup rows for `role` at the first level that has any.
fn pick_rows<'a>(file: &'a MatchupsFile, levels: &[Level], role: u8) -> Option<&'a [MatchupRow]> {
    levels
        .iter()
        .find_map(|&(region, rank)| file.rows.get(&(region, rank, role)))
        .map(Vec::as_slice)
}

fn matchup_stats(rows: &[MatchupRow]) -> Vec<MatchupStat> {
    let mut out: Vec<MatchupStat> = rows
        .iter()
        .filter(|r| r.games > 0)
        .map(|r| MatchupStat {
            opponent_id: r.opponent_id,
            games: r.games,
            wins: r.wins,
            win_rate: win_rate(r.wins, r.games),
        })
        .collect();
    out.sort_by_key(|o| Reverse(o.games));
    out
}

/// Counter-picks from the *enemy's* matchup rows: each row's opponent wins
/// `1 - wins/games` of the time vs the enemy.
///
/// - Normal: keep games >= `min_games`; if fewer than [`COUNTERS_MIN_RESULTS`]
///   survive, relax to `max(min_games / 4, 10)` games.
/// - `counters_pool_only`: only pool champions, games >= `min_games`; if fewer
///   than [`COUNTERS_MIN_RESULTS`] survive, relax straight to 10 games.
///
/// Best win rate first; at most [`COUNTERS_MAX`], but pool champions that
/// pass the filter are kept ahead of non-pool ones when trimming.
fn compute_counters(enemy_id: u32, rows: &[MatchupRow], settings: &Settings) -> Vec<Counter> {
    let pool: HashSet<u32> = settings.champion_pool.iter().copied().collect();
    let candidates: Vec<Counter> = rows
        .iter()
        .filter(|r| r.games > 0 && r.opponent_id != enemy_id)
        .filter(|r| !settings.counters_pool_only || pool.contains(&r.opponent_id))
        .map(|r| Counter {
            champion_id: r.opponent_id,
            games: r.games,
            win_rate: 1.0 - win_rate(r.wins, r.games),
            in_pool: pool.contains(&r.opponent_id),
        })
        .collect();

    let min = settings.min_games;
    let relaxed = if settings.counters_pool_only {
        COUNTERS_RELAX_FLOOR
    } else {
        (min / 4).max(COUNTERS_RELAX_FLOOR)
    }
    .min(min);
    let passing = |threshold: u32| candidates.iter().filter(move |c| c.games >= threshold);
    let threshold = if passing(min).count() >= COUNTERS_MIN_RESULTS {
        min
    } else {
        relaxed
    };
    let mut out: Vec<Counter> = passing(threshold).cloned().collect();
    out.sort_by(|a, b| {
        b.win_rate
            .total_cmp(&a.win_rate)
            .then(b.games.cmp(&a.games))
    });

    if out.len() > COUNTERS_MAX {
        let pool_count = out.iter().filter(|c| c.in_pool).count().min(COUNTERS_MAX);
        let mut other_budget = COUNTERS_MAX - pool_count;
        let mut pool_budget = pool_count;
        out.retain(|c| {
            let budget = if c.in_pool {
                &mut pool_budget
            } else {
                &mut other_budget
            };
            if *budget > 0 {
                *budget -= 1;
                true
            } else {
                false
            }
        });
    }
    out
}

/// Tier list for one role: champions with at least `min_games` games and a
/// pick rate of [`TIER_MIN_PICK_RATE`] (pool champions skip the pick-rate
/// check); only pool champions when `counters_pool_only`. Best win rate first.
fn compute_tier_list(
    file: &RankingFile,
    rows: &[RankingRow],
    settings: &Settings,
) -> Vec<TierEntry> {
    let pool: HashSet<u32> = settings.champion_pool.iter().copied().collect();
    let total = file.total_matches.max(1) as f64;
    let mut out: Vec<TierEntry> = rows
        .iter()
        .filter(|r| r.games > 0 && r.games >= settings.min_games)
        .filter_map(|r| {
            let in_pool = pool.contains(&r.champion_id);
            let pick_rate = f64::from(r.games) / total;
            let wanted = if settings.counters_pool_only {
                in_pool
            } else {
                in_pool || pick_rate >= TIER_MIN_PICK_RATE
            };
            if !wanted {
                return None;
            }
            Some(TierEntry {
                champion_id: r.champion_id,
                games: r.games,
                win_rate: win_rate(r.wins, r.games),
                pick_rate,
                ban_rate: file
                    .bans
                    .get(&r.champion_id)
                    .map_or(0.0, |&b| f64::from(b) / total),
                in_pool,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.win_rate
            .total_cmp(&a.win_rate)
            .then(b.games.cmp(&a.games))
    });
    out
}

#[allow(clippy::too_many_arguments)]
fn make_build(
    champion_id: u32,
    role: Option<Role>,
    opponent_id: Option<u32>,
    queue: Queue,
    patch: &str,
    (region, rank): Level,
    e: &OverviewEntry,
    available_roles: Vec<Role>,
) -> Build {
    Build {
        source: Source::Ugg,
        champion_id,
        role,
        opponent_id,
        queue,
        patch: patch.to_string(),
        rank: rank_name(rank).to_string(),
        region: region_name(region).to_string(),
        games: e.games,
        win_rate: win_rate(e.wins, e.games),
        fell_back_to_general: false,
        runes: e.runes.clone(),
        spells: e.spells.clone(),
        starting_items: e.starting_items.clone(),
        core_items: e.core_items.clone(),
        fourth_items: e.item_options[0].clone(),
        fifth_items: e.item_options[1].clone(),
        sixth_items: e.item_options[2].clone(),
        skill_order: e.skill_order.chars().map(String::from).collect(),
        skill_priority: e.skill_priority.clone(),
        available_roles,
        augments: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// The data source
// ---------------------------------------------------------------------------

pub struct Ugg {
    http: HttpCache,
}

impl Ugg {
    pub fn new(cache_dir: PathBuf) -> Self {
        let http = HttpCache::new(cache_dir, MEMORY_ENTRIES);
        http.prune(|name, age| age < DISK_MAX_AGE && !name.ends_with(".tmp"));
        Ugg { http }
    }

    async fn versions(&self) -> anyhow::Result<Arc<Versions>> {
        self.http
            .get(VERSIONS_URL, VERSIONS_TTL, parse_versions)
            .await?
            .ok_or_else(|| anyhow!("u.gg versions file not found"))
    }

    /// Latest u.gg patch, e.g. "16_19".
    pub async fn latest_patch(&self) -> anyhow::Result<String> {
        Ok(self.versions().await?.patches[0].clone())
    }

    /// A per-patch stats file, newest patch first; falls back to the
    /// previous patch only when the file doesn't exist (403/404).
    /// Returns the patch the data is from.
    async fn stats_file<T: Send + Sync + 'static>(
        &self,
        endpoint: &str,
        url: impl Fn(&str, &str) -> String,
        parse: Parser<T>,
    ) -> anyhow::Result<Option<(String, Arc<T>)>> {
        let versions = self.versions().await?;
        for patch in versions.patches.iter().take(PATCHES_TO_TRY) {
            let url = url(patch, versions.version(patch, endpoint));
            if let Some(v) = self.http.get(&url, STATS_TTL, parse).await? {
                return Ok(Some((patch.clone(), v)));
            }
        }
        Ok(None)
    }

    async fn overview(
        &self,
        champion_id: u32,
        queue: &str,
    ) -> anyhow::Result<Option<(String, Arc<OverviewFile>)>> {
        self.stats_file(
            "overview",
            |patch, ver| format!("{BASE}/overview/{patch}/{queue}/{champion_id}/{ver}.json"),
            parse_overview,
        )
        .await
    }

    async fn matchup_overview(
        &self,
        patch: &str,
        queue: &str,
        champion_id: u32,
        opponent_id: u32,
    ) -> anyhow::Result<Option<Arc<OverviewFile>>> {
        let versions = self.versions().await?;
        let ver = versions.version(patch, "overview");
        let url = format!(
            "{BASE}/overview/{patch}/{queue}/matchups/{champion_id}_{opponent_id}/{ver}.json"
        );
        self.http.get(&url, STATS_TTL, parse_overview).await
    }

    async fn matchups_file(
        &self,
        champion_id: u32,
        queue: &str,
    ) -> anyhow::Result<Option<Arc<MatchupsFile>>> {
        Ok(self
            .stats_file(
                "matchups",
                |patch, ver| format!("{BASE}/matchups/{patch}/{queue}/{champion_id}/{ver}.json"),
                parse_matchups,
            )
            .await?
            .map(|(_, file)| file))
    }

    /// ARAM Mayhem augments for `champion_id` (u.gg's ranking, see
    /// [`compute_augments`]). Empty when u.gg has no ranking for it.
    async fn mayhem_augments(&self, champion_id: u32) -> anyhow::Result<Vec<AugmentOption>> {
        // These files have no endpoint version; the `ver` argument is unused.
        let Some((patch, ranking)) = self
            .stats_file(
                "augments",
                |patch, _| augment_ranking_url(patch, champion_id),
                parse_augment_ranking,
            )
            .await?
        else {
            return Ok(Vec::new());
        };
        let (_, names) = self
            .stats_file(
                "augments",
                |patch, _| augment_names_url(patch),
                parse_augment_names,
            )
            .await?
            .ok_or_else(|| anyhow!("u.gg augment manifest not found"))?;
        Ok(compute_augments(&ranking, &names, &patch))
    }

    /// Champion id → roles, most played first.
    pub async fn primary_roles(&self) -> anyhow::Result<HashMap<u32, Vec<Role>>> {
        let (_, roles) = self
            .stats_file(
                "primary_roles",
                |patch, ver| format!("{BASE}/primary_roles/{patch}/{ver}.json"),
                parse_primary_roles,
            )
            .await?
            .ok_or_else(|| anyhow!("u.gg primary roles file not found"))?;
        Ok((*roles).clone())
    }

    /// Recommended build. `role: None` → the champion's most played role
    /// (ignored for ARAM). `opponent_id` → matchup-specific build, falling
    /// back to the general build when the matchup has too little data.
    ///
    /// A requested role with no data at all falls back to the most played
    /// role (`Build.role` says which role the build is for).
    ///
    /// ARAM Mayhem: the normal ARAM build (as on u.gg's Mayhem page) plus
    /// `augments`; augments are best effort (a failure only leaves them empty).
    pub async fn build(
        &self,
        champion_id: u32,
        role: Option<Role>,
        opponent_id: Option<u32>,
        queue: Queue,
        settings: &Settings,
    ) -> anyhow::Result<Build> {
        let ugg_queue = queue.ugg_queue();
        let region = region_id(&settings.region);
        let (patch, general) = self
            .overview(champion_id, ugg_queue)
            .await?
            .ok_or_else(|| anyhow!("u.gg has no {ugg_queue} data for champion {champion_id}"))?;

        if queue.is_aram() {
            // ARAM files only have rank "overall" and role 6.
            let levels = levels(region, OVERALL);
            let (level, e) = pick_general(&general, &levels, ARAM_ROLE)
                .ok_or_else(|| anyhow!("u.gg has no ARAM data for champion {champion_id}"))?;
            let mut build =
                make_build(champion_id, None, None, queue, &patch, level, e, Vec::new());
            if queue == Queue::AramMayhem {
                match self.mayhem_augments(champion_id).await {
                    Ok(augments) => build.augments = augments,
                    Err(e) => eprintln!("ugg: Mayhem augments for {champion_id}: {e:#}"),
                }
            }
            return Ok(build);
        }

        let levels = levels(region, rank_id(&settings.rank));
        let available: Vec<Role> = available_roles(&general, &levels)
            .into_iter()
            .map(|r| r.0)
            .collect();
        let role = match role {
            Some(r) if pick_general(&general, &levels, r.ugg_id()).is_some() => r,
            _ => *available
                .first()
                .ok_or_else(|| anyhow!("u.gg has no data for champion {champion_id}"))?,
        };
        let (level, entry) = pick_general(&general, &levels, role.ugg_id())
            .ok_or_else(|| anyhow!("u.gg has no {role:?} data for champion {champion_id}"))?;
        let mut build = make_build(
            champion_id,
            Some(role),
            None,
            queue,
            &patch,
            level,
            entry,
            available,
        );

        let Some(opponent_id) = opponent_id.filter(|&o| o != 0) else {
            return Ok(build);
        };
        build.opponent_id = Some(opponent_id);
        build.fell_back_to_general = true;
        match self
            .matchup_overview(&patch, ugg_queue, champion_id, opponent_id)
            .await
        {
            Ok(Some(file)) => {
                if let Some((level, e)) = pick_matchup(&file, &levels, role.ugg_id()) {
                    let available = std::mem::take(&mut build.available_roles);
                    build = make_build(
                        champion_id,
                        Some(role),
                        Some(opponent_id),
                        queue,
                        &patch,
                        level,
                        e,
                        available,
                    );
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("ugg: matchup build {champion_id} vs {opponent_id}: {e:#}"),
        }
        Ok(build)
    }

    /// How `champion_id` does vs every opponent in `role`.
    pub async fn matchups(
        &self,
        champion_id: u32,
        role: Role,
        queue: Queue,
        settings: &Settings,
    ) -> anyhow::Result<Vec<MatchupStat>> {
        if queue.is_aram() {
            return Ok(Vec::new());
        }
        let Some(file) = self.matchups_file(champion_id, queue.ugg_queue()).await? else {
            return Ok(Vec::new());
        };
        let levels = levels(region_id(&settings.region), rank_id(&settings.rank));
        Ok(pick_rows(&file, &levels, role.ugg_id())
            .map(matchup_stats)
            .unwrap_or_default())
    }

    /// Best counter-picks vs `enemy_id` in `role`, best first.
    pub async fn counters(
        &self,
        enemy_id: u32,
        role: Role,
        queue: Queue,
        settings: &Settings,
    ) -> anyhow::Result<Vec<Counter>> {
        if queue.is_aram() {
            return Ok(Vec::new());
        }
        let Some(file) = self.matchups_file(enemy_id, queue.ugg_queue()).await? else {
            return Ok(Vec::new());
        };
        let levels = levels(region_id(&settings.region), rank_id(&settings.rank));
        for (region, rank) in levels {
            if let Some(rows) = file.rows.get(&(region, rank, role.ugg_id())) {
                let counters = compute_counters(enemy_id, rows, settings);
                if !counters.is_empty() {
                    return Ok(counters);
                }
            }
        }
        Ok(Vec::new())
    }

    /// Tier list for `role` (best win rate first), e.g. for blind/first pick.
    /// ARAM has no roles: `role` is ignored and the ARAM list is returned.
    /// Region/rank widen like everywhere else when a file is missing/empty.
    pub async fn tier_list(
        &self,
        role: Role,
        queue: Queue,
        settings: &Settings,
    ) -> anyhow::Result<Vec<TierEntry>> {
        if settings.counters_pool_only && settings.champion_pool.is_empty() {
            return Ok(Vec::new());
        }
        let ugg_queue = queue.ugg_queue();
        let (role, rank) = if queue.is_aram() {
            (None, OVERALL)
        } else {
            (Some(role), rank_id(&settings.rank))
        };
        let levels = levels(region_id(&settings.region), rank);
        let versions = self.versions().await?;
        for patch in versions.patches.iter().take(PATCHES_TO_TRY) {
            let ver = versions.version(patch, "champion_ranking");
            for &(region, rank) in &levels {
                // Region/rank are string keys in this URL ("world", "emerald_plus").
                let url = format!(
                    "{BASE}/champion_ranking/{}/{patch}/{ugg_queue}/{}/{ver}.json",
                    region_name(region),
                    rank_name(rank)
                );
                let Some(file) = self.http.get(&url, STATS_TTL, parse_ranking).await? else {
                    continue;
                };
                if file.total_matches == 0 {
                    continue;
                }
                if let Some(rows) = file.rows(role) {
                    let list = compute_tier_list(&file, rows, settings);
                    if !list.is_empty() {
                        return Ok(list);
                    }
                }
            }
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_cache::test_util::{fixture, temp_dir};

    const YORICK: u32 = 83;
    const GWEN: u32 = 887;

    fn overview_fixture() -> OverviewFile {
        parse_overview(&fixture("overview_83_ranked_solo_5x5.json")).unwrap()
    }

    fn matchups_fixture() -> MatchupsFile {
        parse_matchups(&fixture("matchups_83_ranked_solo_5x5.json")).unwrap()
    }

    /// An offline `Ugg` whose disk cache is pre-filled with the fixtures
    /// (patch 16_19). Anything not seeded behaves like a 403.
    fn seeded_ugg(name: &str) -> (Ugg, PathBuf) {
        let dir = temp_dir(name);
        let ugg = Ugg {
            http: HttpCache::new_offline(dir.clone(), MEMORY_ENTRIES),
        };
        seed(&ugg, VERSIONS_URL, "ugg-api-versions.json");
        let p = "16_19";
        seed(
            &ugg,
            &format!("{BASE}/overview/{p}/ranked_solo_5x5/83/1.5.0.json"),
            "overview_83_ranked_solo_5x5.json",
        );
        seed(
            &ugg,
            &format!("{BASE}/overview/{p}/normal_aram/83/1.5.0.json"),
            "overview_83_normal_aram.json",
        );
        seed(
            &ugg,
            &format!("{BASE}/overview/{p}/ranked_solo_5x5/matchups/83_887/1.5.0.json"),
            "overview_matchup_83_887_ranked_solo_5x5.json",
        );
        seed(
            &ugg,
            &format!("{BASE}/matchups/{p}/ranked_solo_5x5/83/1.5.0.json"),
            "matchups_83_ranked_solo_5x5.json",
        );
        seed(
            &ugg,
            &format!("{BASE}/primary_roles/{p}/1.5.0.json"),
            "primary_roles.json",
        );
        seed(
            &ugg,
            &format!("{BASE}/champion_ranking/world/{p}/ranked_solo_5x5/emerald_plus/1.5.0.json"),
            "champion_ranking_world_emerald_plus.json",
        );
        seed(
            &ugg,
            &augment_ranking_url(p, YORICK),
            "mayhem/tierlist-augments-83-16_19.json",
        );
        seed(
            &ugg,
            &augment_names_url(p),
            "mayhem/aram-mayhem-augment-manifest-16_19.json",
        );
        (ugg, dir)
    }

    fn seed(ugg: &Ugg, url: &str, fixture_name: &str) {
        std::fs::write(ugg.http.path_for(url), fixture(fixture_name)).unwrap();
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn versions_sorted_numerically() {
        let v = parse_versions(&fixture("ugg-api-versions.json")).unwrap();
        assert_eq!(v.patches, ["16_19", "16_18"]);
        assert_eq!(v.version("16_19", "overview"), "1.5.0");
        assert_eq!(v.version("16_19", "flex_bar_data"), "1.4.0");
        assert_eq!(v.version("99_1", "overview"), DEFAULT_API_VERSION);

        let body =
            br#"{"9_24":{},"16_2":{},"16_19":{"overview":"1.6.0"},"16_1":{},"15_24":{},"junk":{}}"#;
        let v = parse_versions(body).unwrap();
        assert_eq!(v.patches, ["16_19", "16_2", "16_1", "15_24", "9_24"]);
        assert_eq!(v.version("16_19", "overview"), "1.6.0");
    }

    #[test]
    fn region_and_rank_ids() {
        assert_eq!(region_id("world"), 12);
        assert_eq!(region_id("EUW1"), 2);
        assert_eq!(region_id("me1"), 18);
        assert_eq!(region_id("nowhere"), WORLD);
        assert_eq!(rank_id("emerald_plus"), 17);
        assert_eq!(rank_id("overall"), 8);
        assert_eq!(rank_id("diamond_2_plus"), 15);
        assert_eq!(rank_id("wood"), EMERALD_PLUS);
        assert_eq!(rank_name(8), "overall");
        assert_eq!(region_name(3), "kr");
        assert_eq!(REGIONS.len(), 18);
        assert_eq!(RANKS.len(), 16);
    }

    #[test]
    fn overview_entry_fields() {
        let file = overview_fixture();
        let e = &file.entries[&(12, 17, 4)];
        assert_eq!((e.wins, e.games), (19553, 40053));
        assert_eq!(e.runes.primary_style, 8400);
        assert_eq!(e.runes.sub_style, 8000);
        assert_eq!(e.runes.perks, [8437, 8009, 8446, 8451, 8473, 9103]);
        assert_eq!(e.runes.shards, [5005, 5008, 5001]);
        assert_eq!(e.runes.games, 9679);
        assert!(approx(e.runes.win_rate, 4741.0 / 9679.0));
        assert_eq!(e.spells.ids, [4, 14]);
        assert_eq!(e.spells.games, 12402);
        assert_eq!(e.starting_items.items, [1054, 2003]);
        assert_eq!(e.core_items.items, [3078, 3047, 3161]);
        assert_eq!(e.core_items.games, 2068);
        assert!(approx(e.core_items.win_rate, 1101.0 / 2068.0));
        assert_eq!(e.skill_order.len(), 18);
        assert!(e.skill_order.starts_with("QEWQQR"));
        assert_eq!(e.skill_priority, "QEW");
        // Item option rows are [id, wins, games]; sorted by games desc.
        let fourth: Vec<(u32, u32)> = e.item_options[0]
            .iter()
            .map(|o| (o.item_id, o.games))
            .collect();
        assert_eq!(fourth, [(6694, 3138), (3053, 2324)]);
        assert!(approx(e.item_options[0][0].win_rate, 1632.0 / 3138.0));
        assert_eq!(e.item_options[1][0].item_id, 3053);
        assert_eq!(e.item_options[2][0].item_id, 6694);
    }

    #[test]
    fn overview_entry_tolerates_nulls_and_string_ids() {
        let v: Value = serde_json::from_str(
            r#"[[[10,5,8000,8100,[8010,null]],[3,1,[4]],[],[null,null,null],[2,1,["q","W",""],"QWE"],
                [[["2-3170",1,1],[3071,2,4]],[],[]],[7,12],false,[1,1,["5008",5002,"x"]]],"ts"]"#,
        )
        .unwrap();
        let e = parse_overview_entry(&v).unwrap();
        assert_eq!(e.games, 12);
        assert_eq!(e.runes.perks, [8010]);
        assert_eq!(e.runes.shards, [5008, 5002]);
        assert_eq!(e.spells.ids, [4, 0]);
        assert!(e.starting_items.items.is_empty());
        assert!(e.core_items.items.is_empty() && e.core_items.games == 0);
        assert_eq!(e.skill_order, "QW");
        assert_eq!(e.item_options[0].len(), 1);
        assert_eq!(e.item_options[0][0].item_id, 3071);
        // No games → no entry.
        let v: Value = serde_json::from_str(r#"[[[],[],[],[],[],[],[0,0]],"ts"]"#).unwrap();
        assert!(parse_overview_entry(&v).is_none());
        assert!(parse_overview_entry(&Value::Null).is_none());
    }

    #[test]
    fn yorick_roles_top_first() {
        let file = overview_fixture();
        let roles: Vec<Role> = available_roles(&file, &levels(12, 17))
            .into_iter()
            .map(|r| r.0)
            .collect();
        assert_eq!(roles[0], Role::Top);
        for r in [Role::Adc, Role::Top, Role::Mid] {
            assert!(roles.contains(&r));
        }
        assert_eq!(
            roles,
            [Role::Top, Role::Mid, Role::Jungle, Role::Support, Role::Adc]
        );
    }

    #[test]
    fn general_pick_widens_small_samples() {
        let file = overview_fixture();
        // Top Emerald+ has plenty of games.
        let (level, e) = pick_general(&file, &levels(12, 17), 4).unwrap();
        assert_eq!((level, e.games), ((12, 17), 40053));
        // ADC Emerald+ has 89 games (< GENERAL_MIN_GAMES) → overall.
        let (level, e) = pick_general(&file, &levels(12, 17), 3).unwrap();
        assert_eq!((level, e.games), ((12, 8), 1318));
        // Region without data → world.
        let (level, _) = pick_general(&file, &levels(3, 17), 4).unwrap();
        assert_eq!(level, (12, 17));
        assert!(pick_general(&file, &levels(12, 17), ARAM_ROLE).is_none());
    }

    #[test]
    fn matchup_overview_yorick_vs_gwen() {
        let file =
            parse_overview(&fixture("overview_matchup_83_887_ranked_solo_5x5.json")).unwrap();
        let e = &file.entries[&(12, 17, 4)];
        assert_eq!((e.wins, e.games), (327, 602));
        // Top: Emerald+ has 602 games.
        let (level, e) = pick_matchup(&file, &levels(12, 17), 4).unwrap();
        assert_eq!((level, e.games), ((12, 17), 602));
        // Jungle: 5 games Emerald+ → overall has 32 (>= 15).
        let (level, e) = pick_matchup(&file, &levels(12, 17), 1).unwrap();
        assert_eq!((level, e.games), ((12, 8), 32));
        // ADC: only 2 games overall → too small.
        assert!(pick_matchup(&file, &levels(12, 17), 3).is_none());
    }

    #[test]
    fn matchups_yorick_vs_gwen() {
        let file = matchups_fixture();
        let rows = pick_rows(&file, &levels(12, 17), 4).unwrap();
        let stats = matchup_stats(rows);
        let gwen = stats.iter().find(|m| m.opponent_id == GWEN).unwrap();
        assert_eq!((gwen.wins, gwen.games), (327, 602));
        assert!(approx(gwen.win_rate, 327.0 / 602.0));
        assert!(stats.windows(2).all(|w| w[0].games >= w[1].games));
        assert!(stats.iter().all(|m| m.games > 0));
        assert_eq!(stats[0].opponent_id, 777); // Yone, most played vs Yorick top
    }

    #[test]
    fn counters_from_enemy_file() {
        let file = matchups_fixture();
        let rows = &file.rows[&(12, 17, 4)];
        let settings = Settings::default(); // min_games 100
        let counters = compute_counters(YORICK, rows, &settings);
        assert!(!counters.is_empty() && counters.len() <= COUNTERS_MAX);
        assert!(counters.iter().all(|c| c.games >= 100));
        assert!(counters.windows(2).all(|w| w[0].win_rate >= w[1].win_rate));
        // Gwen's win rate vs Yorick = 1 - Yorick's. (In the pool so the
        // top-30 cut can't drop her.)
        let all = compute_counters(
            YORICK,
            rows,
            &Settings {
                champion_pool: vec![GWEN],
                ..Settings::default()
            },
        );
        let gwen = all.iter().find(|c| c.champion_id == GWEN).unwrap();
        assert!(gwen.in_pool);
        assert_eq!(gwen.games, 602);
        assert!(approx(gwen.win_rate, 1.0 - 327.0 / 602.0));
    }

    fn row(opponent_id: u32, wins: u32, games: u32) -> MatchupRow {
        MatchupRow {
            opponent_id,
            wins,
            games,
        }
    }

    #[test]
    fn counters_relax_pool_and_cap() {
        // 3 champs with lots of games, 2 with 30 games, 1 with 5.
        let rows = [
            row(1, 60, 100),  // counter wr 0.40
            row(2, 40, 100),  // 0.60
            row(3, 50, 100),  // 0.50
            row(4, 9, 30),    // 0.70
            row(5, 21, 30),   // 0.30
            row(6, 0, 5),     // 1.00 but tiny
            row(99, 50, 100), // the enemy itself (mirror) is never a counter
        ];
        let s = Settings {
            min_games: 100,
            champion_pool: vec![5, 6],
            ..Settings::default()
        };
        // Only 3 pass 100 → relax to max(100/4, 10) = 25.
        let c = compute_counters(99, &rows, &s);
        let ids: Vec<u32> = c.iter().map(|c| c.champion_id).collect();
        assert_eq!(ids, [4, 2, 3, 1, 5]);
        assert!(c.iter().find(|c| c.champion_id == 5).unwrap().in_pool);
        assert!(!c[0].in_pool);

        // Pool only: champs 5 and 6; relax straight to 10 games → 6 (5 games) dropped.
        let pool_only = Settings {
            counters_pool_only: true,
            ..s.clone()
        };
        let c = compute_counters(99, &rows, &pool_only);
        assert_eq!(c.iter().map(|c| c.champion_id).collect::<Vec<_>>(), [5]);

        // Enough with min_games → no relaxing.
        let strict = Settings {
            min_games: 30,
            ..s.clone()
        };
        assert_eq!(compute_counters(99, &rows, &strict).len(), 5);

        // Cap at COUNTERS_MAX, keeping pool champs even if their wr is low.
        let many: Vec<MatchupRow> = (1..=60).map(|i| row(i, i, 200)).collect();
        let s = Settings {
            min_games: 100,
            champion_pool: vec![60],
            ..Settings::default()
        };
        let c = compute_counters(0, &many, &s);
        assert_eq!(c.len(), COUNTERS_MAX);
        assert_eq!(c[0].champion_id, 1);
        assert!(c.iter().any(|c| c.champion_id == 60 && c.in_pool));
        assert!(c.windows(2).all(|w| w[0].win_rate >= w[1].win_rate));
    }

    #[test]
    fn primary_roles_parse() {
        let roles = parse_primary_roles(&fixture("primary_roles.json")).unwrap();
        assert_eq!(
            roles[&YORICK],
            [Role::Top, Role::Mid, Role::Jungle, Role::Support, Role::Adc]
        );
        assert!(roles.len() > 150);
    }

    #[tokio::test]
    async fn build_yorick_vs_gwen_offline() {
        let (ugg, dir) = seeded_ugg("ugg-build");
        let s = Settings::default();
        assert_eq!(ugg.latest_patch().await.unwrap(), "16_19");

        let b = ugg
            .build(YORICK, None, Some(GWEN), Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert_eq!(b.role, Some(Role::Top));
        assert_eq!(b.opponent_id, Some(GWEN));
        assert!(!b.fell_back_to_general);
        assert_eq!((b.games, b.patch.as_str()), (602, "16_19"));
        assert!(approx(b.win_rate, 327.0 / 602.0));
        assert_eq!(
            (b.rank.as_str(), b.region.as_str()),
            ("emerald_plus", "world")
        );
        assert_eq!(b.runes.primary_style, 8400);
        assert_eq!(b.runes.perks.len(), 6);
        assert_eq!(b.runes.shards.len(), 3);
        assert_eq!(b.skill_order.len(), 18);
        assert_eq!(b.skill_order[0], "Q");
        assert_eq!(b.available_roles[0], Role::Top);
        assert!(!b.fourth_items.is_empty());

        // General build (no opponent).
        let g = ugg
            .build(YORICK, Some(Role::Top), None, Queue::NormalDraft, &s)
            .await
            .unwrap();
        assert_eq!(
            (g.games, g.opponent_id, g.fell_back_to_general),
            (40053, None, false)
        );
        assert_eq!(g.queue, Queue::NormalDraft);

        // Jungle vs Gwen: 5 games Emerald+ → overall (32 games).
        let j = ugg
            .build(
                YORICK,
                Some(Role::Jungle),
                Some(GWEN),
                Queue::RankedSolo,
                &s,
            )
            .await
            .unwrap();
        assert_eq!(
            (j.rank.as_str(), j.games, j.fell_back_to_general),
            ("overall", 32, false)
        );

        // ADC vs Gwen: too few games → general ADC build (overall: 89 games Emerald+).
        let a = ugg
            .build(YORICK, Some(Role::Adc), Some(GWEN), Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert!(a.fell_back_to_general);
        assert_eq!(a.opponent_id, Some(GWEN));
        assert_eq!(
            (a.role, a.rank.as_str(), a.games),
            (Some(Role::Adc), "overall", 1318)
        );

        // Matchup file missing (403) → general build.
        let m = ugg
            .build(YORICK, Some(Role::Top), Some(1), Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert!(m.fell_back_to_general);
        assert_eq!((m.opponent_id, m.games), (Some(1), 40053));

        // Region with no data in the (trimmed) fixture → world.
        let kr = Settings {
            region: "kr".into(),
            ..s.clone()
        };
        let k = ugg
            .build(YORICK, None, None, Queue::RankedSolo, &kr)
            .await
            .unwrap();
        assert_eq!((k.region.as_str(), k.role), ("world", Some(Role::Top)));

        // Unknown champion → error.
        assert!(ugg
            .build(1, None, None, Queue::RankedSolo, &s)
            .await
            .is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn build_aram_offline() {
        let (ugg, dir) = seeded_ugg("ugg-aram");
        let s = Settings {
            rank: "challenger".into(),
            ..Settings::default()
        };
        let b = ugg
            .build(YORICK, Some(Role::Top), Some(GWEN), Queue::AramMayhem, &s)
            .await
            .unwrap();
        assert_eq!((b.role, b.opponent_id), (None, None));
        assert_eq!((b.rank.as_str(), b.region.as_str()), ("overall", "world"));
        assert!(b.games > 0);
        assert_eq!(b.spells.ids, [4, 32]);
        assert!(b.available_roles.is_empty());
        // Mayhem: same build as normal ARAM, plus augments.
        assert_eq!(b.augments.len(), 3 * AUGMENTS_PER_RARITY);
        assert_eq!(b.augments[0].id, 1361);
        assert_eq!(b.augments[0].name, "Icathia's Fall");
        assert_eq!(
            b.augments[0].icon,
            "https://static.bigbrain.gg/cdragon-custom/16_19/augments/1361.webp"
        );

        let aram = ugg
            .build(YORICK, None, None, Queue::Aram, &s)
            .await
            .unwrap();
        assert!(aram.augments.is_empty());
        assert_eq!((aram.games, &aram.runes), (b.games, &b.runes));

        // Builds serialized before `augments` existed still deserialize.
        let mut v = serde_json::to_value(&aram).unwrap();
        v.as_object_mut().unwrap().remove("augments");
        assert_eq!(serde_json::from_value::<Build>(v).unwrap(), aram);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn augment_files_parse() {
        let r = parse_augment_ranking(&fixture("mayhem/tierlist-augments-83-16_19.json")).unwrap();
        let len = |k: &str| r.rarities[k].len();
        assert_eq!(
            (len("kPrismatic"), len("kGold"), len("kSilver")),
            (46, 48, 29)
        );
        assert_eq!(r.rarities["kPrismatic"][..3], [1361, 1062, 2098]);
        assert!(parse_augment_ranking(br#"{"tiers":{}}"#).is_err());
        assert!(parse_augment_ranking(b"[]").is_err());

        let names = parse_augment_names(&fixture("mayhem/aram-mayhem-augment-manifest-16_19.json"))
            .unwrap();
        assert_eq!(names.len(), 554);
        assert_eq!(names[&1361], "Icathia's Fall");
        assert_eq!(names[&1104], "Minionmancer");
        // Every augment u.gg ranks for Yorick has a name.
        assert!(r
            .rarities
            .values()
            .flatten()
            .all(|id| names.contains_key(id)));
        let names = parse_augment_names(br#"{"7":" Deft ","x":"Bad","8":"","9":3}"#).unwrap();
        assert_eq!(names, HashMap::from([(7, "Deft".to_string())]));
    }

    #[test]
    fn augments_grouped_by_rarity_best_first() {
        let r = parse_augment_ranking(&fixture("mayhem/tierlist-augments-83-16_19.json")).unwrap();
        let names = parse_augment_names(&fixture("mayhem/aram-mayhem-augment-manifest-16_19.json"))
            .unwrap();
        let a = compute_augments(&r, &names, "16_19");
        assert_eq!(a.len(), 3 * AUGMENTS_PER_RARITY);
        let n = AUGMENTS_PER_RARITY;
        for (i, rarity) in ["prismatic", "gold", "silver"].iter().enumerate() {
            assert!(a[i * n..(i + 1) * n].iter().all(|x| x.rarity == *rarity));
        }
        assert_eq!((a[0].id, a[n].id, a[2 * n].id), (1361, 1403, 1028));
        assert_eq!(
            (a[n].name.as_str(), a[2 * n].name.as_str()),
            ("Stats on Stats!", "Erosion")
        );
        assert!(a
            .iter()
            .all(|x| x.games == 0 && x.win_rate == 0.0 && x.description.is_empty()));

        // Unknown ids, repeats and unknown rarities are skipped; order kept.
        let r = parse_augment_ranking(
            br#"{"rarities":{"kSilver":[5,9,5,6],"kGold":[6],"kOther":[7]}}"#,
        )
        .unwrap();
        let names: HashMap<u32, String> = [(5, "Five"), (6, "Six"), (7, "Seven")]
            .map(|(i, s)| (i, s.to_string()))
            .into();
        let a = compute_augments(&r, &names, "16_18");
        let got: Vec<(u32, &str)> = a.iter().map(|x| (x.id, x.rarity.as_str())).collect();
        assert_eq!(got, [(6, "gold"), (5, "silver")]);
        assert_eq!(
            a[1].icon,
            format!("{AUGMENT_ICON_BASE}/16_18/augments/5.webp")
        );
    }

    #[tokio::test]
    async fn mayhem_augments_previous_patch_and_missing() {
        let (ugg, dir) = seeded_ugg("ugg-mayhem");
        let s = Settings::default();
        // Ranking only on 16_18 (patch day) → used, icons point at 16_18;
        // names still come from the newest manifest.
        std::fs::rename(
            ugg.http.path_for(&augment_ranking_url("16_19", YORICK)),
            ugg.http.path_for(&augment_ranking_url("16_18", YORICK)),
        )
        .unwrap();
        let b = ugg
            .build(YORICK, None, None, Queue::AramMayhem, &s)
            .await
            .unwrap();
        assert_eq!(b.augments[0].name, "Icathia's Fall");
        assert!(b.augments[0].icon.contains("/16_18/"));
        assert_eq!(b.patch, "16_19");

        // No ranking at all → build without augments, not an error.
        let (ugg, dir2) = seeded_ugg("ugg-mayhem-none");
        std::fs::remove_file(ugg.http.path_for(&augment_ranking_url("16_19", YORICK))).unwrap();
        let b = ugg
            .build(YORICK, None, None, Queue::AramMayhem, &s)
            .await
            .unwrap();
        assert!(b.augments.is_empty() && b.games > 0);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(dir2);
    }

    #[tokio::test]
    async fn previous_patch_used_when_latest_missing() {
        let (ugg, dir) = seeded_ugg("ugg-patch");
        // Move Yorick's overview to 16_18 only.
        let new = ugg.http.path_for(&format!(
            "{BASE}/overview/16_19/ranked_solo_5x5/83/1.5.0.json"
        ));
        let old = ugg.http.path_for(&format!(
            "{BASE}/overview/16_18/ranked_solo_5x5/83/1.5.0.json"
        ));
        std::fs::rename(new, old).unwrap();
        let b = ugg
            .build(YORICK, None, None, Queue::RankedSolo, &Settings::default())
            .await
            .unwrap();
        assert_eq!(b.patch, "16_18");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn matchups_counters_roles_offline() {
        let (ugg, dir) = seeded_ugg("ugg-matchups");
        let s = Settings::default();
        let m = ugg
            .matchups(YORICK, Role::Top, Queue::RankedSolo, &s)
            .await
            .unwrap();
        let gwen = m.iter().find(|m| m.opponent_id == GWEN).unwrap();
        assert_eq!((gwen.wins, gwen.games), (327, 602));
        assert!(ugg
            .matchups(YORICK, Role::Top, Queue::Aram, &s)
            .await
            .unwrap()
            .is_empty());
        // No file for this champion → empty, not an error.
        assert!(ugg
            .matchups(1, Role::Top, Queue::RankedSolo, &s)
            .await
            .unwrap()
            .is_empty());

        let c = ugg
            .counters(YORICK, Role::Top, Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert!(!c.is_empty() && c.len() <= COUNTERS_MAX);
        assert!(c.windows(2).all(|w| w[0].win_rate >= w[1].win_rate));

        let roles = ugg.primary_roles().await.unwrap();
        assert_eq!(roles[&YORICK][0], Role::Top);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn ranking_fixture() -> RankingFile {
        parse_ranking(&fixture("champion_ranking_world_emerald_plus.json")).unwrap()
    }

    #[test]
    fn ranking_parse() {
        let file = ranking_fixture();
        assert_eq!(file.total_matches, 1_449_191);
        let mut keys: Vec<&str> = file.roles.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["adc", "jungle", "mid", "supp", "top"]);
        let top = file.rows(Some(Role::Top)).unwrap();
        let yorick = top.iter().find(|r| r.champion_id == YORICK).unwrap();
        // Same numbers as the overview file's Emerald+ World top entry.
        assert_eq!((yorick.wins, yorick.games), (19553, 40053));
        assert!(file.rows(Some(Role::Support)).is_some());
        assert!(file.rows(None).is_none());
        assert_eq!(file.bans[&777], 166_951);
        // Every match has 2 champions per role.
        let games: u64 = top.iter().map(|r| u64::from(r.games)).sum();
        assert_eq!(games, 2 * file.total_matches);

        let empty = parse_ranking(
            br#"[{"adc":[],"top":[]},{"total_matches":0},"2026-10-01T06:19:41Z",0.0]"#,
        )
        .unwrap();
        assert_eq!(empty.total_matches, 0);
        assert!(empty.rows(Some(Role::Top)).is_none() && empty.bans.is_empty());
        let aram = parse_ranking(
            br#"[{"none":[["200",[],3287,5364,1,2]]},{"total_matches":0},"t",215031.0]"#,
        )
        .unwrap();
        assert_eq!(aram.rows(None).unwrap()[0].games, 5364);
    }

    #[test]
    fn tier_list_rates_and_filters() {
        let file = ranking_fixture();
        let rows = file.rows(Some(Role::Top)).unwrap();
        let s = Settings::default();
        let list = compute_tier_list(&file, rows, &s);
        assert!((30..=100).contains(&list.len()), "{} champions", list.len());
        assert!(list.windows(2).all(|w| w[0].win_rate >= w[1].win_rate));
        assert!(list
            .iter()
            .all(|t| t.games >= 100 && t.pick_rate >= TIER_MIN_PICK_RATE));
        let yone = list.iter().find(|t| t.champion_id == 777).unwrap();
        assert!(approx(yone.pick_rate, 122_562.0 / 1_449_191.0)); // ~8.5%
        assert!(approx(yone.ban_rate, 166_951.0 / 1_449_191.0)); // ~11.5%
        assert!(approx(yone.win_rate, 61_375.0 / 122_562.0));
        let yorick = list.iter().find(|t| t.champion_id == YORICK).unwrap();
        assert!(yorick.pick_rate > 0.02 && yorick.pick_rate < 0.04);

        // Pool champions skip the pick-rate filter; pool-only keeps just them.
        let rare = rows
            .iter()
            .find(|r| r.games >= 100 && (f64::from(r.games) / 1_449_191.0) < TIER_MIN_PICK_RATE)
            .unwrap();
        let pool = Settings {
            champion_pool: vec![YORICK, rare.champion_id],
            ..Settings::default()
        };
        let list = compute_tier_list(&file, rows, &pool);
        assert!(list
            .iter()
            .any(|t| t.champion_id == rare.champion_id && t.in_pool));
        let pool_only = Settings {
            counters_pool_only: true,
            ..pool
        };
        let list = compute_tier_list(&file, rows, &pool_only);
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|t| t.in_pool));
    }

    #[tokio::test]
    async fn tier_list_offline() {
        let (ugg, dir) = seeded_ugg("ugg-tier");
        let s = Settings::default();
        let list = ugg
            .tier_list(Role::Support, Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert!(!list.is_empty());
        // kr files aren't seeded (= 403) → widens to world.
        let kr = Settings {
            region: "kr".into(),
            ..Settings::default()
        };
        let list_kr = ugg
            .tier_list(Role::Support, Queue::NormalDraft, &kr)
            .await
            .unwrap();
        assert_eq!(list, list_kr);
        // No ARAM file seeded → empty, not an error.
        assert!(ugg
            .tier_list(Role::Top, Queue::Aram, &s)
            .await
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Live end-to-end check against u.gg: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_yorick_vs_gwen() {
        let dir = temp_dir("ugg-live");
        let ugg = Ugg::new(dir.clone());
        let s = Settings::default();
        let patch = ugg.latest_patch().await.unwrap();
        println!("latest u.gg patch: {patch}");
        let b = ugg
            .build(YORICK, Some(Role::Top), Some(GWEN), Queue::RankedSolo, &s)
            .await
            .unwrap();
        println!(
            "Yorick vs Gwen: {} games, {:.1}% ({} {}), fell back: {}",
            b.games,
            b.win_rate * 100.0,
            b.rank,
            b.region,
            b.fell_back_to_general
        );
        assert_eq!(b.opponent_id, Some(GWEN));
        assert!(!b.fell_back_to_general);
        assert!(b.games >= MATCHUP_MIN_GAMES_RANK);
        assert_eq!(b.runes.perks.len(), 6);
        assert_eq!(b.runes.shards.len(), 3);
        assert!(!b.core_items.items.is_empty());

        let m = ugg
            .matchups(YORICK, Role::Top, Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert!(m.iter().any(|m| m.opponent_id == GWEN));
        let c = ugg
            .counters(YORICK, Role::Top, Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert!(!c.is_empty());
        let roles = ugg.primary_roles().await.unwrap();
        assert_eq!(roles[&YORICK][0], Role::Top);
        let aram = ugg
            .build(YORICK, None, None, Queue::Aram, &s)
            .await
            .unwrap();
        assert!(aram.games > 0 && aram.role.is_none());
        let tiers = ugg
            .tier_list(Role::Top, Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert!(tiers.len() > 20);
        println!(
            "top tier #1: {} ({:.1}% wr, {:.1}% pick, {:.1}% ban)",
            tiers[0].champion_id,
            tiers[0].win_rate * 100.0,
            tiers[0].pick_rate * 100.0,
            tiers[0].ban_rate * 100.0
        );
        let aram_tiers = ugg.tier_list(Role::Top, Queue::Aram, &s).await.unwrap();
        assert!(aram_tiers.len() > 100);

        // Second call is served from memory/disk.
        let again = ugg
            .build(YORICK, Some(Role::Top), Some(GWEN), Queue::RankedSolo, &s)
            .await
            .unwrap();
        assert_eq!(again, b);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Live check of the ARAM Mayhem files: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_yorick_mayhem_augments() {
        let dir = temp_dir("ugg-live-mayhem");
        let ugg = Ugg::new(dir.clone());
        let b = ugg
            .build(YORICK, None, None, Queue::AramMayhem, &Settings::default())
            .await
            .unwrap();
        println!(
            "Yorick Mayhem ({}): {} games, {} augments, top: {:?}",
            b.patch,
            b.games,
            b.augments.len(),
            b.augments
                .iter()
                .take(3)
                .map(|a| &a.name)
                .collect::<Vec<_>>()
        );
        assert!(b.games > 0 && b.runes.perks.len() == 6);
        assert!(b.augments.len() >= 10);
        for rarity in ["prismatic", "gold", "silver"] {
            assert!(b.augments.iter().any(|a| a.rarity == rarity), "{rarity}");
        }
        assert!(b.augments.iter().all(|a| !a.name.is_empty()));
        let icon = &b.augments[0].icon;
        let client = reqwest::Client::builder()
            .user_agent(crate::http_cache::USER_AGENT)
            .build()
            .unwrap();
        let resp = client.get(icon).send().await.unwrap();
        assert!(resp.status().is_success(), "{icon}: {}", resp.status());
        let _ = std::fs::remove_dir_all(dir);
    }
}
