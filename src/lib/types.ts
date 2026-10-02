// Shared types — mirror of src-tauri/src/model.rs. Keep both in sync.

export type Role = "top" | "jungle" | "mid" | "adc" | "support";
export const ROLES: Role[] = ["top", "jungle", "mid", "adc", "support"];

export type Queue =
  | "ranked_solo"
  | "ranked_flex"
  | "normal_draft"
  | "normal_blind"
  | "aram"
  | "aram_mayhem";

/** Stats provider. Only u.gg for now; more may be added later. */
export type Source = "ugg";

export interface Settings {
  /** Import runes / item set once, when you lock in. */
  auto_import: boolean;
  import_runes: boolean;
  import_item_set: boolean;
  /** u.gg rank key, e.g. "emerald_plus" */
  rank: string;
  /** u.gg region key, e.g. "world" */
  region: string;
  min_games: number;
  champion_pool: number[];
  counters_pool_only: boolean;
  source: Source;
}

export interface RunePage {
  primary_style: number;
  sub_style: number;
  perks: number[];
  shards: number[];
  games: number;
  win_rate: number;
}

export interface ItemGroup {
  items: number[];
  games: number;
  win_rate: number;
}

export interface ItemOption {
  item_id: number;
  games: number;
  win_rate: number;
}

export interface Spells {
  ids: [number, number];
  games: number;
  win_rate: number;
}

/** One recommended ARAM Mayhem augment. */
export interface AugmentOption {
  id: number;
  name: string;
  /** Icon URL. */
  icon: string;
  /** "prismatic" | "gold" | "silver" */
  rarity: string;
  /** Plain text; empty when the source has none (u.gg has none). */
  description: string;
  /** Per-augment stats; all 0 when the source has none (u.gg only
   * publishes a ranking) — hide them then. */
  games: number;
  win_rate: number;
  pick_rate: number;
}

export interface Build {
  source: Source;
  champion_id: number;
  role: Role | null;
  opponent_id: number | null;
  queue: Queue;
  patch: string;
  rank: string;
  region: string;
  games: number;
  win_rate: number;
  fell_back_to_general: boolean;
  runes: RunePage;
  spells: Spells;
  starting_items: ItemGroup;
  core_items: ItemGroup;
  fourth_items: ItemOption[];
  fifth_items: ItemOption[];
  sixth_items: ItemOption[];
  skill_order: string[];
  skill_priority: string;
  available_roles: Role[];
  /** ARAM Mayhem only (empty otherwise): prismatic, then gold, then silver;
   * best first within each rarity. */
  augments: AugmentOption[];
}

export interface MatchupStat {
  opponent_id: number;
  games: number;
  wins: number;
  win_rate: number;
}

export interface Counter {
  champion_id: number;
  games: number;
  win_rate: number;
  in_pool: boolean;
}

/** One row of a role tier list (sorted best first). */
export interface TierEntry {
  champion_id: number;
  games: number;
  win_rate: number;
  pick_rate: number;
  ban_rate: number;
  in_pool: boolean;
}

export interface ChampionInfo {
  id: number;
  key: string;
  name: string;
  icon: string;
  roles: Role[];
}

export interface ItemInfo {
  id: number;
  name: string;
  icon: string;
  gold: number;
}

export interface RuneInfo {
  id: number;
  name: string;
  icon: string;
  short_desc: string;
}

export interface RuneStyle {
  id: number;
  name: string;
  icon: string;
  slots: RuneInfo[][];
}

export interface SpellInfo {
  id: number;
  name: string;
  icon: string;
}

export interface StaticData {
  version: string;
  ugg_patch: string;
  champions: ChampionInfo[];
  items: ItemInfo[];
  rune_styles: RuneStyle[];
  shards: RuneInfo[];
  spells: SpellInfo[];
}

export interface EnemyPick {
  champion_id: number;
  role: Role | null;
  role_inferred: boolean;
}

export interface AllyPick {
  champion_id: number;
  role: Role | null;
  is_me: boolean;
}

export interface ChampSelectState {
  in_champ_select: boolean;
  queue_id: number | null;
  queue: Queue | null;
  my_role: Role | null;
  my_champion_id: number | null;
  my_champion_locked: boolean;
  allies: AllyPick[];
  enemies: EnemyPick[];
  lane_opponent_id: number | null;
  bans: number[];
}

export interface LcuStatus {
  connected: boolean;
  summoner_name: string | null;
  phase: string;
}

export interface RunePageRef {
  id: number;
  name: string;
}

/** Summoner spells are recommendation-only: the app never changes them. */
export interface ImportResult {
  runes: boolean;
  item_set: boolean;
  messages: string[];
  /** Set when all rune pages are full: ask before overwriting this page,
   *  then call import_build again with `overwritePageId: page.id`. */
  needs_confirmation: RunePageRef | null;
}

export interface AutoImportEvent {
  champion_id: number;
  opponent_id: number | null;
  result: ImportResult;
}
