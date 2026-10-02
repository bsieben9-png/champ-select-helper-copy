<script lang="ts">
  import { fly } from "svelte/transition";
  import { app } from "../app.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "error" ? "alert" : "status"} transition:fly={{ x: 24, duration: 180 }}>
      <span class="icon" aria-hidden="true">
        {#if t.kind === "success"}
          <svg viewBox="0 0 24 24" width="16" height="16"><path d="m4.5 12.5 5 5 10-11" fill="none" stroke="currentColor" stroke-width="2.6" /></svg>
        {:else if t.kind === "error"}
          <svg viewBox="0 0 24 24" width="16" height="16"><path d="M6 6l12 12M18 6 6 18" stroke="currentColor" stroke-width="2.6" /></svg>
        {:else}
          <svg viewBox="0 0 24 24" width="16" height="16"><path d="M11 5h2.2l-.3 9h-1.6zM12.1 16.5a1.4 1.4 0 1 1 0 2.8 1.4 1.4 0 0 1 0-2.8z" fill="currentColor" /></svg>
        {/if}
      </span>
      <div class="text">
        {#if t.tag}<div class="tag">{t.tag}</div>{/if}
        <div class="title">{t.title}</div>
        {#if t.detail}<div class="detail">{t.detail}</div>{/if}
      </div>
      <button class="icon-btn close" aria-label="Dismiss" onclick={() => app.dismiss(t.id)}>
        <svg viewBox="0 0 24 24" width="11" height="11"><path d="M5 5l14 14M19 5 5 19" stroke="currentColor" stroke-width="2.6" /></svg>
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 14px;
    bottom: 14px;
    z-index: 900;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(380px, calc(100vw - 28px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 10px 10px 12px;
    background: linear-gradient(180deg, #0c1a31, #08121f);
    border: 1px solid var(--gold-dim);
    border-left-width: 3px;
    box-shadow: 0 10px 26px rgba(0, 0, 0, 0.6);
  }
  .toast.success {
    border-left-color: var(--teal);
  }
  .toast.error {
    border-color: #6b2632;
    border-left-color: var(--red);
  }
  .toast.warn {
    border-left-color: var(--gold);
  }
  .icon {
    margin-top: 1px;
    color: var(--gold-hi);
  }
  .success .icon {
    color: var(--teal);
  }
  .error .icon {
    color: var(--red);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .tag {
    font: 700 10px/1.2 var(--font-head);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--gold);
    margin-bottom: 2px;
  }
  .title {
    font-weight: 600;
    color: var(--text);
    line-height: 1.35;
  }
  .detail {
    margin-top: 3px;
    font-size: 12px;
    color: var(--muted);
    user-select: text;
    -webkit-user-select: text;
  }
  .close {
    margin: -2px -2px 0 0;
  }
</style>
