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

export interface Settings {
  /** Import runes + item set once, when you lock in. Spells are never changed. */
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

export interface Build {
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
  description: string;
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

export interface ImportResult {
  runes: boolean;
  item_set: boolean;
  messages: string[];
  /** All rune page slots are used: the user's current page we'd overwrite if
   * they agree (call import_build again with overwritePageId = id). */
  needs_confirmation: RunePageRef | null;
}

export interface AutoImportEvent {
  champion_id: number;
  opponent_id: number | null;
  result: ImportResult;
}
