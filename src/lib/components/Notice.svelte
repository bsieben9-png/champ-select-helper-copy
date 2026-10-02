<script lang="ts">
  import type { Snippet } from "svelte";
  import Spinner from "./Spinner.svelte";

  let {
    kind = "empty",
    title,
    detail,
    children,
    big = false,
  }: {
    kind?: "empty" | "error" | "loading";
    title: string;
    detail?: string;
    /** Action buttons. */
    children?: Snippet;
    big?: boolean;
  } = $props();
</script>

<div class="notice {kind}" class:big role={kind === "error" ? "alert" : undefined}>
  <div class="mark" aria-hidden="true">
    {#if kind === "loading"}
      <Spinner size={big ? 26 : 18} />
    {:else if kind === "error"}
      <svg viewBox="0 0 24 24" width={big ? 30 : 20} height={big ? 30 : 20}>
        <path d="M12 2.5 22.5 21h-21z" fill="none" stroke="currentColor" stroke-width="1.6" />
        <path d="M11.1 9h1.8l-.3 6h-1.2zM12 16.6a1.1 1.1 0 1 1 0 2.2 1.1 1.1 0 0 1 0-2.2z" fill="currentColor" />
      </svg>
    {:else}
      <svg viewBox="0 0 24 24" width={big ? 34 : 22} height={big ? 34 : 22}>
        <path d="M12 2 21 12 12 22 3 12z" fill="none" stroke="currentColor" stroke-width="1.4" />
        <path d="M12 7 16.5 12 12 17 7.5 12z" fill="currentColor" opacity=".55" />
      </svg>
    {/if}
  </div>
  <div class="title">{title}</div>
  {#if detail}<div class="detail">{detail}</div>{/if}
  {#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
  .notice {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
    padding: 22px 16px;
    color: var(--muted);
  }
  .notice.big {
    padding: 56px 24px;
    gap: 8px;
  }
  .mark {
    color: var(--gold-dim);
    margin-bottom: 2px;
  }
  .error .mark {
    color: var(--red);
  }
  .title {
    color: var(--text);
    font: 600 14px/1.3 var(--font-head);
  }
  .big .title {
    font-size: 18px;
    letter-spacing: 0.01em;
  }
  .detail {
    max-width: 460px;
    font-size: 12.5px;
    line-height: 1.5;
    user-select: text;
    -webkit-user-select: text;
  }
  .error .detail {
    color: #c9a3a9;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
</style>
