<script lang="ts">
  import { app } from "../app.svelte";
  import { num } from "../format";
  import type { Build, ItemOption } from "../types";
  import Img from "./Img.svelte";
  import WinRate from "./WinRate.svelte";

  let { build }: { build: Build } = $props();

  const MAX_OPTIONS = 3;

  /** Starting items with duplicates collapsed: [{id, count}] */
  const starting = $derived.by(() => {
    const out: { id: number; count: number }[] = [];
    for (const id of build.starting_items.items) {
      const e = out.find((o) => o.id === id);
      if (e) e.count++;
      else out.push({ id, count: 1 });
    }
    return out;
  });

  // Hide options already in the core build ("what do I buy next?"), but
  // never leave a slot empty because of it.
  const core = $derived(new Set(build.core_items.items));
  const options = (list: ItemOption[]) => {
    const fresh = list.filter((o) => !core.has(o.item_id));
    return (fresh.length ? fresh : list).slice(0, MAX_OPTIONS);
  };
  const later = $derived([
    { label: "4th", list: options(build.fourth_items) },
    { label: "5th", list: options(build.fifth_items) },
    { label: "6th", list: options(build.sixth_items) },
  ]);

  const itemTip = (id: number) => {
    const it = app.items.get(id);
    return it ? { title: it.name, meta: `${num(it.gold)} gold` } : { title: `Item ${id}` };
  };
</script>

{#snippet item(id: number, size: number, count = 1)}
  <span class="item">
    <Img src={app.items.get(id)?.icon} {size} alt={app.items.get(id)?.name ?? ""} tooltip={itemTip(id)} />
    {#if count > 1}<span class="count num">{count}</span>{/if}
  </span>
{/snippet}

<div class="items">
  <div class="group">
    <div class="eyebrow">Starting</div>
    <div class="icons">
      {#each starting as s (s.id)}{@render item(s.id, 32, s.count)}{/each}
    </div>
    <WinRate wr={build.starting_items.win_rate} games={build.starting_items.games} size="sm" />
  </div>

  <div class="group core">
    <div class="eyebrow">Core build</div>
    <div class="icons">
      {#each build.core_items.items as id, i (i)}
        {#if i > 0}<span class="arrow" aria-hidden="true">›</span>{/if}
        {@render item(id, 34)}
      {/each}
    </div>
    <WinRate wr={build.core_items.win_rate} games={build.core_items.games} size="sm" />
  </div>

  {#each later as slot (slot.label)}
    <div class="group opt">
      <div class="eyebrow">{slot.label}</div>
      {#if slot.list.length}
        <div class="options">
          {#each slot.list as o (o.item_id)}
            <div class="option">
              {@render item(o.item_id, 30)}
              <WinRate wr={o.win_rate} games={o.games} stack gamesWord={false} size="sm" />
            </div>
          {/each}
        </div>
      {:else}
        <div class="none faint">—</div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .items {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 0;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 16px;
    border-left: 1px solid var(--line);
  }
  .group:first-child {
    padding-left: 0;
    border-left: 0;
  }
  .group:last-child {
    padding-right: 0;
  }
  .icons {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 34px;
  }
  .item {
    position: relative;
    display: inline-flex;
    border: 1px solid var(--line-gold);
    background: #010a13;
  }
  .core .item {
    border-color: var(--gold-dim);
  }
  .count {
    position: absolute;
    right: 1px;
    bottom: 0;
    font-size: 10.5px;
    font-weight: 700;
    color: var(--text);
    text-shadow:
      0 0 2px #000,
      0 0 2px #000;
  }
  .arrow {
    color: var(--gold-dim);
    font-size: 18px;
    line-height: 1;
    margin: 0 1px;
  }
  .options {
    display: flex;
    gap: 6px;
  }
  .option {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
  }
  .none {
    height: 32px;
    line-height: 32px;
  }
</style>
