<script lang="ts">
  import { tick } from "svelte";
  import { app } from "../app.svelte";
  import { ROLE_LABEL, searchKey } from "../format";
  import type { ChampionInfo } from "../types";
  import ChampIcon from "./ChampIcon.svelte";

  let {
    value,
    onchange,
    placeholder = "Search champion…",
    clearable = false,
    exclude = null,
    label,
    disabled = false,
    width = 220,
  }: {
    value: number | null;
    onchange: (id: number | null) => void;
    placeholder?: string;
    clearable?: boolean;
    exclude?: number | null;
    /** Accessible label. */
    label: string;
    disabled?: boolean;
    width?: number;
  } = $props();

  const uid = `cp-${Math.random().toString(36).slice(2, 8)}`;
  let query = $state("");
  let open = $state(false);
  let active = $state(0);
  let input: HTMLInputElement;
  let list: HTMLDivElement | undefined = $state();

  const selected = $derived(value ? (app.champs.get(value) ?? null) : null);

  const results = $derived.by((): ChampionInfo[] => {
    const all = app.champList.filter((c) => c.id !== exclude);
    const q = searchKey(query);
    if (!q) return all;
    const starts: ChampionInfo[] = [];
    const contains: ChampionInfo[] = [];
    for (const c of all) {
      const n = searchKey(c.name);
      if (n.startsWith(q) || c.key.toLowerCase().startsWith(q)) starts.push(c);
      else if (n.includes(q)) contains.push(c);
    }
    return [...starts, ...contains];
  });

  async function scrollActive() {
    await tick();
    list?.querySelector(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function show() {
    if (open) return;
    query = "";
    open = true;
    active = Math.max(0, results.findIndex((c) => c.id === value));
    scrollActive();
  }

  function close() {
    open = false;
    query = "";
  }

  function choose(c: ChampionInfo) {
    close();
    onchange(c.id);
    input.blur();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!open) return show();
      const d = e.key === "ArrowDown" ? 1 : -1;
      active = (active + d + results.length) % Math.max(results.length, 1);
      scrollActive();
    } else if (e.key === "Enter") {
      if (open && results[active]) {
        e.preventDefault();
        choose(results[active]);
      }
    } else if (e.key === "Escape") {
      if (open) {
        e.preventDefault();
        close();
        input.blur();
      }
    } else if (e.key === "Tab") {
      close();
    }
  }

  function oninput(e: Event) {
    query = (e.currentTarget as HTMLInputElement).value;
    open = true;
    active = 0;
    scrollActive();
  }
</script>

<div class="picker" class:open class:disabled style:width="{width}px">
  <span class="lead" aria-hidden="true">
    {#if selected && !open}
      <ChampIcon id={selected.id} size={22} tooltip={false} />
    {:else}
      <svg viewBox="0 0 24 24" width="15" height="15">
        <circle cx="10.5" cy="10.5" r="6" fill="none" stroke="currentColor" stroke-width="2" />
        <path d="m15 15 5.5 5.5" stroke="currentColor" stroke-width="2.2" />
      </svg>
    {/if}
  </span>
  <input
    bind:this={input}
    type="text"
    role="combobox"
    aria-label={label}
    aria-expanded={open}
    aria-controls="{uid}-list"
    aria-autocomplete="list"
    aria-activedescendant={open && results[active] ? `${uid}-${active}` : undefined}
    autocomplete="off"
    spellcheck="false"
    {disabled}
    placeholder={open && selected ? selected.name : placeholder}
    value={open ? query : (selected?.name ?? "")}
    onfocus={show}
    onclick={show}
    onblur={close}
    {oninput}
    {onkeydown}
  />
  {#if clearable && selected && !open}
    <button
      type="button"
      class="clear icon-btn"
      aria-label="Clear {label}"
      onclick={() => onchange(null)}
    >
      <svg viewBox="0 0 24 24" width="12" height="12"
        ><path d="M5 5l14 14M19 5 5 19" stroke="currentColor" stroke-width="2.4" /></svg
      >
    </button>
  {:else}
    <span class="caret" aria-hidden="true"></span>
  {/if}

  {#if open}
    <div class="list" id="{uid}-list" role="listbox" aria-label={label} bind:this={list}>
      {#each results as c, i (c.id)}
        <div
          id="{uid}-{i}"
          data-i={i}
          role="option"
          tabindex="-1"
          aria-selected={c.id === value}
          class="opt"
          class:active={i === active}
          class:current={c.id === value}
          onmousedown={(e) => {
            e.preventDefault();
            choose(c);
          }}
          onmousemove={() => (active = i)}
        >
          <ChampIcon id={c.id} size={24} tooltip={false} />
          <span class="name">{c.name}</span>
          {#if app.pool.has(c.id)}<span class="star" aria-label="In your pool">★</span>{/if}
          <span class="roles">{c.roles.slice(0, 2).map((r) => ROLE_LABEL[r]).join(" · ")}</span>
        </div>
      {:else}
        <div class="none">No champion matches “{query}”</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    display: flex;
    align-items: center;
    height: 32px;
    background: #010a13;
    border: 1px solid var(--line-2);
    transition: border-color 0.12s;
  }
  .picker:hover {
    border-color: #3a3f45;
  }
  .picker:focus-within,
  .picker.open {
    border-color: var(--gold-dim);
  }
  .disabled {
    opacity: 0.45;
    pointer-events: none;
  }
  .lead {
    display: grid;
    place-items: center;
    width: 32px;
    flex: none;
    color: var(--faint);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: none;
    background: transparent;
    padding: 0 4px 0 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    user-select: text;
    -webkit-user-select: text;
  }
  input::placeholder {
    color: var(--faint);
    font-weight: 400;
  }
  .clear {
    margin-right: 5px;
  }
  .caret {
    width: 0;
    height: 0;
    margin: 0 11px 0 6px;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-top: 5px solid var(--gold-dim);
  }
  .list {
    position: absolute;
    z-index: 50;
    top: calc(100% + 4px);
    left: -1px;
    right: -1px;
    min-width: 230px;
    max-height: 320px;
    overflow-y: auto;
    background: #010a13;
    border: 1px solid var(--gold-dim);
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.65);
    padding: 3px 0;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 8px;
    cursor: pointer;
  }
  .opt.active {
    background: #0f1d36;
  }
  .opt.current .name {
    color: var(--gold-hi);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .star {
    color: var(--gold);
    font-size: 11px;
  }
  .roles {
    color: var(--faint);
    font-size: 11px;
    white-space: nowrap;
  }
  .none {
    padding: 10px 12px;
    color: var(--muted);
  }
</style>
