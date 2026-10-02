// Typed wrappers around the Tauri commands/events in src-tauri/src/lib.rs.
// Outside Tauri (plain `npm run dev` in a browser) a mock backend is loaded
// instead — see mock.ts (it is code-split and never shipped to Tauri users).
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import type {
  AutoImportEvent,
  Build,
  ChampSelectState,
  Counter,
  ImportResult,
  LcuStatus,
  MatchupStat,
  Queue,
  Role,
  Settings,
  StaticData,
  TierEntry,
} from "./types";

export type Unlisten = () => void;

export interface Backend {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, cb: (payload: T) => void): Promise<Unlisten>;
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export const isMock = !inTauri;

let backendP: Promise<Backend> | null = null;

function backend(): Promise<Backend> {
  backendP ??= inTauri
    ? Promise.resolve<Backend>({
        invoke: (cmd, args) => tauriInvoke(cmd, args),
        listen: (event, cb) => tauriListen(event, (e) => cb(e.payload as never)),
      })
    : import("./mock").then((m) => m.createMockBackend());
  return backendP;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return (await backend()).invoke<T>(cmd, args);
}

// Small in-memory cache for stats lookups: the most recently used
// MAX_CACHED answers (a long session must not grow the webview's memory
// without bound). Results depend on settings (rank/region/min games/pool),
// so the cache is dropped whenever they change.
const MAX_CACHED = 60;
const cache = new Map<string, Promise<unknown>>();

function cached<T>(key: string, load: () => Promise<T>): Promise<T> {
  const hit = cache.get(key);
  if (hit) {
    // Map keeps insertion order: move to the end = most recently used.
    cache.delete(key);
    cache.set(key, hit);
    return hit as Promise<T>;
  }
  const p = load();
  cache.set(key, p);
  if (cache.size > MAX_CACHED) cache.delete(cache.keys().next().value as string);
  // Never cache failures (but don't drop a newer entry for the same key).
  p.catch(() => cache.get(key) === p && cache.delete(key));
  return p;
}

export const clearStatsCache = () => cache.clear();

export const getStaticData = () => call<StaticData>("get_static_data");

export const getBuild = (
  championId: number,
  role: Role | null,
  opponentId: number | null,
  queue: Queue,
) =>
  cached(`build:${championId}:${role}:${opponentId}:${queue}`, () =>
    call<Build>("get_build", { championId, role, opponentId, queue }),
  );

export const getCounters = (enemyId: number, role: Role, queue: Queue) =>
  cached(`counters:${enemyId}:${role}:${queue}`, () =>
    call<Counter[]>("get_counters", { enemyId, role, queue }),
  );

export const getMatchups = (championId: number, role: Role, queue: Queue) =>
  cached(`matchups:${championId}:${role}:${queue}`, () =>
    call<MatchupStat[]>("get_matchups", { championId, role, queue }),
  );

export const getTierList = (role: Role, queue: Queue) =>
  cached(`tier:${role}:${queue}`, () => call<TierEntry[]>("get_tier_list", { role, queue }));

export const getSettings = () => call<Settings>("get_settings");

/** Callers should `clearStatsCache()` when stat-affecting settings changed. */
export const saveSettings = (newSettings: Settings) => call<void>("save_settings", { newSettings });

export const getChampSelect = () => call<ChampSelectState>("get_champ_select");
export const getLcuStatus = () => call<LcuStatus>("get_lcu_status");
/** Push a build into the client. Pass `overwritePageId` only after the user
 *  confirmed overwriting the page named in `ImportResult.needs_confirmation`. */
export const importBuild = (build: Build, overwritePageId: number | null = null) =>
  call<ImportResult>("import_build", { build, overwritePageId });

async function on<T>(event: string, cb: (payload: T) => void): Promise<Unlisten> {
  return (await backend()).listen<T>(event, cb);
}

export const onLcuStatus = (cb: (s: LcuStatus) => void) => on("lcu-status", cb);
export const onChampSelect = (cb: (s: ChampSelectState) => void) => on("champ-select", cb);
export const onAutoImported = (cb: (e: AutoImportEvent) => void) => on("auto-imported", cb);
