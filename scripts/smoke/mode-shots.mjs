// Screenshots of the Live view per game mode (mock backend). Run with
//   NODE_PATH=/opt/node-tools/node_modules node shots.mjs <outDir>
import { createRequire } from "node:module";
const require = createRequire("/opt/node-tools/node_modules/");
const { chromium } = require("playwright");

const out = process.argv[2];
const base = "http://127.0.0.1:5199/";
const proxy = process.env.HTTPS_PROXY || process.env.https_proxy;

const shots = [
  // [file, mock mode, wait ms, check text]
  ["mode-ranked.png", "champselect", 6000, "counter picks vs"],
  ["mode-ranked-blind.png", "blind", 6000, "Tier list"],
  ["mode-normal-draft.png", "draft", 6000, "Normal Draft"],
  ["mode-ranked-trade.png", "trade", 6000, "Your imported runes are for"],
  ["mode-swiftplay-lobby.png", "swiftplay", 6000, "Picked in lobby"],
  ["mode-quickplay-lobby.png", "quickplay", 6000, "Quickplay"],
  ["mode-swiftplay-cs.png", "swiftplay-cs", 6000, "Picked in lobby"],
];

// Chromium here doesn't trust the sandbox proxy's CA; curl does. Serve every
// external request (Data Dragon JSON + images) through curl.
import { execFile } from "node:child_process";
const cache = new Map();
const curl = (url) =>
  new Promise((resolve) =>
    execFile("curl", ["-sS", "-L", "--max-time", "30", url], { encoding: "buffer", maxBuffer: 64 << 20 }, (err, out) =>
      resolve(err ? null : out),
    ),
  );
const typeOf = (url) =>
  url.endsWith(".json") ? "application/json" : url.endsWith(".png") ? "image/png" : url.endsWith(".webp") ? "image/webp" : "application/octet-stream";
const browser = await chromium.launch({});
let failed = 0;
for (const [file, mode, wait, text] of shots) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  await page.route(/^https:\/\//, async (route) => {
    const url = route.request().url();
    if (!cache.has(url)) cache.set(url, curl(url));
    const body = await cache.get(url);
    if (!body) return route.abort();
    return route.fulfill({ status: 200, body, contentType: typeOf(url), headers: { "access-control-allow-origin": "*" } });
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto(`${base}?mock=${mode}`);
  await page.waitForTimeout(wait);
  const body = await page.locator("body").innerText();
  const ok = body.toLowerCase().includes(text.toLowerCase());
  if (!ok || errors.length) failed++;
  await page.screenshot({ path: `${out}/${file}`, fullPage: true });
  console.log(`${ok && !errors.length ? "OK  " : "FAIL"} ${file} (${mode}) ${ok ? "" : `missing "${text}"`} ${errors.join(" | ")}`);
  await page.close();
}
await browser.close();
process.exit(failed ? 1 : 0);
