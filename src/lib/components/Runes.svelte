<script lang="ts">
  import { app } from "../app.svelte";
  import { SHARD_ROWS } from "../format";
  import { tip } from "../tooltip";
  import type { RuneInfo, RunePage } from "../types";

  let { runes }: { runes: RunePage } = $props();

  // Tree accent colours as in the client.
  const TREE_COLOR: Record<number, string> = {
    8000: "#c8aa6e",
    8100: "#d44242",
    8200: "#9faafc",
    8300: "#49aab9",
    8400: "#a1d586",
  };

  const primary = $derived(app.styles.get(runes.primary_style));
  const secondary = $derived(app.styles.get(runes.sub_style));
  const selected = $derived(new Set(runes.perks));

  // Each shard row: the client's 3 options, or just the pick if it's unknown.
  const shardRows = $derived(
    SHARD_ROWS.map((row, i) => {
      const pick = runes.shards[i];
      const ids = pick === undefined || row.includes(pick) ? row : [pick];
      return ids.map((id) => ({ id, on: id === pick, info: app.shards.get(id) }));
    }),
  );
</script>

{#snippet rune(r: RuneInfo | undefined, on: boolean, size: number, kind: string)}
  {#if r}
    <span
      class="rune {kind}"
      class:on
      style:width="{size}px"
      style:height="{size}px"
      use:tip={{ title: r.name, body: r.short_desc }}
      tabindex="-1"
    >
      <img src={r.icon} alt={r.name} width={size} height={size} loading="lazy" />
    </span>
  {/if}
{/snippet}

{#snippet tree(style: typeof primary, isPrimary: boolean)}
  {#if style}
    <div class="tree" class:primary={isPrimary} style:--tree={TREE_COLOR[style.id] ?? "var(--gold-hi)"}>
      <div class="tree-head">
        <img src={style.icon} alt="" width="18" height="18" />
        <span>{style.name}</span>
      </div>
      {#each style.slots as row, i}
        {#if isPrimary || i > 0}
          <div class="row" class:keys={i === 0}>
            {#each row as r (r.id)}
              {@render rune(r, selected.has(r.id), i === 0 ? 38 : isPrimary ? 27 : 25, i === 0 ? "keystone" : "minor")}
            {/each}
          </div>
        {/if}
      {/each}
      {#if !isPrimary}
        <div class="shards">
          {#each shardRows as row, i (i)}
            <div class="row shard-row">
              {#each row as s (s.id)}
                {@render rune(s.info, s.on, 18, "shard")}
              {/each}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
{/snippet}

<div class="runes">
  {#if primary && secondary}
    {@render tree(primary, true)}
    <div class="divider" aria-hidden="true"></div>
    {@render tree(secondary, false)}
  {:else}
    <p class="muted">Rune data unavailable for this patch.</p>
  {/if}
</div>

<style>
  .runes {
    display: flex;
    gap: 14px;
    align-items: stretch;
  }
  .divider {
    width: 1px;
    background: linear-gradient(180deg, transparent, var(--line-gold) 15%, var(--line-gold) 85%, transparent);
  }
  .tree {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    min-width: 108px;
  }
  .tree.primary {
    min-width: 150px;
  }
  .tree-head {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: stretch;
    justify-content: center;
    margin-bottom: 2px;
    font: 600 11px/1 var(--font-head);
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--tree);
  }
  .row {
    display: flex;
    gap: 9px;
    justify-content: center;
  }
  .row.keys {
    gap: 4px;
    margin-bottom: 3px;
  }
  .rune {
    position: relative;
    display: inline-block;
    border-radius: 50%;
    outline: none;
  }
  .rune img {
    display: block;
    width: 100%;
    height: 100%;
    filter: grayscale(1) brightness(0.75);
    opacity: 0.3;
    transition: opacity 0.12s, filter 0.12s;
  }
  .rune:hover img {
    opacity: 0.65;
  }
  .rune.on img {
    filter: none;
    opacity: 1;
  }
  .rune.minor.on,
  .rune.shard.on {
    box-shadow:
      0 0 0 1.5px var(--tree),
      0 0 8px color-mix(in srgb, var(--tree) 45%, transparent);
    background: #0a1428;
  }
  .rune.minor.on img {
    transform: scale(0.86);
  }
  .rune.keystone.on {
    filter: drop-shadow(0 0 6px color-mix(in srgb, var(--tree) 55%, transparent));
  }
  .shards {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 4px;
    padding-top: 7px;
    border-top: 1px solid var(--line);
    align-self: stretch;
  }
  .shard-row {
    gap: 10px;
  }
  .rune.shard {
    background: #0a1428;
  }
  .rune.shard.on {
    box-shadow:
      0 0 0 1.5px var(--gold-hi),
      0 0 6px rgba(200, 170, 110, 0.4);
  }
</style>
