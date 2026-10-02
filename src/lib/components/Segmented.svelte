<script lang="ts" generics="T extends string">
  import type { Snippet } from "svelte";
  import { tip, type TipContent } from "../tooltip";

  let {
    options,
    value,
    onchange,
    label,
    item,
    compact = false,
    disabled = false,
  }: {
    options: { value: T; label: string; dim?: boolean; tip?: TipContent }[];
    value: T | null;
    onchange: (v: T) => void;
    /** Accessible group label. */
    label: string;
    /** Custom button content. */
    item?: Snippet<[{ value: T; label: string }]>;
    compact?: boolean;
    disabled?: boolean;
  } = $props();
</script>

<div class="seg" class:compact class:disabled role="radiogroup" aria-label={label} aria-disabled={disabled}>
  {#each options as o (o.value)}
    <button
      type="button"
      role="radio"
      aria-checked={o.value === value}
      class:active={o.value === value}
      class:dim={o.dim}
      {disabled}
      onclick={() => onchange(o.value)}
      use:tip={o.tip}
    >
      {#if item}{@render item(o)}{:else}{o.label}{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    border: 1px solid var(--line-2);
    background: #010a13;
    height: 32px;
  }
  .seg.compact {
    height: 26px;
  }
  .seg.disabled {
    opacity: 0.4;
  }
  .seg.disabled button {
    cursor: default;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    min-width: 34px;
    padding: 0 10px;
    border: 0;
    border-right: 1px solid var(--line);
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
  .compact button {
    font-size: 11.5px;
    padding: 0 8px;
  }
  button:last-child {
    border-right: 0;
  }
  button:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.03);
  }
  button.dim {
    color: var(--faint);
  }
  button.active {
    color: var(--text);
    background: linear-gradient(180deg, #2a2412, #16130a);
    box-shadow: inset 0 0 0 1px var(--gold-dim);
  }
  button.active.dim {
    color: var(--gold-text);
  }
</style>
