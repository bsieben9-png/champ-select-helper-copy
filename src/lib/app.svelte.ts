// Global app state (Svelte 5 runes). One instance, imported as `app`.
import * as api from "./api";
import { errorMessage, joinAnd, lookupQueue } from "./format";
import type {
  AutoImportEvent,
  Build,
  ChampSelectState,
  ChampionInfo,
  ImportResult,
  ItemInfo,
  LcuStatus,
  Queue,
  Role,
  RuneInfo,
  RunePageRef,
  RuneStyle,
  Settings,
  SpellInfo,
  StaticData,
} from "./types";

export type View = "live" | "lookup" | "pool" | "settings";
export type LookupTab = "build" | "counters" | "matchups" | "tiers";
export const VIEWS: View[] = ["live", "lookup", "pool", "settings"];

export type ToastKind = "success" | "error" | "warn" | "info";
export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  detail?: string;
  /** Small label above the title, e.g. "Auto-import". */
  tag?: string;
}

const EMPTY_CS: ChampSelectState = {
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

/** Settings keys that change what the stats endpoints return. */
const STAT_KEYS: (keyof Settings)[] = [
  "source",
  "rank",
  "region",
  "min_games",
  "champion_pool",
  "counters_pool_only",
];

class AppState {
  view = $state<View>("live");

  staticData = $state.raw<StaticData | null>(null);
  staticError = $state<string | null>(null);
  settings = $state.raw<Settings | null>(null);
  lcu = $state.raw<LcuStatus>({ connected: false, summoner_name: null, phase: "None" });
  cs = $state.raw<ChampSelectState>(EMPTY_CS);
  toasts = $state<Toast[]>([]);
  /** Pending "all rune pages are full — overwrite?" question. */
  overwrite = $state.raw<{
    page: RunePageRef;
    championId: number;
    opponentId: number | null;
    getBuild: () => Promise<Build>;
  } | null>(null);
  overwriting = $state(false);
  /** Last build imported in this champ select (auto or manual). Auto-import
   *  runs once at lock-in; Live uses this to offer a manual re-import when
   *  the lane opponent only becomes known afterwards. */
  imported = $state.raw<{ championId: number; opponentId: number | null; auto: boolean } | null>(null);
  /** My champion/queue from the last champ select — the build stays visible
   *  in Live while the game runs (Mayhem augments are picked mid-game). */
  lastPick = $state.raw<{ championId: number; queue: Queue; role: Role | null; opponentId: number | null } | null>(
    null,
  );
  /** Bumped whenever stat-affecting settings change → views refetch. */
  statsVersion = $state(0);
  /** Time of the last successful settings save (for a "Saved" hint). */
  savedAt = $state(0);

  /** Lookup screen selection (kept here so it survives tab switches). */
  lookup = $state<{
    championId: number | null;
    role: Role | null;
    opponentId: number | null;
    queue: Queue;
    tab: LookupTab;
  }>({ championId: null, role: null, opponentId: null, queue: "ranked_solo", tab: "build" });

  champs = $derived(new Map<number, ChampionInfo>((this.staticData?.champions ?? []).map((c) => [c.id, c])));
  champList = $derived([...(this.staticData?.champions ?? [])].sort((a, b) => a.name.localeCompare(b.name)));
  items = $derived(new Map<number, ItemInfo>((this.staticData?.items ?? []).map((i) => [i.id, i])));
  spells = $derived(new Map<number, SpellInfo>((this.staticData?.spells ?? []).map((s) => [s.id, s])));
  styles = $derived(new Map<number, RuneStyle>((this.staticData?.rune_styles ?? []).map((s) => [s.id, s])));
  shards = $derived(new Map<number, RuneInfo>((this.staticData?.shards ?? []).map((s) => [s.id, s])));
  pool = $derived(new Set(this.settings?.champion_pool ?? []));

  #toastId = 0;
  #saveChain: Promise<unknown> = Promise.resolve();

  champName(id: number | null | undefined): string {
    if (!id) return "";
    return this.champs.get(id)?.name ?? `#${id}`;
  }

  async init() {
    await Promise.all([
      api.onLcuStatus((s) => (this.lcu = s)),
      api.onChampSelect((s) => this.#setChampSelect(s)),
      api.onAutoImported((e) => this.#onAutoImported(e)),
    ]).catch((e) => this.toast("error", "Couldn't subscribe to app events", errorMessage(e)));

    api.getSettings().then(
      (s) => (this.settings = s),
      (e) => this.toast("error", "Couldn't load settings", errorMessage(e)),
    );
    api.getLcuStatus().then((s) => (this.lcu = s), () => {});
    api.getChampSelect().then((s) => this.#setChampSelect(s), () => {});
    this.loadStatic();
  }

  #setChampSelect(s: ChampSelectState) {
    if (s.in_champ_select) {
      if (s.my_champion_id) {
        this.lastPick = {
          championId: s.my_champion_id,
          queue: s.queue ?? "ranked_solo",
          role: s.my_role,
          opponentId: s.lane_opponent_id,
        };
      }
    } else {
      this.imported = null;
    }
    this.cs = s;
  }

  loadStatic() {
    this.staticError = null;
    api.getStaticData().then(
      (d) => (this.staticData = d),
      (e) => (this.staticError = errorMessage(e)),
    );
  }

  /** Change settings and save immediately. Saves are serialized. */
  updateSettings(patch: Partial<Settings>) {
    const prev = this.settings;
    if (!prev) return;
    const next = { ...prev, ...patch };
    this.settings = next;
    const statsChanged = STAT_KEYS.some((k) => JSON.stringify(prev[k]) !== JSON.stringify(next[k]));
    this.#saveChain = this.#saveChain.then(() =>
      api.saveSettings(next).then(
        () => {
          this.savedAt = Date.now();
          if (statsChanged) {
            api.clearStatsCache();
            this.statsVersion++;
          }
        },
        (e) => {
          this.settings = prev;
          this.toast("error", "Couldn't save settings", errorMessage(e));
        },
      ),
    );
  }

  togglePool(id: number) {
    const pool = this.settings?.champion_pool ?? [];
    this.updateSettings({
      champion_pool: pool.includes(id) ? pool.filter((p) => p !== id) : [...pool, id],
    });
  }

  toast(kind: ToastKind, title: string, detail?: string, tag?: string) {
    const id = ++this.#toastId;
    this.toasts.push({ id, kind, title, detail, tag });
    setTimeout(() => this.dismiss(id), kind === "error" ? 9000 : 6000);
  }

  dismiss(id: number) {
    const i = this.toasts.findIndex((t) => t.id === id);
    if (i >= 0) this.toasts.splice(i, 1);
  }

  /** "Yorick vs Gwen" / "Yorick" */
  matchupLabel(championId: number, opponentId: number | null | undefined) {
    const me = this.champName(championId);
    return opponentId ? `${me} vs ${this.champName(opponentId)}` : me;
  }

  reportImport(
    result: ImportResult,
    championId: number,
    opponentId: number | null,
    auto: boolean,
    getBuild: () => Promise<Build>,
  ) {
    const parts = [result.runes && "runes", result.item_set && "item set"].filter((p): p is string => !!p);
    const label = this.matchupLabel(championId, opponentId);
    const tag = auto ? "Auto-import" : undefined;
    if (this.cs.in_champ_select) this.imported = { championId, opponentId, auto };
    if (result.needs_confirmation) {
      // Rune pages are full: ask first (modal), report the rest now.
      this.overwrite = { page: result.needs_confirmation, championId, opponentId, getBuild };
      if (parts.length) this.toast("success", `Imported ${joinAnd(parts)} for ${label}`, undefined, tag);
      return;
    }
    const detail = result.messages.join(" · ") || undefined;
    if (parts.length) {
      this.toast(result.messages.length ? "warn" : "success", `Imported ${joinAnd(parts)} for ${label}`, detail, tag);
    } else {
      this.toast("error", `Couldn't import for ${label}`, detail ?? "Nothing was imported.", tag);
    }
  }

  async importBuild(build: Build) {
    try {
      const res = await api.importBuild(build);
      this.reportImport(res, build.champion_id, build.opponent_id, false, async () => build);
    } catch (e) {
      this.toast("error", "Import failed", errorMessage(e));
    }
  }

  #onAutoImported(e: AutoImportEvent) {
    // Read role/queue now, not when the "rune pages full" question is
    // answered: by then champ select may be over (cs reset → wrong queue/role).
    const role = this.cs.my_role;
    const queue = this.cs.queue ?? "ranked_solo";
    this.reportImport(e.result, e.champion_id, e.opponent_id, true, () =>
      api.getBuild(e.champion_id, role, e.opponent_id, queue),
    );
  }

  /** User confirmed overwriting a rune page. */
  async confirmOverwrite() {
    const o = this.overwrite;
    if (!o || this.overwriting) return;
    this.overwriting = true;
    try {
      const build = await o.getBuild();
      const res = await api.importBuild(build, o.page.id);
      this.overwrite = null;
      // Never loop back into another confirmation.
      this.reportImport({ ...res, needs_confirmation: null }, o.championId, o.opponentId, false, o.getBuild);
    } catch (e) {
      this.overwrite = null;
      this.toast("error", "Couldn't overwrite rune page", errorMessage(e));
    } finally {
      this.overwriting = false;
    }
  }

  cancelOverwrite() {
    this.overwrite = null;
  }

  /** Open Lookup preloaded with a champion (and optional opponent/role). */
  openLookup(championId: number, opponentId: number | null = null, role: Role | null = null, queue?: Queue) {
    const champRole = this.champs.get(championId)?.roles[0] ?? null;
    // Mutate in place: views hold a reference to this proxy.
    Object.assign(this.lookup, {
      championId,
      opponentId,
      role: role ?? champRole,
      queue: lookupQueue(queue ?? this.lookup.queue),
      tab: "build",
    });
    this.view = "lookup";
  }
}

export const app = new AppState();
