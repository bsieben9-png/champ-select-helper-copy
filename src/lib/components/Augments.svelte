<script lang="ts">
  import { RARITY_LABEL, games as fmtGames, LOW_SAMPLE, num, pct, rarityOf, wrTone, type Rarity } from "../format";
  import { tip } from "../tooltip";
  import type { AugmentOption } from "../types";
  import AugmentIcon from "./AugmentIcon.svelte";
  import Segmented from "./Segmented.svelte";

  let { augments }: { augments: AugmentOption[] } = $props();

  const TOP = 6;
  const ORDER: Rarity[] = ["prismatic", "gold", "silver", "other"];

  let filter = $state<Rarity | "all">("all");
  let expanded = $state(false);

  const present = $derived(ORDER.filter((r) => augments.some((a) => rarityOf(a.rarity) === r)));
  const filtered = $derived(filter === "all" ? augments : augments.filter((a) => rarityOf(a.rarity) === filter));
  const shown = $derived(expanded ? filtered : filtered.slice(0, TOP));
  const pr = (x: number) => `${(x * 100).toFixed(1)}%`;
</script>

<div class="augments">
  <div class="head">
    <span class="eyebrow">Augments</span>
    <span class="faint sub">picked in-game · win rate · pick rate · games</span>
    <span class="spacer"></span>
    {#if present.length > 1}
      <Segmented
        label="Augment rarity"
        compact
        value={filter}
        onchange={(v) => (filter = v)}
        options={[
          { value: "all" as const, label: "All" },
          ...present.map((r) => ({ value: r, label: RARITY_LABEL[r] })),
        ]}
      >
        {#snippet item(o)}
          {#if o.value !== "all"}<span class="dot {o.value}"></span>{/if}{o.label}
        {/snippet}
      </Segmented>
    {/if}
  </div>

  <div class="list" role="list">
    {#each shown as a (a.id)}
      {@const r = rarityOf(a.rarity)}
      <div
        class="row {r}"
        role="listitem"
        use:tip={{
          title: a.name,
          meta: `${RARITY_LABEL[r]} · ${pct(a.win_rate)} win rate · ${num(a.games)} games`,
          body: a.description,
        }}
      >
        <span class="rank num">{augments.indexOf(a) + 1}</span>
        <AugmentIcon name={a.name} icon={a.icon} rarity={a.rarity} size={30} />
        <span class="name">{a.name}</span>
        <span class="wr num {wrTone(a.win_rate)}" class:low={a.games < LOW_SAMPLE}>{pct(a.win_rate)}</span>
        <span class="pick num" title="Pick rate">{pr(a.pick_rate)}</span>
        <span class="games num" class:lowg={a.games < LOW_SAMPLE}>{fmtGames(a.games)}</span>
      </div>
    {:else}
      <div class="none faint">No augments of this rarity.</div>
    {/each}
  </div>

  {#if filtered.length > TOP}
    <button class="btn small ghost more" onclick={() => (expanded = !expanded)}>
      {expanded ? "Show top picks only" : `Show all ${filtered.length}`}
    </button>
  {/if}
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
    min-height: 26px;
  }
  .sub {
    font-size: 11px;
  }
  .spacer {
    flex: 1;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .dot.prismatic {
    background: linear-gradient(135deg, #f08ad8, #8cc8f5, #f5dc8c);
  }
  .dot.gold {
    background: #e0b153;
  }
  .dot.silver {
    background: #9fb0c2;
  }
  .dot.other {
    background: var(--gold-dim);
  }
  .list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 4px 12px;
  }
  .row {
    display: grid;
    grid-template-columns: 16px 30px minmax(0, 1fr) 50px 46px 44px;
    align-items: center;
    gap: 9px;
    height: 38px;
    padding: 0 10px 0 6px;
    background: rgba(255, 255, 255, 0.015);
    border: 1px solid #0f1828;
    border-left: 2px solid var(--rc, var(--gold-dim));
    font-size: 12px;
    outline: none;
  }
  .row:hover,
  .row:focus-visible {
    background: #0c1a31;
  }
  .row.prismatic {
    --rc: #c58cf2;
  }
  .row.gold {
    --rc: #e0b153;
  }
  .row.silver {
    --rc: #9fb0c2;
  }
  .rank {
    font-size: 10.5px;
    color: var(--faint);
    text-align: right;
  }
  .name {
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .wr {
    text-align: right;
    font-weight: 700;
  }
  .low {
    opacity: 0.6;
  }
  .pick,
  .games {
    text-align: right;
    color: var(--muted);
    font-size: 11px;
  }
  .lowg {
    text-decoration: underline dotted var(--faint);
  }
  .none {
    padding: 8px 4px;
  }
  .more {
    margin-top: 6px;
  }
</style>
