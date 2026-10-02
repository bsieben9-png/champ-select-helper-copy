<script lang="ts">
  import { LOW_SAMPLE, games as fmtGames, num, pct, wrTone } from "../format";
  import { tip } from "../tooltip";

  let {
    wr,
    games,
    stack = false,
    left = false,
    gamesWord = true,
    size = "md",
  }: {
    wr: number;
    games?: number;
    /** WR above games instead of side by side. */
    stack?: boolean;
    /** With `stack`: left-align instead of centering. */
    left?: boolean;
    gamesWord?: boolean;
    size?: "sm" | "md" | "lg";
  } = $props();

  const low = $derived(games !== undefined && games < LOW_SAMPLE);
</script>

<span
  class="wr {size}"
  class:stack
  class:left
  class:low
  use:tip={low
    ? { title: "Small sample", body: `Only ${num(games ?? 0)} games — this win rate is not very reliable.` }
    : null}
>
  <b class="num {wrTone(wr)}">{pct(wr)}</b>
  {#if games !== undefined}
    <span class="games num">{fmtGames(games)}{gamesWord ? (games === 1 ? " game" : " games") : ""}</span>
  {/if}
</span>

<style>
  .wr {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    white-space: nowrap;
    line-height: 1.2;
  }
  .wr.stack {
    flex-direction: column;
    align-items: center;
    gap: 0;
  }
  .wr.stack.left {
    align-items: flex-start;
    gap: 1px;
  }
  b {
    font-weight: 700;
  }
  .sm b {
    font-size: 11px;
  }
  .md b {
    font-size: 12.5px;
  }
  .lg b {
    font-size: 15px;
  }
  .games {
    color: var(--muted);
    font-size: 11px;
  }
  .sm .games {
    font-size: 10px;
  }
  .low b {
    opacity: 0.6;
  }
  .low .games {
    text-decoration: underline dotted var(--faint);
    text-underline-offset: 2px;
    color: var(--faint);
  }
</style>
