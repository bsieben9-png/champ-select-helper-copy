// Real-app smoke test: drives the actual Tauri app (real Rust backend, live
// u.gg / Data Dragon data, League NOT running) through tauri-driver's
// WebDriver endpoint. Plain `fetch`, no npm dependencies.
//
// Usually started by scripts/smoke/run.sh (Linux: xvfb + tauri-driver).
// Env: APP=<path to the built app binary> (required), WEBDRIVER=http://127.0.0.1:4444,
//      SHOTS=<dir for screenshots> (default docs/screenshots), CONFIG_DIR=<app config dir>.
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const WD = process.env.WEBDRIVER ?? "http://127.0.0.1:4444";
const APP = process.env.APP;
const SHOTS = process.env.SHOTS ?? "docs/screenshots";
const CONFIG_DIR = process.env.CONFIG_DIR;
if (!APP) throw new Error("set APP to the app binary");
mkdirSync(SHOTS, { recursive: true });

const ELEMENT = "element-6066-11e4-a52f-4a8a0b7e0b01";
const results = [];
let session = null;

async function wd(method, path, body) {
  const res = await fetch(`${WD}${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(`${method} ${path}: ${res.status} ${JSON.stringify(json.value ?? json)}`);
  return json.value;
}

const s = (path) => `/session/${session}${path}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function start() {
  const v = await wd("POST", "/session", {
    capabilities: { alwaysMatch: { browserName: "wry", "tauri:options": { application: APP } } },
  });
  session = v.sessionId;
}

async function stop() {
  if (session) await wd("DELETE", `/session/${session}`).catch(() => {});
  session = null;
}

const js = (script, ...args) => wd("POST", s("/execute/sync"), { script, args });

async function waitFor(what, script, timeout = 45000) {
  const t0 = Date.now();
  let last;
  while (Date.now() - t0 < timeout) {
    try {
      last = await js(script);
      if (last) return last;
    } catch (e) {
      last = String(e);
    }
    await sleep(300);
  }
  throw new Error(`timed out waiting for ${what} (last: ${JSON.stringify(last)})`);
}

async function find(css) {
  const v = await wd("POST", s("/element"), { using: "css selector", value: css });
  const id = v?.[ELEMENT] ?? Object.values(v ?? {})[0];
  if (!id) throw new Error(`no element ${css}: ${JSON.stringify(v)}`);
  return id;
}
const click = async (css) => wd("POST", s(`/element/${await find(css)}/click`), {});
const type = async (css, text) => wd("POST", s(`/element/${await find(css)}/value`), { text });

/** Click the first button whose text starts with `text` (optionally inside `scope`). */
async function clickText(text, scope = "body") {
  const ok = await js(
    `const b = [...document.querySelectorAll(arguments[1] + ' button')].find(b => b.textContent.trim().startsWith(arguments[0]));
     if (!b) return false; b.click(); return true;`,
    text,
    scope,
  );
  if (!ok) throw new Error(`no button "${text}"`);
}

/** Wait (max 15 s) for the images on screen to finish loading; return the
 *  ones that failed (broken icon URLs) and those still loading. */
async function images() {
  const script = `
    const vis = [...document.images].filter(i => { const r = i.getBoundingClientRect();
      return r.width > 0 && r.bottom > 0 && r.top < innerHeight; });
    return { pending: vis.filter(i => !i.complete).map(i => i.src),
             broken: vis.filter(i => i.complete && i.naturalWidth === 0).map(i => i.src) };`;
  const t0 = Date.now();
  let r = await js(script);
  while (r.pending.length && Date.now() - t0 < 15000) {
    await sleep(300);
    r = await js(script);
  }
  brokenImages.push(...r.broken, ...r.pending.map((src) => `(still loading) ${src}`));
  return r;
}
const brokenImages = [];

async function shot(name) {
  await images().catch(() => {});
  // WebKitGTK under Xvfb sometimes doesn't repaint <img loading=lazy> whose
  // src changed (they are loaded — see images()); nudge a repaint.
  await js(`const m = document.querySelector('main'); m.scrollBy(0, 1); m.scrollBy(0, -1);
            document.querySelectorAll('img[loading=lazy]').forEach(i => { i.loading = 'eager'; const s = i.src; i.src = ''; i.src = s; });
            return true;`).catch(() => {});
  await sleep(400);
  const png = await wd("GET", s("/screenshot"));
  const file = join(SHOTS, `real-${name}.png`);
  writeFileSync(file, Buffer.from(png, "base64"));
  return file;
}

/** Whole virtual screen via X (window frame included), when run.sh set XSHOT_DISPLAY. */
function xshot(name) {
  if (!process.env.XSHOT_DISPLAY) return null;
  const file = join(SHOTS, `real-${name}.png`);
  try {
    execFileSync("import", ["-window", "root", file], { env: { ...process.env, DISPLAY: process.env.XSHOT_DISPLAY } });
    return file;
  } catch (e) {
    return `X screenshot failed: ${e.message}`;
  }
}

/** Visible error notices / toasts in the page. */
const pageErrors = () =>
  js(`return [...document.querySelectorAll('.notice.error, .toast.error, [role=alert]')]
        .map(e => e.innerText.trim()).filter(Boolean);`);

async function step(name, fn) {
  const t0 = Date.now();
  try {
    const info = await fn();
    const errors = await pageErrors().catch(() => []);
    results.push({ name, ok: errors.length === 0, ms: Date.now() - t0, info, errors });
  } catch (e) {
    let file;
    try {
      file = await shot(`FAILED-${name.replace(/\W+/g, "-")}`);
    } catch {}
    results.push({ name, ok: false, ms: Date.now() - t0, error: String(e), screenshot: file });
  }
}

const buildLoaded = `
  const b = document.querySelector('section.build');
  return !!b && b.getAttribute('aria-busy') === 'false' && document.querySelectorAll('.runes-card img').length >= 9
    && document.querySelector('.build .source')?.textContent.includes('u.gg')
    && document.querySelector('section.build .title').innerText;`;

// ---------------------------------------------------------------------------

await start();

await step("app starts, static data loads (Live, League not running)", async () => {
  await waitFor("nav", `return document.querySelectorAll('nav button').length === 4`);
  await waitFor(
    "Live screen without loading spinner",
    `return !document.querySelector('main .spinner, main .skeleton') && document.querySelector('main').innerText.length > 20`,
  );
  const pill = await js(`return document.querySelector('header').innerText`);
  return { header: pill.replace(/\s+/g, " "), shot: await shot("live") };
});

await step("Lookup: Yorick build (general)", async () => {
  await clickText("Lookup", "nav");
  await type('input[aria-label="Champion"]', "Yorick");
  const title = await waitFor("Yorick build", buildLoaded);
  const meta = await js(`return document.querySelector('section.build .meta').innerText`);
  const counts = await js(`return {
      runes: document.querySelectorAll('.runes-card img').length,
      items: document.querySelectorAll('section.build img[src*="/img/item/"]').length,
      spells: document.querySelectorAll('section.build img[src*="/img/spell/"]').length,
      skills: document.querySelector('section.build').innerText.match(/Skill/g)?.length ?? 0 }`);
  if (counts.items < 3 || counts.spells !== 2) throw new Error(`build looks incomplete: ${JSON.stringify(counts)}`);
  return {
    title: title.replace(/\s+/g, " "),
    meta: meta.replace(/\s+/g, " "),
    counts,
    shot: await shot("lookup-build"),
    xshot: xshot("xvfb-window"),
  };
});

await step("Lookup: Yorick vs Gwen (matchup build)", async () => {
  await type('input[aria-label="Opponent"]', "Gwen");
  const title = await waitFor(
    "matchup build",
    `const t = document.querySelector('section.build .title')?.innerText ?? '';
     return t.includes('Gwen') && document.querySelector('section.build').getAttribute('aria-busy') === 'false' && t;`,
  );
  await waitFor("matchup build body", buildLoaded);
  const meta = await js(`return document.querySelector('section.build .meta').innerText`);
  const fallback = await js(`return document.querySelector('section.build .fallback')?.innerText ?? null`);
  return { title: title.replace(/\s+/g, " "), meta: meta.replace(/\s+/g, " "), fallback, shot: await shot("lookup-matchup") };
});

// The UI hides icons that fail to load (empty box), so check every icon the
// build needs directly: backend build + static data via the real IPC, then
// load each icon URL in the webview.
await step("Icons: every item / spell / rune of Yorick vs Gwen resolves", async () => {
  const r = await wd("POST", s("/execute/async"), {
    script: `const done = arguments[arguments.length - 1];
      (async () => {
        const invoke = window.__TAURI_INTERNALS__.invoke;
        const sd = await invoke('get_static_data');
        const b = await invoke('get_build', { championId: 83, role: 'top', opponentId: 887, queue: 'ranked_solo' });
        const items = new Map(sd.items.map(i => [i.id, i]));
        const spells = new Map(sd.spells.map(x => [x.id, x]));
        const runes = new Map(sd.rune_styles.flatMap(st => st.slots.flat()).concat(sd.shards).map(x => [x.id, x]));
        const want = [];
        const missing = [];
        const add = (kind, id, map) => { const x = map.get(id); x ? want.push([kind + ' ' + id + ' ' + x.name, x.icon]) : missing.push(kind + ' ' + id); };
        for (const id of [...b.starting_items.items, ...b.core_items.items,
                          ...[...b.fourth_items, ...b.fifth_items, ...b.sixth_items].map(o => o.item_id)]) add('item', id, items);
        for (const id of b.spells.ids) add('spell', id, spells);
        for (const id of [...b.runes.perks, ...b.runes.shards]) add('rune', id, runes);
        const load = ([what, src]) => new Promise(res => { const i = new Image(); i.onload = () => res(null); i.onerror = () => res(what + ' -> ' + src); i.src = src; });
        const broken = (await Promise.all(want.map(load))).filter(Boolean);
        return { checked: want.length, missing, broken };
      })().then(done, e => done({ error: String(e) }));`,
    args: [],
  });
  if (r.error) throw new Error(r.error);
  if (r.missing.length || r.broken.length)
    throw new Error(`not in static data: ${JSON.stringify(r.missing)}; icon failed to load: ${JSON.stringify(r.broken)}`);
  return r;
});

await step("Lookup: who beats Yorick (counters)", async () => {
  await clickText("Who beats", '[role="tablist"][aria-label="Lookup"]');
  const n = await waitFor(
    "counter cards",
    `const c = document.querySelectorAll('section.counters .grid .card:not(.skeleton)'); return c.length > 0 && c.length;`,
  );
  const top = await js(`return [...document.querySelectorAll('section.counters .grid .card:not(.skeleton)')].slice(0,3).map(c => c.innerText.replace(/\\s+/g,' '))`);
  return { counters: n, top, shot: await shot("lookup-counters") };
});

await step("Lookup: Yorick matchups", async () => {
  await clickText("Matchups", '[role="tablist"][aria-label="Lookup"]');
  const n = await waitFor("matchup rows", `const r = document.querySelectorAll('.col .row'); return r.length > 0 && r.length;`);
  return { rows: n, shot: await shot("lookup-matchups") };
});

await step("Lookup: tier list (top)", async () => {
  await clickText("Tier list", '[role="tablist"][aria-label="Lookup"]');
  const n = await waitFor("tier rows", `const r = document.querySelectorAll('section.tiers .row'); return r.length > 5 && r.length;`);
  const top = await js(`return [...document.querySelectorAll('section.tiers .row')].slice(0,3).map(r => r.innerText.replace(/\\s+/g,' '))`);
  return { rows: n, top, shot: await shot("lookup-tierlist") };
});

await step("Lookup: ARAM Mayhem build + augments", async () => {
  await js(`const q = document.querySelector('select[aria-label="Queue"]'); q.value = 'aram_mayhem'; q.dispatchEvent(new Event('change', {bubbles: true})); return true;`);
  await clickText("Build", '[role="tablist"][aria-label="Lookup"]');
  await waitFor(
    "Mayhem build",
    `return document.querySelector('section.build .meta')?.innerText.includes('Mayhem') && document.querySelector('section.build').getAttribute('aria-busy') === 'false' && document.querySelectorAll('.runes-card img').length >= 9`,
  );
  const augments = await js(`return document.querySelectorAll('.augments-card img').length`);
  if (!augments) throw new Error("no augments shown");
  const r = { augmentIcons: augments, shot: await shot("lookup-mayhem") };
  await js(`const q = document.querySelector('select[aria-label="Queue"]'); q.value = 'ranked_solo'; q.dispatchEvent(new Event('change', {bubbles: true})); return true;`);
  return r;
});

await step("Pool: add Yorick and Gwen", async () => {
  await clickText("Pool", "nav");
  await waitFor("pool grid", `return document.querySelectorAll('.grid .tile').length > 100`);
  await click('button[title="Add Yorick to pool"]');
  await click('button[title="Add Gwen to pool"]');
  await waitFor("pool saved", `return document.querySelectorAll('.grid .tile.on').length === 2`);
  return { shot: await shot("pool") };
});

await step("Settings: change rank + minimum games (saved)", async () => {
  await clickText("Settings", "nav");
  await waitFor("settings", `return !!document.querySelector('select[aria-label="Rank"]')`);
  await js(`const r = document.querySelector('select[aria-label="Rank"]'); r.value = 'master_plus'; r.dispatchEvent(new Event('change', {bubbles: true})); return true;`);
  await sleep(300);
  await js(`const m = document.querySelector('input[aria-label="Minimum games"]'); m.value = '200'; m.dispatchEvent(new Event('change', {bubbles: true})); return true;`);
  await waitFor("Saved hint", `return document.body.innerText.includes('Saved')`, 10000).catch(() => {});
  await sleep(500);
  return { shot: await shot("settings") };
});

await stop();

if (CONFIG_DIR) {
  await step("Settings file written", async () => {
    const file = join(CONFIG_DIR, "settings.json");
    if (!existsSync(file)) throw new Error(`${file} missing`);
    const saved = JSON.parse(readFileSync(file, "utf8"));
    if (saved.rank !== "master_plus" || saved.min_games !== 200) throw new Error(`unexpected ${JSON.stringify(saved)}`);
    if (!(saved.champion_pool.includes(83) && saved.champion_pool.includes(887)))
      throw new Error(`pool not saved: ${JSON.stringify(saved.champion_pool)}`);
    return saved;
  });
}

// Relaunch: settings must come back.
await start();
await step("Relaunch: settings + pool reloaded", async () => {
  await waitFor("nav", `return document.querySelectorAll('nav button').length === 4`);
  await clickText("Settings", "nav");
  const v = await waitFor(
    "settings values",
    `const r = document.querySelector('select[aria-label="Rank"]'); const m = document.querySelector('input[aria-label="Minimum games"]');
     return r && m && {rank: r.value, minGames: m.value};`,
  );
  if (v.rank !== "master_plus" || v.minGames !== "200") throw new Error(`not reloaded: ${JSON.stringify(v)}`);
  await clickText("Pool", "nav");
  const pool = await waitFor("pool", `const on = document.querySelectorAll('.grid .tile.on'); return on.length > 0 && [...on].map(t => t.innerText.trim())`);
  if (pool.length !== 2) throw new Error(`pool after relaunch: ${pool}`);
  await clickText("Settings", "nav");
  return { ...v, pool, shot: await shot("settings-reloaded") };
});
await stop();

const broken = [...new Set(brokenImages)];
results.push({
  name: "every icon on screen loaded",
  ok: broken.length === 0,
  ms: 0,
  info: broken.length ? undefined : "all images loaded",
  error: broken.length ? `broken/unloaded images: ${JSON.stringify(broken)}` : undefined,
});
const failed = results.filter((r) => !r.ok);
for (const r of results) {
  console.log(`${r.ok ? "PASS" : "FAIL"} ${r.name} (${r.ms} ms)`);
  if (r.info) console.log("     " + JSON.stringify(r.info));
  if (r.errors?.length) console.log("     page errors: " + JSON.stringify(r.errors));
  if (r.error) console.log("     " + r.error + (r.screenshot ? ` [${r.screenshot}]` : ""));
}
console.log(`\n${results.length - failed.length}/${results.length} steps passed`);
process.exit(failed.length ? 1 : 0);
