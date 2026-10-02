<script lang="ts">
  import * as api from "../api";
  import { app } from "../app.svelte";
  import { ROLE_LABEL, num } from "../format";
  import { Loader } from "../loader.svelte";
  import { tip } from "../tooltip";
  import type { MatchupStat, Queue, Role } from "../types";
  import ChampIcon from "./ChampIcon.svelte";
  import Notice from "./Notice.svelte";
  import Segmented from "./Segmented.svelte";
  import Spinner from "./Spinner.svelte";
  import Toggle from "./Toggle.svelte";
  import WinRate from "./WinRate.svelte";

  let { championId, role, queue }: { championId: number; role: Role; queue: Queue } = $props();

  const loader = new Loader<MatchupStat[]>();
  $effect(() => {
    const args = [championId, role, queue] as const;
    void app.statsVersion;
    loader.run(() => api.getMatchups(...args));
  });

  type Sort = "wr" | "games";
  let sort = $state<Sort>("wr");
  let includeLow = $state(false);
  let expanded = $state(false);
  const SHOWN = 12;

  const minGames = $derived(app.settings?.min_games ?? 100);
  const lists = $derived.by(() => {
    const all = (loader.data ?? []).filter((m) => includeLow || m.games >= minGames);
    const byGames = (a: MatchupStat, b: MatchupStat) => b.games - a.games;
    const best = all
      .filter((m) => m.win_rate >= 0.5)
      .sort(sort === "wr" ? (a, b) => b.win_rate - a.win_rate : byGames);
    const worst = all
      .filter((m) => m.win_rate < 0.5)
      .sort(sort === "wr" ? (a, b) => a.win_rate - b.win_rate : byGames);
    const hiddenLow = (loader.data ?? []).length - all.length;
    return { best, worst, hiddenLow };
  });

  const name = $derived(app.champName(championId));
  // Diverging bar around 50%: ±8% fills the half.
  const bar = (wr: number) => `${Math.min(100, (Math.abs(wr - 0.5) / 0.08) * 100)}%`;

  function pickOpponent(id: number) {
    app.lookup.opponentId = id;
    app.lookup.tab = "build";
  }
</script>

{#snippet list(items: MatchupStat[], kind: "best" | "worst")}
  <div class="col {kind}">
    <div class="col-head">
      <span class="eyebrow">{kind === "best" ? "Best matchups" : "Worst matchups"} <span class="faint count">{items.length}</span></span>
      <span class="faint count">Win rate · games</span>
    </div>
    {#if items.length === 0}
      <div class="empty faint">None</div>
    {:else}
      {#each expanded ? items : items.slice(0, SHOWN) as m (m.opponent_id)}
        <button
          class="row"
          onclick={() => pickOpponent(m.opponent_id)}
          use:tip={{
            title: `${name} vs ${app.champName(m.opponent_id)}`,
            meta: `${num(m.wins)} wins / ${num(m.games)} games`,
            body: "Click to see the matchup build.",
          }}
        >
          <ChampIcon id={m.opponent_id} size={26} tooltip={false} />
          <span class="name">{app.champName(m.opponent_id)}</span>
          <span class="meter" aria-hidden="true"><span style:width={bar(m.win_rate)}></span></span>
          <WinRate wr={m.win_rate} games={m.games} gamesWord={false} />
        </button>
      {/each}
    {/if}
  </div>
{/snippet}

<section>
  <div class="toolbar">
    <span class="title">
      <span class="eyebrow">{name} · {ROLE_LABEL[role]}</span>
      {#if loader.loading && loader.data}<Spinner size={13} />{/if}
    </span>
    <span class="spacer"></span>
    <Toggle label="Show low-sample" checked={includeLow} onchange={(v) => (includeLow = v)} title="Include matchups with fewer than {minGames} games" />
    <Segmented
      label="Sort matchups"
      compact
      value={sort}
      onchange={(v) => (sort = v)}
      options={[
        { value: "wr", label: "Win rate" },
        { value: "games", label: "Games" },
      ]}
    />
  </div>

  {#if loader.error}
    <Notice kind="error" title="Couldn't load matchups" detail={loader.error}>
      <button class="btn small" onclick={loader.retry}>Retry</button>
    </Notice>
  {:else if !loader.data}
    <div class="cols" aria-hidden="true">
      {#each [0, 1] as c (c)}
        <div class="col">{#each Array(6) as _, i (i)}<div class="row skeleton ph"></div>{/each}</div>
      {/each}
    </div>
  {:else if lists.best.length + lists.worst.length === 0}
    <Notice
      title="No matchup data"
      detail={lists.hiddenLow ? `${lists.hiddenLow} matchups have fewer than ${minGames} games — turn on “Show low-sample”.` : `No matchup data for ${name} ${ROLE_LABEL[role]}.`}
    />
  {:else}
    <div class="cols" class:stale={loader.loading}>
      {@render list(lists.best, "best")}
      {@render list(lists.worst, "worst")}
    </div>
    <div class="foot">
      {#if Math.max(lists.best.length, lists.worst.length) > SHOWN}
        <button class="btn small ghost" onclick={() => (expanded = !expanded)}>{expanded ? "Show fewer" : "Show all"}</button>
      {/if}
      {#if lists.hiddenLow && !includeLow}
        <span class="faint">{lists.hiddenLow} {lists.hiddenLow === 1 ? "matchup" : "matchups"} under {minGames} games hidden</span>
      {/if}
    </div>
  {/if}
</section>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 10px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    transition: opacity 0.15s;
  }
  .cols.stale {
    opacity: 0.55;
  }
  .col {
    border: 1px solid var(--line);
    background: rgba(1, 10, 19, 0.5);
    padding: 4px 0;
  }
  .col-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 6px 12px 8px;
  }
  .worst .eyebrow {
    color: #d47a86;
  }
  .best .eyebrow {
    color: #7fd6ac;
  }
  .count {
    font-size: 11px;
  }
  .row {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr) 70px auto;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 34px;
    padding: 0 12px;
    border: 0;
    background: transparent;
    text-align: left;
  }
  button.row:hover {
    background: #0c1a31;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meter {
    height: 4px;
    background: #0d1626;
    position: relative;
  }
  .meter span {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
  }
  .best .meter span {
    background: linear-gradient(90deg, rgba(62, 207, 142, 0.25), var(--green));
  }
  .worst .meter span {
    background: linear-gradient(90deg, rgba(232, 64, 87, 0.25), var(--red));
  }
  .row :global(.wr) {
    justify-content: flex-end;
    min-width: 92px;
  }
  .empty {
    padding: 10px 12px;
  }
  .ph {
    margin: 2px 8px;
    width: auto;
    height: 30px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 8px;
    font-size: 11.5px;
  }
</style>
