<script lang="ts">
  import type { Snippet } from "svelte";
  import * as api from "../api";
  import { app } from "../app.svelte";
  import { num, wrTone } from "../format";
  import { Loader } from "../loader.svelte";
  import { tip } from "../tooltip";
  import type { Counter, Queue, Role } from "../types";
  import ChampIcon from "./ChampIcon.svelte";
  import Notice from "./Notice.svelte";
  import PoolOnlyToggle from "./PoolOnlyToggle.svelte";
  import Spinner from "./Spinner.svelte";
  import WinRate from "./WinRate.svelte";

  let {
    enemyId,
    role,
    queue,
    title,
    limit = 20,
  }: {
    enemyId: number;
    role: Role;
    queue: Queue;
    title: Snippet;
    limit?: number;
  } = $props();

  const loader = new Loader<Counter[]>();
  $effect(() => {
    const args = [enemyId, role, queue] as const;
    void app.statsVersion;
    loader.run(() => api.getCounters(...args));
  });

  const poolOnly = $derived(app.settings?.counters_pool_only ?? false);
  // Pool membership is checked locally so starring a champ updates instantly.
  const groups = $derived.by(() => {
    const all = (loader.data ?? []).map((c) => ({ ...c, in_pool: app.pool.has(c.champion_id) }));
    // Pool champs go first — but only ones that actually win the matchup,
    // unless the user asked to see nothing but their pool.
    const first = (c: Counter) => c.in_pool && (poolOnly || c.win_rate >= 0.5);
    const mine = all.filter(first);
    const rest = poolOnly ? [] : all.filter((c) => !first(c)).slice(0, limit);
    return { mine, rest, total: all.length };
  });

  const enemyName = $derived(app.champName(enemyId));
  // WR bar: 44% → empty, 58% → full.
  const barWidth = (wr: number) => `${Math.max(4, Math.min(100, ((wr - 0.44) / 0.14) * 100))}%`;
</script>

{#snippet card(c: Counter)}
  <button
    class="card tone-{wrTone(c.win_rate)}"
    class:mine={c.in_pool}
    onclick={() => app.openLookup(c.champion_id, enemyId, role, queue)}
    use:tip={{
      title: `${app.champName(c.champion_id)} vs ${enemyName}`,
      meta: `${num(c.games)} games`,
      body: "Click to open this matchup build in Lookup.",
    }}
  >
    <ChampIcon id={c.champion_id} size={44} tooltip={false} />
    <span class="info">
      <span class="name">{app.champName(c.champion_id)}</span>
      <WinRate wr={c.win_rate} games={c.games} size="lg" stack left />
    </span>
    {#if c.in_pool}<span class="star" aria-label="In your pool">★</span>{/if}
    <span class="bar" style:width={barWidth(c.win_rate)}></span>
  </button>
{/snippet}

<section class="counters">
  <div class="toolbar">
    <div class="title">{@render title()}</div>
    {#if loader.loading && loader.data}<Spinner size={13} />{/if}
    <span class="spacer"></span>
    <PoolOnlyToggle />
  </div>

  {#if loader.error}
    <Notice kind="error" title="Couldn't load counters" detail={loader.error}>
      <button class="btn small" onclick={loader.retry}>Retry</button>
    </Notice>
  {:else if !loader.data}
    <div class="grid" aria-hidden="true">
      {#each Array(10) as _, i (i)}<div class="card skeleton ph"></div>{/each}
    </div>
  {:else if groups.mine.length === 0 && groups.rest.length === 0}
    {#if poolOnly}
      <Notice
        title={app.pool.size ? `None of your pool champions counter ${enemyName}` : "Your champion pool is empty"}
        detail={app.pool.size
          ? `Nobody from your pool has a good record vs ${enemyName} with at least ${app.settings?.min_games ?? 0} games.`
          : "Add the champions you play in Pool to see them first here."}
      >
        <button class="btn small" onclick={() => app.updateSettings({ counters_pool_only: false })}>Show all counters</button>
        <button class="btn small ghost" onclick={() => (app.view = "pool")}>Edit pool</button>
      </Notice>
    {:else}
      <Notice
        title="No counter data"
        detail={`No matchups vs ${enemyName} with at least ${app.settings?.min_games ?? 0} games. Lower “Min games” in Settings to see more.`}
      />
    {/if}
  {:else}
    {#if groups.mine.length}
      <div class="group-label"><span class="star-label">★</span> Your pool</div>
      <div class="grid">
        {#each groups.mine as c (c.champion_id)}{@render card(c)}{/each}
      </div>
    {/if}
    {#if groups.rest.length}
      {#if groups.mine.length}<div class="group-label">Best counters</div>{/if}
      <div class="grid" class:stale={loader.loading}>
        {#each groups.rest as c (c.champion_id)}{@render card(c)}{/each}
      </div>
    {/if}
  {/if}
</section>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 10px;
    min-height: 26px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .spacer {
    flex: 1;
  }
  .group-label {
    margin: 10px 0 6px;
    font: 600 11px/1 var(--font-head);
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .group-label:first-of-type {
    margin-top: 0;
  }
  .star-label {
    color: var(--gold);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 6px;
    transition: opacity 0.15s;
  }
  .grid.stale {
    opacity: 0.55;
  }
  .card {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 66px;
    padding: 0 10px 0 10px;
    text-align: left;
    background: linear-gradient(180deg, #0c1830, #091428);
    border: 1px solid var(--line);
    overflow: hidden;
    transition: border-color 0.12s, background 0.12s;
  }
  .card:hover {
    border-color: var(--gold-dim);
    background: linear-gradient(180deg, #102040, #0b1830);
  }
  .card.mine {
    border-color: var(--line-gold);
  }
  .card :global(.champ) {
    border: 1px solid #1e2328;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .info :global(.wr.lg b) {
    font-size: 16px;
  }
  .name {
    font-weight: 600;
    font-size: 13px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .star {
    position: absolute;
    top: 4px;
    right: 6px;
    color: var(--gold);
    font-size: 12px;
    line-height: 1;
  }
  .bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    opacity: 0.7;
  }
  .tone-good .bar {
    background: var(--green);
  }
  .tone-bad .bar {
    background: var(--red);
  }
  .tone-even .bar {
    background: var(--gold-dim);
  }
  .ph {
    border-color: transparent;
  }
</style>
