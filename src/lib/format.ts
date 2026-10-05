// Labels and small formatting helpers shared by the UI.
import type { Queue, Role, Source } from "./types";

export const SOURCES: { value: Source; label: string }[] = [{ value: "ugg", label: "u.gg" }];
export const sourceLabel = (s: Source | string | undefined) =>
  SOURCES.find((x) => x.value === s)?.label ?? s ?? "";

export const ROLE_LABEL: Record<Role, string> = {
  top: "Top",
  jungle: "Jungle",
  mid: "Mid",
  adc: "ADC",
  support: "Support",
};

export const ROLE_SHORT: Record<Role, string> = {
  top: "Top",
  jungle: "Jg",
  mid: "Mid",
  adc: "Bot",
  support: "Sup",
};

export const QUEUE_LABEL: Record<Queue, string> = {
  ranked_solo: "Ranked Solo/Duo",
  ranked_flex: "Ranked Flex",
  normal_draft: "Normal Draft",
  normal_blind: "Normal Blind",
  swiftplay: "Swiftplay",
};

/** Queues offered in Lookup. Normals and Swiftplay use ranked solo data. */
export const LOOKUP_QUEUES: { value: Queue; label: string }[] = [
  { value: "ranked_solo", label: "Ranked Solo" },
  { value: "ranked_flex", label: "Flex" },
  { value: "normal_draft", label: "Normal Draft" },
  { value: "swiftplay", label: "Swiftplay" },
];

/** Queue name for a client queue id: 490-493 is the old Quickplay. */
export function queueLabel(q: Queue | null | undefined, queueId?: number | null): string {
  if (!q) return "";
  if (q === "swiftplay" && queueId != null && queueId >= 490 && queueId <= 493) return "Quickplay";
  return QUEUE_LABEL[q];
}

/** Champions are picked in the lobby (no champ select to lock in). */
export const isLobbyPick = (q: Queue | null | undefined) => q === "swiftplay";

/** Gameflow phases before champ select, while the lobby exists. */
export const isLobbyPhase = (phase: string) => ["Lobby", "Matchmaking", "ReadyCheck"].includes(phase);

/** Map any queue onto the ones Lookup offers. */
export function lookupQueue(q: Queue): Queue {
  if (q === "normal_blind" || q === "swiftplay") return "normal_draft";
  return q;
}

export type Rarity = "prismatic" | "gold" | "silver" | "other";
export function rarityOf(r: string): Rarity {
  const s = r.toLowerCase();
  if (s.includes("prism")) return "prismatic";
  if (s.includes("gold")) return "gold";
  if (s.includes("silver")) return "silver";
  return "other";
}
export const RARITY_LABEL: Record<Rarity, string> = {
  prismatic: "Prismatic",
  gold: "Gold",
  silver: "Silver",
  other: "Augment",
};

/** Gameflow phases during which the game itself is running. */
export const isInGamePhase = (phase: string) => ["GameStart", "InProgress", "Reconnect"].includes(phase);

export const RANKS: { value: string; label: string; group: "single" | "plus" }[] = [
  { value: "overall", label: "Overall (all ranks)", group: "single" },
  { value: "iron", label: "Iron", group: "single" },
  { value: "bronze", label: "Bronze", group: "single" },
  { value: "silver", label: "Silver", group: "single" },
  { value: "gold", label: "Gold", group: "single" },
  { value: "platinum", label: "Platinum", group: "single" },
  { value: "emerald", label: "Emerald", group: "single" },
  { value: "diamond", label: "Diamond", group: "single" },
  { value: "master", label: "Master", group: "single" },
  { value: "grandmaster", label: "Grandmaster", group: "single" },
  { value: "challenger", label: "Challenger", group: "single" },
  { value: "platinum_plus", label: "Platinum+", group: "plus" },
  { value: "emerald_plus", label: "Emerald+", group: "plus" },
  { value: "diamond_plus", label: "Diamond+", group: "plus" },
  { value: "diamond_2_plus", label: "Diamond 2+", group: "plus" },
  { value: "master_plus", label: "Master+", group: "plus" },
];

export const REGIONS: { value: string; label: string }[] = [
  { value: "world", label: "World" },
  { value: "na1", label: "NA" },
  { value: "euw1", label: "EUW" },
  { value: "eun1", label: "EUNE" },
  { value: "kr", label: "KR" },
  { value: "br1", label: "BR" },
  { value: "la1", label: "LAN" },
  { value: "la2", label: "LAS" },
  { value: "oc1", label: "OCE" },
  { value: "ru", label: "RU" },
  { value: "tr1", label: "TR" },
  { value: "jp1", label: "JP" },
  { value: "ph2", label: "PH" },
  { value: "sg2", label: "SG" },
  { value: "th2", label: "TH" },
  { value: "tw2", label: "TW" },
  { value: "vn2", label: "VN" },
  { value: "me1", label: "ME" },
];

export function rankLabel(key: string): string {
  const r = RANKS.find((r) => r.value === key);
  if (!r) return key;
  return key === "overall" ? "All ranks" : r.label;
}

export function regionLabel(key: string): string {
  return REGIONS.find((r) => r.value === key)?.label ?? key.toUpperCase();
}

/** "16_19" → "16.19" */
export const patchLabel = (p: string) => p.replace(/_/g, ".");

/** 0.5432 → "54.3%" */
export const pct = (wr: number) => `${(wr * 100).toFixed(1)}%`;

const nf = new Intl.NumberFormat("en-US");
export const num = (n: number) => nf.format(n);

/** Compact game count: 602 → "602", 18_420 → "18.4k" */
export function games(n: number): string {
  if (n >= 100_000) return `${Math.round(n / 1000)}k`;
  if (n >= 10_000) return `${(n / 1000).toFixed(1)}k`;
  return nf.format(n);
}

export const GOOD_WR = 0.52;
export const BAD_WR = 0.48;
export const LOW_SAMPLE = 100;

export function wrTone(wr: number): "good" | "bad" | "even" {
  if (wr >= GOOD_WR) return "good";
  if (wr <= BAD_WR) return "bad";
  return "even";
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

/** Loose search key: "Kog'Maw" → "kogmaw", "Dr. Mundo" → "drmundo". */
export const searchKey = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

/** Stat-shard rows (offense, flex, defense) as shown in the client. */
export const SHARD_ROWS: number[][] = [
  [5008, 5005, 5007],
  [5008, 5010, 5001],
  [5011, 5013, 5001],
];

/** "Imported runes & item set" style list joiner. */
export function joinAnd(parts: string[]): string {
  if (parts.length <= 1) return parts.join("");
  return `${parts.slice(0, -1).join(", ")} & ${parts[parts.length - 1]}`;
}

export function phaseLabel(phase: string): string {
  switch (phase) {
    case "Lobby":
      return "Lobby";
    case "Matchmaking":
      return "In queue";
    case "ReadyCheck":
      return "Ready check";
    case "ChampSelect":
      return "Champ select";
    case "GameStart":
    case "InProgress":
    case "Reconnect":
      return "In game";
    case "WaitingForStats":
    case "PreEndOfGame":
    case "EndOfGame":
      return "Post game";
    default:
      return "Home";
  }
}
