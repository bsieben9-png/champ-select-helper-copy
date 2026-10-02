// Mock backend used when the UI runs in a normal browser (`npm run dev`).
// Static data comes live from Data Dragon; stats are canned (Yorick vs Gwen
// uses real u.gg numbers) or generated deterministically.
//
// Dev URL params:
//   ?mock=champselect   (default) Ranked Solo/Duo, hovering Yorick top vs Gwen
//                       (build + counter picks vs Gwen below it until lock-in)
//   ?mock=draft         Normal Draft (400), same picks
//   ?mock=flex          Ranked Flex (440), Yorick locked
//   ?mock=trade         Yorick locked + auto-imported, then traded to Garen →
//                       "Your imported runes are for Yorick — Import Garen?"
//   ?mock=swiftplay     Swiftplay lobby: Yorick top + Ahri mid picked per position
//   ?mock=quickplay     the same as a Quickplay (490) lobby
//   ?mock=swiftplay-cs  Swiftplay's short skip-champ-select step (nothing to lock)
//   ?mock=counters      in champ select, nothing picked yet → counter picks
//   ?mock=locked        Yorick locked → fires an `auto-imported` event
//   ?mock=fullpages     like locked, but all rune pages are full → overwrite prompt
//   ?mock=late          Yorick locked + auto-imported before Gwen locked → Gwen
//                       locks later → "Import matchup build?" hint
//   ?mock=blind         in champ select, lane opponent unknown → tier list
//   ?mock=mayhem        ARAM Mayhem champ select (build + augments, auto-imported)
//   ?mock=mayhem-back   Mayhem: Yorick imported → bench swap Ashe (imported) →
//                       swap back to Yorick → "Import Yorick?" (no re-import)
//   ?mock=ingame        Mayhem champ select, then the game starts (InProgress)
//                       → Live keeps showing the build/augments
//   ?mock=aram          ARAM champ select
//   ?mock=lobby         client open, in lobby
//   ?mock=disconnected  League not running
//   ?mock=cycle         walks through the states above over ~15 s
//   ?fail=build|counters|matchups|static|all   make those calls reject
//   ?view=live|lookup|pool|settings            initial screen (read by App)
import type { Backend, Unlisten } from "./api";
import type {
  AllyPick,
  Build,
  ChampSelectState,
  ChampionInfo,
  Counter,
  EnemyPick,
  ImportResult,
  ItemInfo,
  ItemOption,
  LcuStatus,
  LobbyState,
  MatchupStat,
  Queue,
  Role,
  RuneInfo,
  RuneStyle,
  Settings,
  AugmentOption,
  SpellInfo,
  StaticData,
  TierEntry,
} from "./types";

const DD = "https://ddragon.leagueoflegends.com";
const params = new URLSearchParams(location.search);
const mode = params.get("mock") ?? "champselect";
const fail = new Set((params.get("fail") ?? "").split(",").filter(Boolean));

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const latency = () => sleep(180 + Math.random() * 320);

function maybeFail(what: string) {
  if (fail.has(what) || fail.has("all")) {
    throw "u.gg request failed: HTTP 503 Service Unavailable (stats2.u.gg)";
  }
}

// ---------------------------------------------------------------- roles
// Main roles for the mock only (the real app gets these from u.gg).
const R: Record<string, Role> = { t: "top", j: "jungle", m: "mid", a: "adc", s: "support" };
const ROLE_TABLE =
  "Aatrox:t Ahri:m Akali:mt Akshan:m Alistar:s Ambessa:t Amumu:js Anivia:m Annie:ms Aphelios:a " +
  "Ashe:as AurelionSol:m Aurora:mt Azir:m Bard:s Belveth:j Blitzcrank:s Brand:smj Braum:s Briar:j " +
  "Caitlyn:a Camille:t Cassiopeia:m Chogath:tm Corki:m Darius:t Diana:jm Draven:a DrMundo:t Ekko:mj " +
  "Elise:j Evelynn:j Ezreal:a Fiddlesticks:j Fiora:t Fizz:m Galio:ms Gangplank:t Garen:t Gnar:t " +
  "Gragas:jt Graves:j Gwen:t Hecarim:j Heimerdinger:ms Hwei:ms Illaoi:t Irelia:tm Ivern:j Janna:s " +
  "JarvanIV:j Jax:tj Jayce:tm Jhin:a Jinx:a Kaisa:a Kalista:a Karma:sm Karthus:jm Kassadin:m " +
  "Katarina:m Kayle:t Kayn:j Kennen:t Khazix:j Kindred:j Kled:t KogMaw:a KSante:t Leblanc:m LeeSin:j " +
  "Leona:s Lillia:j Lissandra:m Locke:m Lucian:am Lulu:s Lux:sm Malphite:t Malzahar:m Maokai:sj " +
  "MasterYi:j Mel:ms Milio:s MissFortune:a MonkeyKing:jt Mordekaiser:t Morgana:s Naafiri:m Nami:s " +
  "Nasus:t Nautilus:s Neeko:ms Nidalee:j Nilah:a Nocturne:j Nunu:j Olaf:tj Orianna:m Ornn:t " +
  "Pantheon:tsm Poppy:tjs Pyke:s Qiyana:mj Quinn:t Rakan:s Rammus:j RekSai:j Rell:s Renata:s " +
  "Renekton:t Rengar:jt Riven:t Rumble:t Ryze:m Samira:a Sejuani:j Senna:sa Seraphine:sa Sett:t " +
  "Shaco:j Shen:t Shyvana:j Singed:t Sion:t Sivir:a Skarner:j Smolder:a Sona:s Soraka:s Swain:sm " +
  "Sylas:m Syndra:m TahmKench:ts Taliyah:jm Talon:m Taric:s Teemo:t Thresh:s Tristana:a Trundle:tj " +
  "Tryndamere:t TwistedFate:m Twitch:a Udyr:j Urgot:t Varus:a Vayne:at Veigar:m Velkoz:sm Vex:m Vi:j " +
  "Viego:j Viktor:m Vladimir:mt Volibear:tj Warwick:jt Xayah:a Xerath:sm XinZhao:j Yasuo:mt Yone:mt " +
  "Yorick:t Yunara:a Yuumi:s Zaahen:t Zac:j Zed:m Zeri:a Ziggs:am Zilean:s Zoe:m Zyra:s";
const ROLE_MAP = new Map(
  ROLE_TABLE.split(" ").map((e) => {
    const [k, rs] = e.split(":");
    return [k, [...rs].map((c) => R[c])] as const;
  }),
);

function rolesFromTags(tags: string[]): Role[] {
  switch (tags[0]) {
    case "Marksman":
      return ["adc"];
    case "Support":
      return ["support"];
    case "Mage":
    case "Assassin":
      return ["mid"];
    default:
      return ["top"];
  }
}

// ---------------------------------------------------------------- static data
const SHARDS: [number, string, string, string][] = [
  [5008, "Adaptive Force", "StatModsAdaptiveForceIcon", "+9 Adaptive Force"],
  [5005, "Attack Speed", "StatModsAttackSpeedIcon", "+10% Attack Speed"],
  [5007, "Ability Haste", "StatModsCDRScalingIcon", "+8 Ability Haste"],
  [5010, "Move Speed", "StatModsMovementSpeedIcon", "+2% Move Speed"],
  [5001, "Health Scaling", "StatModsHealthScalingIcon", "+10-180 Health (based on level)"],
  [5011, "Health", "StatModsHealthPlusIcon", "+65 Health"],
  [5013, "Tenacity and Slow Resist", "StatModsTenacityIcon", "+10% Tenacity and Slow Resist"],
];

const stripHtml = (s: string) =>
  s
    .replace(/<br\s*\/?>/gi, " ")
    .replace(/<[^>]+>/g, "")
    .replace(/\s+/g, " ")
    .trim();

let staticP: Promise<StaticData> | null = null;

async function loadStatic(): Promise<StaticData> {
  const j = (u: string) =>
    fetch(u).then((r) => {
      if (!r.ok) throw `Data Dragon: HTTP ${r.status} for ${u}`;
      return r.json();
    });
  const versions: string[] = await j(`${DD}/api/versions.json`);
  const v = versions[0];
  const base = `${DD}/cdn/${v}`;
  const [champs, items, runes, summ] = await Promise.all([
    j(`${base}/data/en_US/champion.json`),
    j(`${base}/data/en_US/item.json`),
    j(`${base}/data/en_US/runesReforged.json`),
    j(`${base}/data/en_US/summoner.json`),
  ]);

  const champions: ChampionInfo[] = Object.values<any>(champs.data).map((c) => ({
    id: Number(c.key),
    key: c.id,
    name: c.name,
    icon: `${base}/img/champion/${c.image.full}`,
    roles: ROLE_MAP.get(c.id) ?? rolesFromTags(c.tags),
  }));

  const itemList: ItemInfo[] = Object.entries<any>(items.data).map(([id, it]) => ({
    id: Number(id),
    name: it.name,
    icon: `${base}/img/item/${it.image.full}`,
    gold: it.gold?.total ?? 0,
    description: stripHtml(it.description ?? ""),
  }));

  const rune_styles: RuneStyle[] = runes.map((s: any) => ({
    id: s.id,
    name: s.name,
    icon: `${DD}/cdn/img/${s.icon}`,
    slots: s.slots.map((sl: any) =>
      sl.runes.map(
        (r: any): RuneInfo => ({
          id: r.id,
          name: r.name,
          icon: `${DD}/cdn/img/${r.icon}`,
          short_desc: stripHtml(r.shortDesc ?? ""),
        }),
      ),
    ),
  }));

  const shards: RuneInfo[] = SHARDS.map(([id, name, icon, desc]) => ({
    id,
    name,
    icon: `${DD}/cdn/img/perk-images/StatMods/${icon}.png`,
    short_desc: desc,
  }));

  const spells: SpellInfo[] = Object.values<any>(summ.data).map((s) => ({
    id: Number(s.key),
    name: s.name,
    icon: `${base}/img/spell/${s.image.full}`,
  }));

  const [maj, min] = v.split(".");
  return {
    version: v,
    ugg_patch: `${maj}_${min}`,
    champions,
    items: itemList,
    rune_styles,
    shards,
    spells,
  };
}

const staticData = () => (staticP ??= loadStatic().catch((e) => ((staticP = null), Promise.reject(e))));

// ---------------------------------------------------------------- settings
const DEFAULT_SETTINGS: Settings = {
  auto_import: true,
  import_runes: true,
  import_item_set: true,
  rank: "emerald_plus",
  region: "world",
  min_games: 100,
  champion_pool: [83, 122, 54, 875, 516],
  counters_pool_only: false,
  source: "ugg",
};

function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem("csh-mock-settings");
    if (raw) return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
  } catch {
    /* storage unavailable */
  }
  return { ...DEFAULT_SETTINGS };
}

let settings = loadSettings();

// ---------------------------------------------------------------- builds
function rng(seed: number) {
  let a = seed >>> 0 || 1;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const ROLE_IDX: Record<Role, number> = { top: 1, jungle: 2, mid: 3, adc: 4, support: 5 };
const pick = <T>(r: () => number, xs: T[]): T => xs[Math.floor(r() * xs.length)];

function pickN<T>(r: () => number, xs: T[], n: number): T[] {
  const pool = [...xs];
  const out: T[] = [];
  while (out.length < n && pool.length) out.push(pool.splice(Math.floor(r() * pool.length), 1)[0]);
  return out;
}

const ITEM_POOLS: Record<string, number[]> = {
  fighter: [3078, 3053, 3071, 6610, 3161, 6333, 3074, 3748, 6631, 3073, 2501, 3181, 6609],
  tank: [3068, 2502, 3075, 3742, 3065, 6665, 3143, 3110, 2504, 4401, 6662, 3084],
  mage: [6655, 3089, 4645, 3135, 3157, 3118, 2503, 6653, 3165, 3102, 3137, 4646],
  marksman: [3031, 3032, 3046, 3036, 3094, 3085, 3072, 3508, 3153, 3124, 2512],
  assassin: [3142, 3814, 3179, 6333, 3156, 3026, 3071, 2520],
  support: [3190, 3050, 3109, 3107, 3222, 6620, 6617, 6616, 3504, 2065, 4005],
};
const BOOTS: Record<string, number[]> = {
  fighter: [3047, 3111],
  tank: [3047, 3111],
  mage: [3020, 3158],
  marksman: [3006],
  assassin: [3158, 3047],
  support: [3158, 3009, 3111],
};

function itemClass(champ: ChampionInfo | undefined, role: Role | null): string {
  if (role === "support") return "support";
  if (role === "adc") return "marksman";
  if (role === "mid" || champ?.roles[0] === "mid") return "mage";
  return "fighter";
}

const opts = (r: () => number, ids: number[], base: number): ItemOption[] =>
  ids
    .map((item_id) => ({
      item_id,
      games: Math.round(80 + r() * 1400),
      win_rate: base + (r() - 0.5) * 0.08,
    }))
    .sort((a, b) => b.games - a.games);

function skillOrder(priority: string): string[] {
  const order: string[] = [priority[0], priority[1], priority[2]];
  const pts: Record<string, number> = { Q: 0, W: 0, E: 0 };
  order.forEach((k) => pts[k]++);
  for (let lvl = 4; lvl <= 18; lvl++) {
    if (lvl === 6 || lvl === 11 || lvl === 16) {
      order.push("R");
      continue;
    }
    const k = [...priority].find((s) => pts[s] < 5)!;
    pts[k]++;
    order.push(k);
  }
  return order;
}

const YORICK = 83;
const GWEN = 887;
const GAREN = 86;

// Fake ARAM Mayhem augments. Icons are left empty on purpose so the UI's
// letter-badge fallback is exercised (real icons aren't on ddragon).
const AUGMENTS: [string, string, string][] = [
  ["Goliath", "prismatic", "Grow much larger, gaining bonus health and adaptive force."],
  ["Infernal Conduit", "prismatic", "Burning enemies refunds part of your basic ability cooldowns."],
  ["Jeweled Gauntlet", "prismatic", "Your abilities can critically strike."],
  ["Back to Basics", "prismatic", "Your ultimate is disabled; your other abilities deal much more damage."],
  ["Courage of the Colossus", "gold", "Gain a shield after immobilizing an enemy champion."],
  ["Tank Engine", "gold", "Takedowns permanently increase your size and health."],
  ["Mystic Punch", "gold", "Attacks reduce your ability cooldowns."],
  ["Phantom Step", "gold", "Dashing grants a short burst of move speed."],
  ["Witchful Thinking", "silver", "Gain ability power."],
  ["Firebrand", "silver", "Attacks burn enemies over a few seconds."],
  ["Ice Cold", "silver", "Your slows are stronger."],
  ["Deft", "silver", "Gain attack speed."],
];

function mockAugments(championId: number): AugmentOption[] {
  const r = rng(championId * 4099 + 17);
  // Like the real data: a ranking only (Riot policy forbids augment win rates).
  const order = AUGMENTS.map((_, i) => ({ i, key: r() }));
  const rank = new Map(order.map((o) => [o.i, o.key]));
  return AUGMENTS.map(([name, rarity, description], i) => ({
    id: 1000 + i,
    name,
    icon: "",
    rarity,
    description,
    games: 0,
    win_rate: 0,
    pick_rate: 0,
  })).sort((a, b) => rank.get(b.id - 1000)! - rank.get(a.id - 1000)!);
}

function yorickBuild(): Omit<Build, "opponent_id" | "games" | "win_rate" | "fell_back_to_general" | "queue" | "role"> {
  return {
    source: "ugg",
    champion_id: YORICK,
    patch: "",
    rank: settings.rank,
    region: settings.region,
    runes: {
      primary_style: 8400,
      sub_style: 8000,
      perks: [8437, 8446, 8429, 8451, 9111, 9105],
      shards: [5008, 5008, 5001],
      games: 588,
      win_rate: 0.548,
    },
    spells: { ids: [4, 12], games: 571, win_rate: 0.545 },
    starting_items: { items: [1054, 2003], games: 402, win_rate: 0.551 },
    core_items: { items: [3078, 3047, 6694], games: 214, win_rate: 0.579 },
    fourth_items: [
      { item_id: 3053, games: 96, win_rate: 0.594 },
      { item_id: 6694, games: 71, win_rate: 0.563 },
      { item_id: 3071, games: 58, win_rate: 0.552 },
      { item_id: 6333, games: 41, win_rate: 0.537 },
    ],
    fifth_items: [
      { item_id: 3071, games: 52, win_rate: 0.615 },
      { item_id: 6333, games: 38, win_rate: 0.579 },
      { item_id: 3065, games: 27, win_rate: 0.481 },
    ],
    sixth_items: [
      { item_id: 3026, games: 24, win_rate: 0.625 },
      { item_id: 3742, games: 18, win_rate: 0.556 },
      { item_id: 3075, games: 15, win_rate: 0.467 },
    ],
    skill_order: "Q E W Q Q R Q E Q E R E E W W R W W".split(" "),
    skill_priority: "QEW",
    available_roles: ["top"],
    augments: [],
  };
}

async function genBuild(
  championId: number,
  role: Role | null,
  opponentId: number | null,
  queue: Queue,
): Promise<Build> {
  const sd = await staticData();
  const champ = sd.champions.find((c) => c.id === championId);
  const name = champ?.name ?? `Champion ${championId}`;
  const aram = queue === "aram" || queue === "aram_mayhem";
  const roles = champ?.roles.length ? champ.roles : (["top"] as Role[]);
  const effRole: Role | null = aram ? null : (role ?? roles[0]);
  if (effRole && !roles.includes(effRole)) {
    throw `No ${effRole} data for ${name} on this patch (too few games)`;
  }
  const patch = sd.ugg_patch;
  const r = rng(championId * 97 + (effRole ? ROLE_IDX[effRole] : 9) * 7 + (aram ? 13 : 0));

  // Matchup sample: real numbers for Yorick vs Gwen, generated otherwise.
  const general = { games: Math.round(8000 + r() * 60000), win_rate: 0.485 + r() * 0.045 };
  let stats = general;
  let fell = false;
  let opp: number | null = null;
  if (opponentId && !aram) {
    opp = opponentId;
    if (championId === YORICK && opponentId === GWEN) {
      stats = { games: 602, win_rate: 327 / 602 };
    } else {
      const mr = rng(championId * 1009 + opponentId);
      const g = Math.round(20 + mr() * 1500);
      if (g < 150) fell = true;
      else stats = { games: g, win_rate: 0.44 + mr() * 0.12 };
    }
  }

  if (championId === YORICK && !aram) {
    const y = yorickBuild();
    const ys = opp && !fell ? stats : { games: 48210, win_rate: 0.516 };
    return {
      ...y,
      patch,
      role: effRole,
      queue,
      opponent_id: opp,
      ...ys,
      fell_back_to_general: fell,
      available_roles: roles,
    };
  }

  const styles = sd.rune_styles;
  const prim = pick(r, styles);
  const sub = pick(
    r,
    styles.filter((s) => s.id !== prim.id),
  );
  const subRows = pickN(r, [1, 2, 3], 2).sort();
  const perks = [
    pick(r, prim.slots[0]).id,
    pick(r, prim.slots[1]).id,
    pick(r, prim.slots[2]).id,
    pick(r, prim.slots[3]).id,
    ...subRows.map((i) => pick(r, sub.slots[i]).id),
  ];
  const shards = [pick(r, [5008, 5005, 5007]), pick(r, [5008, 5010, 5001]), pick(r, [5011, 5013, 5001])];

  const cls = itemClass(champ, effRole);
  const have = new Set(sd.items.map((i) => i.id));
  const pool = ITEM_POOLS[cls].filter((i) => have.has(i));
  const boots = pick(r, BOOTS[cls]);
  const [c1, c2, ...rest] = pickN(r, pool, 11);
  const second: Record<string, number> = { top: 12, jungle: 11, mid: 14, adc: 7, support: 14 };
  const starting: Record<string, number[]> = {
    top: [1055, 2003],
    jungle: [pick(r, [1101, 1102, 1103]), 2003],
    mid: [1056, 2003],
    adc: [1055, 2003],
    support: [3865, 2003],
  };
  const priority = pick(r, ["QEW", "QWE", "EQW", "WQE", "QEW"]);
  const base = stats.win_rate;

  return {
    source: "ugg",
    champion_id: championId,
    role: effRole,
    opponent_id: opp,
    queue,
    patch,
    rank: aram ? "overall" : settings.rank,
    region: settings.region,
    games: stats.games,
    win_rate: stats.win_rate,
    fell_back_to_general: fell,
    runes: { primary_style: prim.id, sub_style: sub.id, perks, shards, games: Math.round(stats.games * 0.6), win_rate: base + 0.01 },
    spells: {
      ids: aram ? [4, 32] : [4, second[effRole ?? "top"]],
      games: Math.round(stats.games * 0.8),
      win_rate: base + 0.004,
    },
    starting_items: {
      items: aram ? [1055, 2003, 2003] : starting[effRole ?? "top"],
      games: Math.round(stats.games * 0.5),
      win_rate: base + 0.006,
    },
    core_items: { items: [c1, boots, c2], games: Math.round(stats.games * 0.2), win_rate: base + 0.03 },
    fourth_items: opts(r, rest.slice(0, 4), base + 0.03),
    fifth_items: opts(r, rest.slice(3, 7), base + 0.04),
    sixth_items: opts(r, rest.slice(6, 10), base + 0.05),
    skill_order: skillOrder(priority),
    skill_priority: priority,
    available_roles: aram ? [] : roles,
    augments: queue === "aram_mayhem" ? mockAugments(championId) : [],
  };
}

// ---------------------------------------------------------------- counters / matchups
// Counter picks vs Gwen top (WR = the counter's win rate vs Gwen).
const GWEN_COUNTERS: [number, number, number][] = [
  [122, 0.551, 1840],
  [YORICK, 327 / 602, 602],
  [86, 0.538, 2210],
  [68, 0.536, 74],
  [6, 0.535, 412],
  [54, 0.532, 1530],
  [875, 0.529, 1320],
  [133, 0.526, 238],
  [516, 0.524, 880],
  [14, 0.522, 940],
  [420, 0.52, 455],
  [58, 0.518, 1210],
  [98, 0.515, 640],
  [266, 0.512, 2450],
  [24, 0.51, 1980],
  [92, 0.507, 1100],
  [17, 0.505, 760],
  [39, 0.499, 900],
  [150, 0.491, 520],
  [897, 0.478, 680],
];

async function matchupRows(championId: number, role: Role): Promise<MatchupStat[]> {
  const sd = await staticData();
  const rows: MatchupStat[] = [];
  for (const c of sd.champions) {
    if (c.id === championId || !c.roles.includes(role)) continue;
    const r = rng(Math.min(championId, c.id) * 7919 + Math.max(championId, c.id));
    const games = Math.round(25 + r() ** 1.6 * 3200);
    let wr = 0.5 + (r() - 0.5) * 0.14;
    if (championId > c.id) wr = 1 - wr; // keep A-vs-B consistent with B-vs-A
    rows.push({ opponent_id: c.id, games, wins: Math.round(games * wr), win_rate: wr });
  }
  if (championId === YORICK && role === "top") {
    const i = rows.findIndex((m) => m.opponent_id === GWEN);
    const gwen = { opponent_id: GWEN, games: 602, wins: 327, win_rate: 327 / 602 };
    if (i >= 0) rows[i] = gwen;
    else rows.push(gwen);
  }
  return rows;
}

async function counters(enemyId: number, role: Role): Promise<Counter[]> {
  let list: Counter[];
  if (enemyId === GWEN && role === "top") {
    list = GWEN_COUNTERS.map(([champion_id, win_rate, games]) => ({ champion_id, win_rate, games, in_pool: false }));
  } else {
    list = (await matchupRows(enemyId, role)).map((m) => ({
      champion_id: m.opponent_id,
      games: m.games,
      win_rate: 1 - m.win_rate,
      in_pool: false,
    }));
  }
  return list
    .map((c) => ({ ...c, in_pool: settings.champion_pool.includes(c.champion_id) }))
    .filter((c) => c.games >= settings.min_games)
    .filter((c) => !settings.counters_pool_only || c.in_pool)
    .sort((a, b) => b.win_rate - a.win_rate);
}

async function tierList(role: Role): Promise<TierEntry[]> {
  const sd = await staticData();
  const rows: TierEntry[] = [];
  for (const c of sd.champions) {
    if (!c.roles.includes(role)) continue;
    const r = rng(c.id * 31 + ROLE_IDX[role] * 1000);
    const main = c.roles[0] === role;
    const pick_rate = (main ? 0.012 : 0.002) + r() ** 2 * (main ? 0.13 : 0.03);
    const games = Math.round(pick_rate * 420000);
    const win_rate = c.id === YORICK && role === "top" ? 0.516 : 0.468 + r() * 0.07;
    const ban_rate = r() ** 3 * 0.32;
    rows.push({
      champion_id: c.id,
      games,
      win_rate,
      pick_rate,
      ban_rate,
      in_pool: settings.champion_pool.includes(c.id),
    });
  }
  // "Best first": win rate, nudged by how often it is picked.
  const score = (t: TierEntry) => t.win_rate + Math.min(t.pick_rate, 0.1) * 0.12;
  return rows.filter((t) => t.games >= settings.min_games).sort((a, b) => score(b) - score(a));
}

// ---------------------------------------------------------------- champ select
const NOT_IN_CS: ChampSelectState = {
  in_champ_select: false,
  queue_id: null,
  queue: null,
  my_role: null,
  my_champion_id: null,
  my_champion_locked: false,
  allies: [],
  enemies: [],
  lane_opponent_id: null,
  bans: [],
};

const enemies: EnemyPick[] = [
  { champion_id: GWEN, role: "top", role_inferred: true },
  { champion_id: 64, role: "jungle", role_inferred: true },
  { champion_id: 103, role: "mid", role_inferred: true },
  { champion_id: 222, role: "adc", role_inferred: true },
  { champion_id: 412, role: "support", role_inferred: true },
];

function allies(me: number): AllyPick[] {
  return [
    { champion_id: me, role: "top", is_me: true },
    { champion_id: 234, role: "jungle", is_me: false },
    { champion_id: 0, role: "mid", is_me: false },
    { champion_id: 145, role: "adc", is_me: false },
    { champion_id: 111, role: "support", is_me: false },
  ];
}

const NO_PICK: EnemyPick = { champion_id: 0, role: null, role_inferred: false };

/** `oppKnown: false` = blind/first pick: the enemy top hasn't picked yet. */
function rankedCs(me: number, locked: boolean, oppKnown = true): ChampSelectState {
  return {
    in_champ_select: true,
    queue_id: 420,
    queue: "ranked_solo",
    my_role: "top",
    my_champion_id: me || null,
    my_champion_locked: locked,
    allies: allies(me),
    enemies: oppKnown ? enemies : [NO_PICK, enemies[1], enemies[2], NO_PICK, NO_PICK],
    lane_opponent_id: oppKnown ? GWEN : null,
    bans: [157, 238, 555, 266, 119, 82, 10, 0],
  };
}

function withQueue(cs: ChampSelectState, queue_id: number, queue: Queue): ChampSelectState {
  return { ...cs, queue_id, queue };
}

/** Locked Yorick, then traded with the ally Garen (pick actions unchanged). */
function tradedCs(): ChampSelectState {
  const cs = rankedCs(YORICK, true);
  return {
    ...cs,
    my_champion_id: GAREN,
    allies: cs.allies.map((a) => (a.is_me ? { ...a, champion_id: GAREN } : a.champion_id === 0 ? { ...a, champion_id: YORICK } : a)),
  };
}

/** Swiftplay's skip-champ-select step: champions from the lobby, nothing to lock. */
const SWIFTPLAY_CS: ChampSelectState = {
  ...rankedCs(YORICK, false),
  queue_id: 480,
  queue: "swiftplay",
  enemies: [NO_PICK, NO_PICK, NO_PICK, NO_PICK, NO_PICK],
  lane_opponent_id: null,
  bans: [],
};

// ARAM: the client hides the enemy team during champ select.
const ARAM_CS: ChampSelectState = {
  in_champ_select: true,
  queue_id: 450,
  queue: "aram",
  my_role: null,
  my_champion_id: 115,
  my_champion_locked: true,
  allies: [115, 22, 54, 99, 86].map((id, i) => ({ champion_id: id, role: null, is_me: i === 0 })),
  enemies: [],
  lane_opponent_id: null,
  bans: [],
};

const MAYHEM_CS: ChampSelectState = {
  ...ARAM_CS,
  queue_id: 2400,
  queue: "aram_mayhem",
  my_champion_id: YORICK,
  my_champion_locked: true,
  allies: [YORICK, 22, 54, 99, 86].map((id, i) => ({ champion_id: id, role: null, is_me: i === 0 })),
};

const SUMMONER = "DeadManWalking#EUW";
const NO_LOBBY: LobbyState = { in_lobby: false, queue_id: null, queue: null, slots: [] };
const swiftLobby = (queue_id: number): LobbyState => ({
  in_lobby: true,
  queue_id,
  queue: "swiftplay",
  slots: [
    { index: 0, champion_id: YORICK, role: "top" },
    { index: 1, champion_id: 103, role: "mid" },
  ],
});
type MockState = { lcu: LcuStatus; cs: ChampSelectState; lobby?: LobbyState };
const offline: MockState = { lcu: { connected: false, summoner_name: null, phase: "None" }, cs: NOT_IN_CS };
const lobby: MockState = {
  lcu: { connected: true, summoner_name: SUMMONER, phase: "Lobby" },
  cs: NOT_IN_CS,
  lobby: { in_lobby: true, queue_id: 420, queue: "ranked_solo", slots: [] },
};
const inLobby = (queue_id: number): MockState => ({
  lcu: { connected: true, summoner_name: SUMMONER, phase: "Lobby" },
  cs: NOT_IN_CS,
  lobby: swiftLobby(queue_id),
});
const inCs = (cs: ChampSelectState): MockState => ({
  lcu: { connected: true, summoner_name: SUMMONER, phase: "ChampSelect" },
  cs,
});

const STATES: Record<string, MockState> = {
  disconnected: offline,
  lobby,
  counters: inCs(rankedCs(0, false)),
  champselect: inCs(rankedCs(YORICK, false)),
  draft: inCs(withQueue(rankedCs(YORICK, false), 400, "normal_draft")),
  flex: inCs(withQueue(rankedCs(YORICK, true), 440, "ranked_flex")),
  trade: inCs(rankedCs(YORICK, true)),
  swiftplay: inLobby(480),
  quickplay: inLobby(490),
  "swiftplay-cs": inCs(SWIFTPLAY_CS),
  "mayhem-back": inCs(MAYHEM_CS),
  locked: inCs(rankedCs(YORICK, true)),
  fullpages: inCs(rankedCs(YORICK, true)),
  late: inCs(rankedCs(YORICK, true, false)),
  blind: inCs(rankedCs(0, false, false)),
  aram: inCs(ARAM_CS),
  mayhem: inCs(MAYHEM_CS),
  ingame: inCs(MAYHEM_CS),
};
const inGame: MockState = { lcu: { connected: true, summoner_name: SUMMONER, phase: "InProgress" }, cs: NOT_IN_CS };

// ---------------------------------------------------------------- backend
export function createMockBackend(): Backend {
  const handlers = new Map<string, Set<(p: unknown) => void>>();
  const emit = (event: string, payload: unknown) => handlers.get(event)?.forEach((h) => h(payload));
  let state: MockState = STATES[mode] ?? STATES.champselect;

  const setState = (s: MockState) => {
    state = s;
    emit("lcu-status", s.lcu);
    emit("champ-select", s.cs);
    emit("lobby", s.lobby ?? NO_LOBBY);
  };
  const imported = (champion_id: number, opponent_id: number | null, page: string) =>
    emit("auto-imported", {
      champion_id,
      opponent_id,
      result: {
        runes: true,
        item_set: true,
        messages: [`Runes: set page "${page}".`, `Item set: saved "${page}".`],
        needs_confirmation: null,
      },
    });
  const fullPages = mode === "fullpages";
  const FULL_PAGE = { id: 1987, name: "Ranked Top" };
  const autoImport = (opponent: number | null = GWEN) =>
    emit("auto-imported", {
      champion_id: YORICK,
      opponent_id: opponent,
      result: fullPages
        ? { runes: false, item_set: true, messages: ["All rune pages are full"], needs_confirmation: FULL_PAGE }
        : {
            runes: true,
            item_set: true,
            // The real backend always says what it did.
            messages: ['Runes: set page "CSH: Yorick vs Gwen".', 'Item set: saved "CSH: Yorick vs Gwen".'],
            needs_confirmation: null,
          },
    });

  if (mode === "locked" || fullPages) setTimeout(autoImport, 1500);
  if (mode === "ingame") setTimeout(() => setState(inGame), 1000);
  if (mode === "mayhem" || mode === "ingame") setTimeout(() => imported(YORICK, null, "CSH: Yorick ARAM"), 600);
  if (mode === "trade") {
    setTimeout(() => imported(YORICK, GWEN, "CSH: Yorick vs Gwen"), 600);
    setTimeout(() => setState(inCs(tradedCs())), 2500);
  }
  if (mode === "mayhem-back") {
    const ashe = { ...MAYHEM_CS, my_champion_id: 22, allies: MAYHEM_CS.allies.map((a) => (a.is_me ? { ...a, champion_id: 22 } : a)) };
    setTimeout(() => imported(YORICK, null, "CSH: Yorick ARAM"), 500);
    setTimeout(() => setState(inCs(ashe)), 1500);
    setTimeout(() => imported(22, null, "CSH: Ashe ARAM"), 2200);
    // Back to Yorick: already imported in this champ select → no auto-import.
    setTimeout(() => setState(inCs(MAYHEM_CS)), 3500);
  }
  if (mode === "late") {
    setTimeout(() => autoImport(null), 1200);
    setTimeout(() => setState(STATES.locked), 4000);
  }
  if (mode === "cycle") {
    const seq: [number, () => void][] = [
      [2500, () => setState(lobby)],
      [5000, () => setState(STATES.counters)],
      [9000, () => setState(STATES.champselect)],
      [12500, () => setState(STATES.locked)],
      [13500, autoImport],
    ];
    state = offline;
    seq.forEach(([t, f]) => setTimeout(f, t));
  }

  const commands: Record<string, (a: any) => Promise<unknown>> = {
    async get_static_data() {
      maybeFail("static");
      return staticData();
    },
    async get_build(a) {
      await latency();
      maybeFail("build");
      return genBuild(a.championId, a.role ?? null, a.opponentId ?? null, a.queue);
    },
    async get_counters(a) {
      await latency();
      maybeFail("counters");
      return counters(a.enemyId, a.role);
    },
    async get_matchups(a) {
      await latency();
      maybeFail("matchups");
      return matchupRows(a.championId, a.role);
    },
    async get_tier_list(a) {
      await latency();
      maybeFail("tiers");
      return tierList(a.role);
    },
    async get_settings() {
      return { ...settings };
    },
    async save_settings(a) {
      settings = { ...a.newSettings };
      try {
        localStorage.setItem("csh-mock-settings", JSON.stringify(settings));
      } catch {
        /* ignore */
      }
    },
    async get_champ_select() {
      return state.cs;
    },
    async get_lcu_status() {
      return state.lcu;
    },
    async get_lobby() {
      return state.lobby ?? NO_LOBBY;
    },
    async import_build(a): Promise<ImportResult> {
      await sleep(600);
      if (!state.lcu.connected) throw "League client is not running";
      if (fullPages && settings.import_runes && a.overwritePageId == null) {
        return {
          runes: false,
          item_set: settings.import_item_set,
          messages: ["All rune pages are full"],
          needs_confirmation: FULL_PAGE,
        };
      }
      // Like the real backend (lcu.rs import_build): it always says what it did.
      const messages = [
        ...(settings.import_runes ? ['Runes: set page "CSH: …".'] : []),
        ...(settings.import_item_set ? ['Item set: saved "CSH: …".'] : []),
      ];
      return {
        runes: settings.import_runes,
        item_set: settings.import_item_set,
        messages,
        needs_confirmation: null,
      };
    },
  };

  return {
    async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
      const f = commands[cmd];
      if (!f) throw `mock: unknown command ${cmd}`;
      return (await f(args ?? {})) as T;
    },
    async listen<T>(event: string, cb: (payload: T) => void): Promise<Unlisten> {
      let set = handlers.get(event);
      if (!set) handlers.set(event, (set = new Set()));
      const h = cb as (p: unknown) => void;
      set.add(h);
      return () => set!.delete(h);
    },
  };
}
