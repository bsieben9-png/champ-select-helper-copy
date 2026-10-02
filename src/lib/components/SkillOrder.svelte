<script lang="ts">
  let { order }: { order: string[] } = $props();

  const KEYS = ["Q", "W", "E", "R"] as const;
  const LEVELS = Array.from({ length: 18 }, (_, i) => i + 1);
</script>

{#if order.length}
  <div class="grid" role="table" aria-label="Skill order by level">
    <span class="corner" role="presentation"></span>
    {#each LEVELS as lvl (lvl)}
      <span class="lvl num" role="columnheader">{lvl}</span>
    {/each}
    {#each KEYS as k (k)}
      <span class="key key-{k}" role="rowheader">{k}</span>
      {#each LEVELS as lvl (lvl)}
        {@const on = order[lvl - 1]?.toUpperCase() === k}
        <span class="cell" class:on class:ult={k === "R"} role="cell" aria-label={on ? `Level ${lvl}: ${k}` : undefined}
          >{on ? k : ""}</span
        >
      {/each}
    {/each}
  </div>
{:else}
  <p class="muted none">No skill order data.</p>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: 20px repeat(18, minmax(14px, 1fr));
    gap: 2px;
    max-width: 470px;
  }
  .lvl {
    font-size: 9.5px;
    color: var(--faint);
    text-align: center;
    line-height: 12px;
  }
  .key {
    display: grid;
    place-items: center;
    font: 700 11px/1 var(--font-head);
    color: var(--gold-text);
    background: #1e2328;
    border: 1px solid var(--line-gold);
  }
  .key-R {
    color: var(--gold);
  }
  .cell {
    display: grid;
    place-items: center;
    aspect-ratio: 1;
    min-height: 14px;
    max-height: 24px;
    background: #071021;
    border: 1px solid #111b2c;
    font: 700 10.5px/1 var(--font-head);
    color: #e8fffc;
  }
  .cell.on {
    background: linear-gradient(180deg, #0a7d86, #075a63);
    border-color: var(--teal);
  }
  .cell.on.ult {
    background: linear-gradient(180deg, #8a6a2a, #5b4519);
    border-color: var(--gold-hi);
    color: #fff6e0;
  }
  .none {
    margin: 4px 0;
  }
</style>
