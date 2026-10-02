<script lang="ts">
  import { app } from "../lib/app.svelte";
  import ChampIcon from "../lib/components/ChampIcon.svelte";
  import Notice from "../lib/components/Notice.svelte";
  import RoleIcon from "../lib/components/RoleIcon.svelte";
  import Segmented from "../lib/components/Segmented.svelte";
  import Toggle from "../lib/components/Toggle.svelte";
  import { ROLE_LABEL, ROLE_SHORT, searchKey } from "../lib/format";
  import { ROLES, type Role } from "../lib/types";

  let query = $state("");
  let roleFilter = $state<Role | "all">("all");
  let onlyMine = $state(false);

  const shown = $derived.by(() => {
    const q = searchKey(query);
    return app.champList.filter(
      (c) =>
        (!q || searchKey(c.name).includes(q)) &&
        (roleFilter === "all" || c.roles.includes(roleFilter)) &&
        (!onlyMine || app.pool.has(c.id)),
    );
  });
  const poolList = $derived((app.settings?.champion_pool ?? []).filter((id) => app.champs.has(id)));
</script>

<div class="pool">
  <div class="head">
    <div>
      <h1>Champion pool</h1>
      <p class="muted">
        Champions you play. They're shown first (★) in counter picks and tier lists — or exclusively with “Only my pool”.
      </p>
    </div>
  </div>

  <div class="mine panel gold">
    <span class="eyebrow">Your pool <span class="faint num">{poolList.length}</span></span>
    {#if poolList.length}
      <div class="mine-list">
        {#each poolList as id (id)}
          <button
            class="chip"
            onclick={() => app.togglePool(id)}
            aria-label="Remove {app.champName(id)} from pool"
            title="Remove from pool"
          >
            <ChampIcon {id} size={22} tooltip={false} />
            <span>{app.champName(id)}</span>
            <svg viewBox="0 0 24 24" width="9" height="9" aria-hidden="true"
              ><path d="M5 5l14 14M19 5 5 19" stroke="currentColor" stroke-width="3" /></svg
            >
          </button>
        {/each}
      </div>
    {:else}
      <span class="faint">Empty — click champions below to add them.</span>
    {/if}
  </div>

  <div class="filters">
    <div class="search">
      <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
        <circle cx="10.5" cy="10.5" r="6" fill="none" stroke="currentColor" stroke-width="2" />
        <path d="m15 15 5.5 5.5" stroke="currentColor" stroke-width="2.2" />
      </svg>
      <input class="input" type="search" placeholder="Search champions…" aria-label="Search champions" bind:value={query} />
    </div>
    <Segmented
      label="Filter by role"
      value={roleFilter}
      onchange={(v) => (roleFilter = v)}
      options={[
        { value: "all" as const, label: "All" },
        ...ROLES.map((r) => ({ value: r, label: ROLE_SHORT[r], tip: ROLE_LABEL[r] })),
      ]}
    >
      {#snippet item(o)}
        {#if o.value !== "all"}<RoleIcon role={o.value} size={13} />{/if}<span class="rl">{o.label}</span>
      {/snippet}
    </Segmented>
    <Toggle label="Only my pool" checked={onlyMine} onchange={(v) => (onlyMine = v)} />
    <span class="spacer"></span>
    <span class="faint count">{shown.length} champions</span>
  </div>

  {#if shown.length}
    <div class="grid">
      {#each shown as c (c.id)}
        {@const on = app.pool.has(c.id)}
        <button
          class="tile"
          class:on
          aria-pressed={on}
          onclick={() => app.togglePool(c.id)}
          title={on ? `Remove ${c.name} from pool` : `Add ${c.name} to pool`}
        >
          <span class="portrait">
            <ChampIcon id={c.id} size={52} tooltip={false} />
            {#if on}<span class="star" aria-hidden="true">★</span>{/if}
          </span>
          <span class="name">{c.name}</span>
        </button>
      {/each}
    </div>
  {:else}
    <Notice title="No champions found" detail="Try another name or role filter." />
  {/if}
</div>

<style>
  .pool {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h1 {
    margin: 0 0 3px;
    font: 700 16px/1.2 var(--font-head);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--gold-hi);
  }
  .head p {
    margin: 0;
    font-size: 12.5px;
  }
  .mine {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 7px 12px;
  }
  .mine .eyebrow {
    flex: none;
  }
  .mine-list {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px 0 2px;
    border: 1px solid var(--line-gold);
    background: #010a13;
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
  }
  .chip svg {
    color: var(--faint);
  }
  .chip:hover {
    border-color: var(--red);
  }
  .chip:hover svg {
    color: var(--red);
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
  }
  .search svg {
    position: absolute;
    left: 9px;
    color: var(--faint);
    pointer-events: none;
  }
  .search .input {
    width: 220px;
    padding-left: 30px;
    user-select: text;
    -webkit-user-select: text;
  }
  .rl {
    font-size: 11.5px;
  }
  .spacer {
    flex: 1;
  }
  .count {
    font-size: 11.5px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(78px, 1fr));
    gap: 4px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 7px 2px 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--muted);
  }
  .tile:hover {
    background: rgba(255, 255, 255, 0.03);
    border-color: var(--line-2);
    color: var(--text);
  }
  .portrait {
    position: relative;
    display: inline-flex;
    border: 1px solid #262b31;
  }
  .tile:not(.on) .portrait :global(.champ) {
    filter: saturate(0.55) brightness(0.8);
  }
  .tile:hover .portrait :global(.champ) {
    filter: none;
  }
  .tile.on {
    color: var(--text);
  }
  .tile.on .portrait {
    border-color: var(--gold);
    box-shadow: 0 0 10px rgba(200, 155, 60, 0.4);
  }
  .star {
    position: absolute;
    top: -1px;
    right: -1px;
    display: grid;
    place-items: center;
    width: 17px;
    height: 17px;
    background: var(--gold);
    color: #1a1404;
    font-size: 11px;
    line-height: 1;
  }
  .name {
    max-width: 100%;
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
