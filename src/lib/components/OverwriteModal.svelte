<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { app } from "../app.svelte";
  import Spinner from "./Spinner.svelte";

  let confirmBtn: HTMLButtonElement | undefined = $state();

  $effect(() => {
    if (app.overwrite) confirmBtn?.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    if (app.overwrite && e.key === "Escape" && !app.overwriting) app.cancelOverwrite();
  }
</script>

<svelte:window {onkeydown} />

{#if app.overwrite}
  {@const o = app.overwrite}
  <div class="backdrop" transition:fade={{ duration: 120 }}>
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="ow-title"
      aria-describedby="ow-body"
      transition:scale={{ start: 0.96, duration: 140 }}
    >
      <div class="frame" aria-hidden="true"></div>
      <h2 id="ow-title">Rune pages full</h2>
      <p id="ow-body">
        All rune pages are full. Overwrite <strong>“{o.page.name}”</strong> with the recommended runes for
        <span class="who">{app.matchupLabel(o.championId, o.opponentId)}</span>?
      </p>
      <div class="buttons">
        <button class="btn primary" bind:this={confirmBtn} disabled={app.overwriting} onclick={() => app.confirmOverwrite()}>
          {#if app.overwriting}<Spinner size={12} />{/if}Overwrite
        </button>
        <button class="btn" disabled={app.overwriting} onclick={() => app.cancelOverwrite()}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 950;
    display: grid;
    place-items: center;
    background: rgba(1, 10, 19, 0.72);
    padding: 16px;
  }
  .dialog {
    position: relative;
    width: min(420px, 100%);
    padding: 26px 28px 22px;
    text-align: center;
    background: linear-gradient(180deg, #0d1b33, #010a13);
    border: 1px solid var(--gold-dim);
    box-shadow:
      0 0 0 1px #010a13,
      0 20px 50px rgba(0, 0, 0, 0.7);
  }
  .frame {
    position: absolute;
    inset: 4px;
    border: 1px solid rgba(200, 155, 60, 0.18);
    pointer-events: none;
  }
  h2 {
    margin: 0 0 10px;
    font: 700 16px/1.2 var(--font-head);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--gold-hi);
  }
  p {
    margin: 0 0 20px;
    color: var(--muted);
    line-height: 1.5;
  }
  strong {
    color: var(--text);
    font-weight: 600;
  }
  .who {
    color: var(--text);
  }
  .buttons {
    display: flex;
    justify-content: center;
    gap: 10px;
  }
  .buttons .btn {
    min-width: 120px;
  }
</style>
