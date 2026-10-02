<script lang="ts">
  import { onMount } from "svelte";
  import { isMock } from "./lib/api";
  import { app, VIEWS, type View } from "./lib/app.svelte";
  import ConnectionPill from "./lib/components/ConnectionPill.svelte";
  import Notice from "./lib/components/Notice.svelte";
  import OverwriteModal from "./lib/components/OverwriteModal.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import Live from "./views/Live.svelte";
  import Lookup from "./views/Lookup.svelte";
  import Pool from "./views/Pool.svelte";
  import Settings from "./views/Settings.svelte";

  const LABELS: Record<View, string> = { live: "Live", lookup: "Lookup", pool: "Pool", settings: "Settings" };

  onMount(() => {
    if (isMock) {
      // Dev only: ?view=lookup etc. to open a screen directly.
      const v = new URLSearchParams(location.search).get("view") as View | null;
      if (v && VIEWS.includes(v)) app.view = v;
    }
    app.init();
  });

  // Ctrl+1..4 switches screens.
  function onkeydown(e: KeyboardEvent) {
    if (e.ctrlKey && !e.altKey && !e.shiftKey && /^[1-4]$/.test(e.key)) {
      e.preventDefault();
      app.view = VIEWS[Number(e.key) - 1];
    }
  }

  let main: HTMLElement | undefined = $state();
  $effect(() => {
    void app.view;
    main?.scrollTo(0, 0);
  });

  const needsStatic = $derived(app.view !== "settings");
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <header class="topbar">
    <div class="brand">
      <svg class="logo" viewBox="0 0 32 32" width="22" height="22" aria-hidden="true">
        <path d="M16 2 29 9.5v13L16 30 3 22.5v-13z" fill="none" stroke="#c89b3c" stroke-width="1.6" />
        <path d="M16 7.5 24 12v8l-8 4.5L8 20v-8z" fill="#c89b3c" opacity=".18" />
        <path d="m11 17 3.2 3.2L21.5 12" fill="none" stroke="#f0e6d2" stroke-width="2.2" />
      </svg>
      <span class="brand-text">Champ Select <b>Helper</b></span>
    </div>

    <nav aria-label="Screens">
      {#each VIEWS as v, i (v)}
        <button
          class:active={app.view === v}
          aria-current={app.view === v ? "page" : undefined}
          onclick={() => (app.view = v)}
          title="{LABELS[v]} (Ctrl+{i + 1})"
        >
          {LABELS[v]}
          {#if v === "live" && app.cs.in_champ_select}<span class="live-dot" aria-label="in champ select"></span>{/if}
        </button>
      {/each}
    </nav>

    <ConnectionPill />
  </header>

  <main bind:this={main}>
    <div class="page">
      {#if needsStatic && !app.staticData}
        {#if app.staticError}
          <Notice
            big
            kind="error"
            title="Couldn't load champion data"
            detail={`${app.staticError.replace(/\.$/, "")}. Check your internet connection and try again.`}
          >
            <button class="btn" onclick={() => app.loadStatic()}>Retry</button>
          </Notice>
        {:else}
          <Notice big kind="loading" title="Loading champion data…" />
        {/if}
      {:else if app.view === "live"}
        <Live />
      {:else if app.view === "lookup"}
        <Lookup />
      {:else if app.view === "pool"}
        <Pool />
      {:else}
        <Settings />
      {/if}
    </div>
  </main>

  <Toasts />
  <OverwriteModal />
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    background:
      radial-gradient(ellipse 80% 50% at 50% -10%, rgba(10, 50, 80, 0.35), transparent 70%),
      var(--bg);
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 22px;
    height: 46px;
    flex: none;
    padding: 0 16px;
    background: linear-gradient(180deg, #0a1428, #050d19);
    border-bottom: 1px solid var(--line-gold);
    box-shadow: 0 1px 0 #010a13;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
  }
  .brand-text {
    font: 600 12px/1 var(--font-head);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--gold-text);
  }
  .brand-text b {
    color: var(--text);
  }
  nav {
    display: flex;
    align-self: stretch;
    flex: 1;
  }
  nav button {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 14px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font: 700 12px/1 var(--font-head);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    transition: color 0.12s;
  }
  nav button:hover {
    color: var(--text);
  }
  nav button.active {
    color: var(--text);
  }
  nav button.active::after {
    content: "";
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: -1px;
    height: 2px;
    background: linear-gradient(90deg, transparent, var(--gold), transparent);
  }
  nav button.active::before {
    content: "";
    position: absolute;
    left: 50%;
    bottom: 1px;
    width: 5px;
    height: 5px;
    background: var(--gold-hi);
    transform: translateX(-50%) rotate(45deg);
  }
  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--gold);
    box-shadow: 0 0 6px var(--gold);
    animation: pulse 1.6s ease-in-out infinite;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .page {
    max-width: 1280px;
    margin: 0 auto;
    padding: 14px 16px 24px;
  }

  @media (max-width: 860px) {
    .brand-text {
      display: none;
    }
    .topbar {
      gap: 12px;
    }
    nav button {
      padding: 0 10px;
    }
  }
</style>
