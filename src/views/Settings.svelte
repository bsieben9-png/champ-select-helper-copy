<script lang="ts">
  import { fade } from "svelte/transition";
  import { app } from "../lib/app.svelte";
  import Notice from "../lib/components/Notice.svelte";
  import Toggle from "../lib/components/Toggle.svelte";
  import { RANKS, REGIONS, SOURCES, patchLabel } from "../lib/format";
  import type { Settings, Source } from "../lib/types";

  const s = $derived(app.settings);
  const set = (patch: Partial<Settings>) => app.updateSettings(patch);

  // Briefly show "Saved" after each successful save.
  let showSaved = $state(false);
  const savedOnOpen = app.savedAt;
  $effect(() => {
    if (app.savedAt === savedOnOpen) return;
    showSaved = true;
    const t = setTimeout(() => (showSaved = false), 1600);
    return () => clearTimeout(t);
  });

  function setMinGames(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const v = Math.max(0, Math.min(100000, Math.round(Number(input.value) || 0)));
    input.value = String(v);
    if (v !== s?.min_games) set({ min_games: v });
  }
</script>

<div class="settings">
  <div class="head">
    <h1>Settings</h1>
    {#if showSaved}<span class="saved" transition:fade={{ duration: 150 }}>✓ Saved</span>{/if}
    <span class="spacer"></span>
    <span class="faint small">Changes are saved immediately.</span>
  </div>

  {#if !s}
    <Notice kind="loading" title="Loading settings…" />
  {:else}
    <div class="cards">
      <section class="panel gold card">
        <h2 class="eyebrow">Import into the client</h2>

        <div class="row">
          <div class="text">
            <div class="label">Auto-import</div>
            <div class="desc">
              Imports runes &amp; item set once when you lock in. After that, nothing changes unless you press Import.
            </div>
          </div>
          <Toggle label="" checked={s.auto_import} onchange={(v) => set({ auto_import: v })} title="Auto-import" />
        </div>

        <div class="row">
          <div class="text">
            <div class="label">Rune page</div>
            <div class="desc">
              Writes a “CSH: …” rune page. If all pages are full, you're asked before one of yours is overwritten.
            </div>
          </div>
          <Toggle label="" checked={s.import_runes} onchange={(v) => set({ import_runes: v })} title="Import rune page" />
        </div>

        <div class="row">
          <div class="text">
            <div class="label">Item set</div>
            <div class="desc">Adds the recommended build to the in-game shop as an item set.</div>
          </div>
          <Toggle label="" checked={s.import_item_set} onchange={(v) => set({ import_item_set: v })} title="Import item set" />
        </div>

        <p class="note">
          <svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"
            ><circle cx="12" cy="12" r="9.5" fill="none" stroke="currentColor" stroke-width="2" /><path
              d="M11 10h2v7h-2zM11 6.5h2v2h-2z"
              fill="currentColor"
            /></svg
          >
          Summoner spells are only shown as a recommendation — the app never changes them.
        </p>
      </section>

      <section class="panel gold card">
        <h2 class="eyebrow">Stats</h2>

        <div class="row">
          <div class="text">
            <div class="label">Source</div>
            <div class="desc">Where builds, counters and win rates come from.</div>
          </div>
          <select
            class="select ctl dimmed"
            aria-label="Source"
            value={s.source}
            onchange={(e) => set({ source: e.currentTarget.value as Source })}
            disabled={SOURCES.length < 2}
            title="More sources may be added later"
          >
            {#each SOURCES as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
          </select>
        </div>

        <div class="row">
          <div class="text">
            <div class="label">Rank</div>
            <div class="desc">Games from this rank bracket.</div>
          </div>
          <select class="select ctl" aria-label="Rank" value={s.rank} onchange={(e) => set({ rank: e.currentTarget.value })}>
            <optgroup label="Rank and above">
              {#each RANKS.filter((r) => r.group === "plus") as r (r.value)}<option value={r.value}>{r.label}</option>{/each}
            </optgroup>
            <optgroup label="Single rank">
              {#each RANKS.filter((r) => r.group === "single") as r (r.value)}<option value={r.value}>{r.label}</option>{/each}
            </optgroup>
          </select>
        </div>

        <div class="row">
          <div class="text">
            <div class="label">Region</div>
            <div class="desc">World combines every server.</div>
          </div>
          <select class="select ctl" aria-label="Region" value={s.region} onchange={(e) => set({ region: e.currentTarget.value })}>
            {#each REGIONS as r (r.value)}<option value={r.value}>{r.label}</option>{/each}
          </select>
        </div>

        <div class="row">
          <div class="text">
            <div class="label">Minimum games</div>
            <div class="desc">Matchups with fewer games are ignored for counter picks and tier lists.</div>
          </div>
          <input
            class="input ctl num"
            type="number"
            min="0"
            step="50"
            aria-label="Minimum games"
            value={s.min_games}
            onchange={setMinGames}
          />
        </div>

        <div class="row">
          <div class="text">
            <div class="label">Counters from my pool only</div>
            <div class="desc">Only suggest champions from your champion pool.</div>
          </div>
          <Toggle
            label=""
            checked={s.counters_pool_only}
            onchange={(v) => set({ counters_pool_only: v })}
            title="Counters from my pool only"
          />
        </div>
      </section>
    </div>

    <p class="about faint">
      Stats: u.gg (unofficial){app.staticData?.ugg_patch ? ` · Patch ${patchLabel(app.staticData.ugg_patch)}` : ""} · Names &amp;
      icons: Riot Data Dragon{app.staticData ? ` ${app.staticData.version}` : ""}. Champ Select Helper isn't endorsed by
      Riot Games.
    </p>
  {/if}
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  h1 {
    margin: 0;
    font: 700 16px/1.2 var(--font-head);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--gold-hi);
  }
  .saved {
    font-size: 12px;
    color: var(--teal);
  }
  .spacer {
    flex: 1;
  }
  .small {
    font-size: 11.5px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 12px;
    align-items: start;
  }
  .card {
    padding: 12px 16px 6px;
  }
  h2 {
    margin: 0 0 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 0;
    border-bottom: 1px solid #111b2c;
  }
  .row:last-of-type {
    border-bottom: 0;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .label {
    font-weight: 600;
    color: var(--text);
  }
  .desc {
    margin-top: 1px;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--muted);
  }
  .ctl {
    width: 150px;
    flex: none;
  }
  .dimmed:disabled {
    opacity: 0.6;
    cursor: default;
  }
  input.ctl {
    text-align: right;
    user-select: text;
    -webkit-user-select: text;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 4px 0 10px;
    padding: 7px 10px;
    font-size: 11.5px;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--line);
  }
  .about {
    margin: 4px 0 0;
    font-size: 11px;
  }
</style>
