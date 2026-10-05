//! Shared types sent between the Rust backend and the Svelte UI.
//! Mirrored by `src/lib/types.ts` — keep both in sync.

use serde::{Deserialize, Serialize};

/// A lane/position. Serialized lowercase: "top", "jungle", "mid", "adc", "support".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Top,
    Jungle,
    Mid,
    Adc,
    Support,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Top, Role::Jungle, Role::Mid, Role::Adc, Role::Support];

    /// u.gg role id (1 jungle, 2 support, 3 adc, 4 top, 5 mid).
    pub fn ugg_id(self) -> u8 {
        match self {
            Role::Jungle => 1,
            Role::Support => 2,
            Role::Adc => 3,
            Role::Top => 4,
            Role::Mid => 5,
        }
    }

    pub fn from_ugg_id(id: u8) -> Option<Role> {
        match id {
            1 => Some(Role::Jungle),
            2 => Some(Role::Support),
            3 => Some(Role::Adc),
            4 => Some(Role::Top),
            5 => Some(Role::Mid),
            _ => None,
        }
    }

    /// League client `assignedPosition` ("top", "jungle", "middle", "bottom", "utility").
    pub fn from_lcu_position(pos: &str) -> Option<Role> {
        match pos.to_ascii_lowercase().as_str() {
            "top" => Some(Role::Top),
            "jungle" => Some(Role::Jungle),
            "middle" | "mid" => Some(Role::Mid),
            "bottom" | "adc" => Some(Role::Adc),
            "utility" | "support" => Some(Role::Support),
            _ => None,
        }
    }
}

/// Game mode for stats. Serialized snake_case.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Queue {
    RankedSolo,
    RankedFlex,
    NormalDraft,
    NormalBlind,
    /// Swiftplay (480) and the old Quickplay (490): champion, runes and
    /// spells are picked per position in the LOBBY; there is no real champ
    /// select (see DESIGN.md "Swiftplay / Quickplay").
    Swiftplay,
    /// Practice Tool: a custom Summoner's Rift game. The client sends
    /// `gameMode` `PRACTICETOOL`. Its queue catalog also names id 3140
    /// "Multiplayer Practice Tool Custom". Id 0 is a generic custom and is
    /// not this mode. Stats are ranked solo.
    PracticeTool,
}

impl Queue {
    /// Map a League client queue id to a stats queue. Unknown queues → None.
    /// ARAM (450) and ARAM Mayhem (2400, 2450) are unsupported; that code is
    /// kept on branch `saved/aram-mayhem`.
    /// 480-483 / 490-493 are the client's own `QUICKPLAY_AND_SWIFTPLAY_QUEUE_IDS`
    /// (rcp-fe-lol-parties); Quickplay (490) was replaced by Swiftplay in 25.07.
    pub fn from_lcu_queue_id(id: i64) -> Option<Queue> {
        match id {
            420 => Some(Queue::RankedSolo),
            440 => Some(Queue::RankedFlex),
            400 => Some(Queue::NormalDraft),
            430 => Some(Queue::NormalBlind),
            480..=483 | 490..=493 => Some(Queue::Swiftplay),
            // Clash: draft with lanes, use ranked solo data.
            700 => Some(Queue::NormalDraft),
            // Client queue catalog: "Multiplayer Practice Tool Custom".
            // Id 0 is generic custom and stays unmapped.
            3140 => Some(Queue::PracticeTool),
            _ => None,
        }
    }

    /// `gameMode` from the lobby or the gameflow session. Only Practice Tool
    /// is recognized here. Custom classic games stay `None`.
    pub fn from_game_mode(mode: &str) -> Option<Queue> {
        mode.eq_ignore_ascii_case("PRACTICETOOL")
            .then_some(Queue::PracticeTool)
    }

    /// Queue name of u.gg's build/tier-list files. Normals, Swiftplay, and
    /// Practice Tool use ranked solo data (bigger sample, has matchup builds).
    pub fn ugg_queue(self) -> &'static str {
        match self {
            Queue::RankedSolo
            | Queue::NormalDraft
            | Queue::NormalBlind
            | Queue::Swiftplay
            | Queue::PracticeTool => "ranked_solo_5x5",
            Queue::RankedFlex => "ranked_flex_sr",
        }
    }

    /// Champions are picked in the lobby, before queueing (Swiftplay /
    /// Quickplay): there is nothing to lock in during "champ select".
    pub fn is_lobby_pick(self) -> bool {
        matches!(self, Queue::Swiftplay)
    }
}

/// Where stats come from. Serialized lowercase ("ugg"). Only u.gg for now.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Ugg,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Automatically import runes + item set once, when you lock in.
    /// (Summoner spells are never changed — recommendation only.)
    pub auto_import: bool,
    pub import_runes: bool,
    pub import_item_set: bool,
    /// u.gg rank key, e.g. "emerald_plus", "overall", "master_plus".
    pub rank: String,
    /// u.gg region key, e.g. "world", "na1", "euw1".
    pub region: String,
    /// Ignore matchups with fewer games than this when ranking counters.
    pub min_games: u32,
    /// Champion ids the user plays.
    pub champion_pool: Vec<u32>,
    /// Only suggest counters from the champion pool.
    pub counters_pool_only: bool,
    /// Stats source.
    #[serde(default)]
    pub source: Source,
}

#[cfg(test)]
impl Settings {
    /// Tests: defaults with auto-import switched on.
    pub fn auto_on() -> Self {
        Settings {
            auto_import: true,
            ..Settings::default()
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // Owner decision: off by default (most cautious under Riot's rules);
            // the user presses Import, or turns auto-import on in Settings.
            auto_import: false,
            import_runes: true,
            import_item_set: true,
            rank: "emerald_plus".into(),
            region: "world".into(),
            min_games: 100,
            champion_pool: Vec::new(),
            counters_pool_only: false,
            source: Source::Ugg,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunePage {
    pub primary_style: u32,
    pub sub_style: u32,
    /// The 6 selected perks (keystone first). Order is u.gg's (the rest
    /// sorted by id, trees mixed), not slot order: match by id. The import
    /// re-orders them into the client's slot order.
    pub perks: Vec<u32>,
    /// 3 stat shards (offense, flex, defense).
    pub shards: Vec<u32>,
    pub games: u32,
    pub win_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemGroup {
    pub items: Vec<u32>,
    pub games: u32,
    pub win_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemOption {
    pub item_id: u32,
    pub games: u32,
    pub win_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Spells {
    pub ids: [u32; 2],
    pub games: u32,
    pub win_rate: f64,
}

/// One recommended ARAM Mayhem augment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AugmentOption {
    pub id: u32,
    pub name: String,
    /// Icon URL.
    pub icon: String,
    /// "prismatic", "gold" or "silver".
    pub rarity: String,
    /// Plain text; empty when the source has none (u.gg has none).
    pub description: String,
    /// Per-augment stats; all 0 when the source has none (u.gg only
    /// publishes a ranking) — the UI should hide them then.
    pub games: u32,
    pub win_rate: f64,
    pub pick_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Build {
    /// Where the stats came from.
    pub source: Source,
    pub champion_id: u32,
    /// None for ARAM.
    pub role: Option<Role>,
    /// Set when this is a matchup-specific build.
    pub opponent_id: Option<u32>,
    pub queue: Queue,
    /// u.gg patch, e.g. "16_19".
    pub patch: String,
    pub rank: String,
    pub region: String,
    /// Games / win rate for this champion (in this matchup, if opponent_id is set).
    pub games: u32,
    pub win_rate: f64,
    /// True when an opponent was requested but the matchup sample was too
    /// small, so this is the general build instead.
    pub fell_back_to_general: bool,
    pub runes: RunePage,
    pub spells: Spells,
    pub starting_items: ItemGroup,
    pub core_items: ItemGroup,
    pub fourth_items: Vec<ItemOption>,
    pub fifth_items: Vec<ItemOption>,
    pub sixth_items: Vec<ItemOption>,
    /// e.g. ["Q","E","W","Q",...]
    pub skill_order: Vec<String>,
    /// Max order, e.g. "QEW".
    pub skill_priority: String,
    /// Roles u.gg has data for, most played first.
    pub available_roles: Vec<Role>,
    /// Unused. ARAM Mayhem augments were removed; kept so older saved builds
    /// still deserialize. Always empty.
    #[serde(default)]
    pub augments: Vec<AugmentOption>,
}

/// How `champion_id` does against one opponent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatchupStat {
    pub opponent_id: u32,
    pub games: u32,
    pub wins: u32,
    pub win_rate: f64,
}

/// A suggested counter-pick against an enemy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Counter {
    pub champion_id: u32,
    pub games: u32,
    /// The counter's win rate vs the enemy (0..1).
    pub win_rate: f64,
    pub in_pool: bool,
}

/// One champion in a role's tier list (used for blind / first pick).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TierEntry {
    pub champion_id: u32,
    pub games: u32,
    /// 0..1
    pub win_rate: f64,
    /// Share of matches in which this champion was picked in this role (0..1).
    pub pick_rate: f64,
    /// Share of matches in which this champion was banned, any role (0..1; 0 if unknown).
    pub ban_rate: f64,
    pub in_pool: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChampionInfo {
    pub id: u32,
    /// Data Dragon id, e.g. "MonkeyKing".
    pub key: String,
    /// Display name, e.g. "Wukong".
    pub name: String,
    pub icon: String,
    /// Primary roles from u.gg, most played first (may be empty).
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemInfo {
    pub id: u32,
    pub name: String,
    pub icon: String,
    pub gold: u32,
    /// Plain-text description (HTML tags stripped).
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuneInfo {
    pub id: u32,
    pub name: String,
    pub icon: String,
    /// Plain-text short description (HTML tags stripped).
    pub short_desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuneStyle {
    pub id: u32,
    pub name: String,
    pub icon: String,
    /// slots[0] = keystones, then 3 rows of minor runes.
    pub slots: Vec<Vec<RuneInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpellInfo {
    /// Numeric id used by the client (e.g. 4 = Flash).
    pub id: u32,
    pub name: String,
    pub icon: String,
}

/// Everything static the UI needs to render names and icons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StaticData {
    /// Data Dragon version, e.g. "16.19.1".
    pub version: String,
    /// u.gg patch, e.g. "16_19".
    pub ugg_patch: String,
    pub champions: Vec<ChampionInfo>,
    pub items: Vec<ItemInfo>,
    pub rune_styles: Vec<RuneStyle>,
    /// Stat shards (5001, 5005, 5007, 5008, 5010, 5011, 5013).
    pub shards: Vec<RuneInfo>,
    pub spells: Vec<SpellInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnemyPick {
    /// 0 when not yet picked.
    pub champion_id: u32,
    pub role: Option<Role>,
    /// True when `role` was guessed from champion role data.
    pub role_inferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllyPick {
    pub champion_id: u32,
    pub role: Option<Role>,
    pub is_me: bool,
}

/// Snapshot of champ select, emitted to the UI as the `champ-select` event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChampSelectState {
    pub in_champ_select: bool,
    pub queue_id: Option<i64>,
    pub queue: Option<Queue>,
    pub my_role: Option<Role>,
    /// Hovered or locked champion (0/None if nothing yet).
    pub my_champion_id: Option<u32>,
    pub my_champion_locked: bool,
    pub allies: Vec<AllyPick>,
    pub enemies: Vec<EnemyPick>,
    /// Enemy in my role (best guess), if known.
    pub lane_opponent_id: Option<u32>,
    pub bans: Vec<u32>,
}

/// One of my Swiftplay / Quickplay lobby slots: a champion picked for a
/// position before queueing (`localMember.playerSlots[i]` in the client).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LobbySlot {
    /// Index in the client's `playerSlots` (0 = primary position).
    pub index: usize,
    /// None while no champion is chosen for this slot.
    pub champion_id: Option<u32>,
    /// From `positionPreference` ("TOP", "JUNGLE", "MIDDLE", "BOTTOM",
    /// "UTILITY"); None for "FILL" / "UNSELECTED".
    pub role: Option<Role>,
}

/// The lobby before queueing, emitted to the UI as the `lobby` event while the
/// client is in the Lobby / Matchmaking / ReadyCheck phases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LobbyState {
    pub in_lobby: bool,
    pub queue_id: Option<i64>,
    pub queue: Option<Queue>,
    /// Swiftplay / Quickplay: my champions per position (`slots`) are picked
    /// here; there is no champ select. Empty for every other queue.
    pub slots: Vec<LobbySlot>,
}

/// Emitted as the `lcu-status` event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LcuStatus {
    pub connected: bool,
    pub summoner_name: Option<String>,
    /// Gameflow phase, e.g. "None", "Lobby", "ChampSelect", "InProgress".
    pub phase: String,
}

/// A rune page in the League client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunePageRef {
    pub id: u64,
    pub name: String,
}

/// Result of importing a build into the client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ImportResult {
    pub runes: bool,
    pub item_set: bool,
    /// Human-readable notes/errors.
    pub messages: Vec<String>,
    /// Set when runes were NOT imported because every rune page slot is
    /// used (and there is no `CSH:` page to reuse): the user's current,
    /// editable page that would be overwritten if they agree. Ask, then call
    /// `import_build` again with `overwrite_page_id = Some(id)`.
    pub needs_confirmation: Option<RunePageRef>,
}

/// Emitted as the `auto-imported` event after an automatic import.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoImportEvent {
    pub champion_id: u32,
    pub opponent_id: Option<u32>,
    pub result: ImportResult,
}

#[cfg(test)]
mod tests {

    #[test]
    fn auto_import_is_off_by_default() {
        assert!(!Settings::default().auto_import);
        // An old settings file without the field also means off.
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.auto_import);
    }

    use super::*;

    #[test]
    fn lcu_queue_ids() {
        assert_eq!(Queue::from_lcu_queue_id(450), None); // ARAM
        assert_eq!(Queue::from_lcu_queue_id(2400), None); // ARAM Mayhem
        assert_eq!(Queue::from_lcu_queue_id(2450), None); // ARAM Mayhem Classic
        assert_eq!(Queue::from_lcu_queue_id(700), Some(Queue::NormalDraft));
        assert_eq!(Queue::from_lcu_queue_id(400), Some(Queue::NormalDraft));
        assert_eq!(Queue::from_lcu_queue_id(430), Some(Queue::NormalBlind));
        for swift in [480, 481, 483, 490, 493] {
            assert_eq!(Queue::from_lcu_queue_id(swift), Some(Queue::Swiftplay));
        }
        assert_eq!(Queue::from_lcu_queue_id(1700), None); // Arena
        assert_eq!(Queue::from_lcu_queue_id(0), None); // generic custom
        assert_eq!(Queue::from_lcu_queue_id(3140), Some(Queue::PracticeTool));
        assert_eq!(Queue::from_game_mode("PRACTICETOOL"), Some(Queue::PracticeTool));
        assert_eq!(Queue::from_game_mode("practicetool"), Some(Queue::PracticeTool));
        assert_eq!(Queue::from_game_mode("CLASSIC"), None);
        assert_eq!(Queue::PracticeTool.ugg_queue(), "ranked_solo_5x5");
        assert!(!Queue::PracticeTool.is_lobby_pick());
        assert_eq!(Queue::Swiftplay.ugg_queue(), "ranked_solo_5x5");
        assert!(Queue::Swiftplay.is_lobby_pick());
        assert!(!Queue::NormalDraft.is_lobby_pick());
        assert_eq!(
            serde_json::to_string(&Queue::Swiftplay).unwrap(),
            "\"swiftplay\""
        );
        assert_eq!(
            serde_json::to_string(&Queue::PracticeTool).unwrap(),
            "\"practice_tool\""
        );
    }
}
