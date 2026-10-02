<script lang="ts">
  import type { Snippet } from "svelte";
  import * as api from "../api";
  import { app } from "../app.svelte";
  import { LOW_SAMPLE, games as fmtGames, num, pct, wrTone } from "../format";
  import { Loader } from "../loader.svelte";
  import { tip } from "../tooltip";
  import type { Queue, Role, TierEntry } from "../types";
  import ChampIcon from "./ChampIcon.svelte";
  import Notice from "./Notice.svelte";
  import PoolOnlyToggle from "./PoolOnlyToggle.svelte";
  import Spinner from "./Spinner.svelte";

  let {
    role,
    queue,
    title,
    limit = 30,
  }: {
    role: Role;
    queue: Queue;
    title: Snippet;
    limit?: number;
  } = $props();

  const loader = new Loader<TierEntry[]>();
  $effect(() => {
    const args = [role, queue] as const;
    void app.statsVersion;
    loader.run(() => api.getTierList(...args));
  });

  let showAll = $state(false);
  const poolOnly = $derived(app.settings?.counters_pool_only ?? false);
  const rows = $derived.by(() => {
    const all = (loader.data ?? []).map((t, i) => ({ ...t, rank: i + 1, in_pool: app.pool.has(t.champion_id) }));
    const mine = all.filter((t) => t.in_pool);
    const rest = poolOnly ? [] : all.filter((t) => !t.in_pool);
    return { mine, rest: showAll ? rest : rest.slice(0, limit), hidden: Math.max(0, rest.length - limit) };
  });
  const pr = (x: number) => `${(x * 100).toFixed(1)}%`;
  // WR bar: 45% → empty, 56% → full.
  const bar = (wr: number) => `${Math.max(3, Math.min(100, ((wr - 0.45) / 0.11) * 100))}%`;
</script>

{#snippet row(t: TierEntry & { rank: number })}
  <button
    class="row"
    class:mine={t.in_pool}
    onclick={() => app.openLookup(t.champion_id, null, role, queue)}
    use:tip={{ title: app.champName(t.champion_id), body: "Click to open this build in Lookup." }}
  >
    <span class="rank num">{t.rank}</span>
    <span class="champ">
      <ChampIcon id={t.champion_id} size={26} tooltip={false} />
      <span class="name">{app.champName(t.champion_id)}</span>
      {#if t.in_pool}<span class="star" aria-label="In your pool">★</span>{/if}
    </span>
    <span class="wrcell">
      <span class="c num {wrTone(t.win_rate)}" class:low={t.games < LOW_SAMPLE}>{pct(t.win_rate)}</span>
      <span class="track" aria-hidden="true"><span class="fill tone-{wrTone(t.win_rate)}" style:width={bar(t.win_rate)}></span></span>
    </span>
    <span class="c num">{pr(t.pick_rate)}</span>
    <span class="c num muted">{pr(t.ban_rate)}</span>
    <span class="c num muted" class:lowg={t.games < LOW_SAMPLE} title="{num(t.games)} games">{fmtGames(t.games)}</span>
  </button>
{/snippet}

<section class="tiers">
  <div class="toolbar">
    <div class="title">{@render title()}</div>
    {#if loader.loading && loader.data}<Spinner size={13} />{/if}
    <span class="spacer"></span>
    <PoolOnlyToggle />
  </div>

  {#if loader.error}
    <Notice kind="error" title="Couldn't load the tier list" detail={loader.error}>
      <button class="btn small" onclick={loader.retry}>Retry</button>
    </Notice>
  {:else if !loader.data}
    <div class="table" aria-hidden="true">
      {#each Array(8) as _, i (i)}<div class="row skeleton ph"></div>{/each}
    </div>
  {:else if rows.mine.length === 0 && rows.rest.length === 0}
    <Notice
      title={poolOnly ? "None of your pool champions play this role" : "No tier list data"}
      detail={poolOnly ? "Turn off “Only my pool” or add champions in Pool." : "Try another role or lower “Min games”."}
    >
      {#if poolOnly}
        <button class="btn small" onclick={() => app.updateSettings({ counters_pool_only: false })}>Show all</button>
      {/if}
    </Notice>
  {:else}
    <div class="table" class:stale={loader.loading} role="list">
      <div class="row head" aria-hidden="true">
        <span class="rank">#</span><span>Champion</span><span class="wrcell"><span class="c">Win rate</span></span><span
          class="c">Pick</span
        ><span class="c">Ban</span><span class="c">Games</span>
      </div>
      {#if rows.mine.length}
        {#each rows.mine as t (t.champion_id)}{@render row(t)}{/each}
        {#if rows.rest.length}<div class="divider" aria-hidden="true"></div>{/if}
      {/if}
      {#each rows.rest as t (t.champion_id)}{@render row(t)}{/each}
    </div>
    {#if rows.hidden && !showAll}
      <button class="btn small ghost more" onclick={() => (showAll = true)}>Show {rows.hidden} more</button>
    {/if}
  {/if}
</section>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
    min-height: 26px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .table {
    border: 1px solid var(--line);
    background: rgba(1, 10, 19, 0.5);
    transition: opacity 0.15s;
  }
  .table.stale {
    opacity: 0.55;
  }
  .row {
    display: grid;
    grid-template-columns: 34px minmax(140px, 210px) minmax(150px, 1fr) 64px 64px 70px;
    align-items: center;
    width: 100%;
    height: 34px;
    padding: 0 12px 0 6px;
    border: 0;
    border-bottom: 1px solid #0f1724;
    background: transparent;
    text-align: left;
    font-size: 12.5px;
  }
  button.row:hover {
    background: #0c1a31;
  }
  .row.head {
    height: 26px;
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--faint);
    background: #050f1d;
  }
  .row.mine {
    background: rgba(200, 155, 60, 0.05);
  }
  .rank {
    color: var(--faint);
    text-align: center;
    font-size: 11px;
  }
  .champ {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
  }
  .name {
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .star {
    color: var(--gold);
    font-size: 11px;
  }
  .c {
    text-align: right;
  }
  .wrcell {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-right: 12px;
  }
  .wrcell .c {
    width: 52px;
    flex: none;
  }
  .head .wrcell .c {
    width: auto;
  }
  .track {
    flex: 1;
    height: 4px;
    background: #0d1626;
  }
  .fill {
    display: block;
    height: 100%;
  }
  .fill.tone-good {
    background: linear-gradient(90deg, rgba(62, 207, 142, 0.3), var(--green));
  }
  .fill.tone-even {
    background: linear-gradient(90deg, rgba(200, 170, 110, 0.2), var(--gold-dim));
  }
  .fill.tone-bad {
    background: linear-gradient(90deg, rgba(232, 64, 87, 0.3), var(--red));
  }
  .c.good,
  .c.bad,
  .c.even {
    font-weight: 700;
  }
  .low {
    opacity: 0.6;
  }
  .lowg {
    text-decoration: underline dotted var(--faint);
  }
  .divider {
    height: 0;
    border-top: 1px solid var(--line-gold);
  }
  .ph {
    border-bottom: 2px solid #010a13;
  }
  .more {
    margin-top: 6px;
  }
</style>
