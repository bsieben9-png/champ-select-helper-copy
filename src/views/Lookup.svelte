<script lang="ts">
  import { app, type LookupTab } from "../lib/app.svelte";
  import BuildPanel from "../lib/components/BuildPanel.svelte";
  import ChampIcon from "../lib/components/ChampIcon.svelte";
  import ChampPicker from "../lib/components/ChampPicker.svelte";
  import CounterGrid from "../lib/components/CounterGrid.svelte";
  import MatchupList from "../lib/components/MatchupList.svelte";
  import Notice from "../lib/components/Notice.svelte";
  import RoleIcon from "../lib/components/RoleIcon.svelte";
  import Segmented from "../lib/components/Segmented.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TierList from "../lib/components/TierList.svelte";
  import { LOOKUP_QUEUES, ROLE_LABEL, ROLE_SHORT } from "../lib/format";
  import { tip } from "../lib/tooltip";
  import { ROLES, type Build, type Queue, type Role } from "../lib/types";

  const L = app.lookup;
  const champ = $derived(L.championId ? app.champs.get(L.championId) : undefined);
  const role = $derived<Role>(L.role ?? champ?.roles[0] ?? "top");
  const name = $derived(champ?.name ?? "");
  let importing = $state(false);

  function setChampion(id: number | null) {
    L.championId = id;
    if (id) {
      const roles = app.champs.get(id)?.roles ?? [];
      // Keep the current role if the new champion plays it, else its main role.
      if (!L.role || !roles.includes(L.role)) L.role = roles[0] ?? L.role ?? "top";
      if (L.opponentId === id) L.opponentId = null;
    }
  }

  const tabs = $derived<{ id: LookupTab; label: string; disabled: boolean }[]>([
    { id: "build", label: "Build", disabled: !L.championId },
    { id: "counters", label: name ? `Who beats ${name}` : "Counters", disabled: !L.championId },
    { id: "matchups", label: "Matchups", disabled: !L.championId },
    { id: "tiers", label: `Tier list · ${ROLE_SHORT[role]}`, disabled: false },
  ]);
  // Fall back to an enabled tab when the current one becomes unavailable.
  const tab = $derived<LookupTab>(
    tabs.find((t) => t.id === L.tab && !t.disabled)?.id ?? (L.championId ? "build" : "tiers"),
  );

  async function doImport(build: Build) {
    importing = true;
    await app.importBuild(build);
    importing = false;
  }
</script>

<div class="lookup">
  <div class="controls panel">
    <div class="field">
      <span class="eyebrow">Champion</span>
      <ChampPicker label="Champion" value={L.championId} onchange={setChampion} width={210} />
    </div>
    <div class="field">
      <span class="eyebrow">Role</span>
      <Segmented
        label="Role"
        value={role}
        onchange={(r: Role) => (L.role = r)}
        options={ROLES.map((r) => ({
          value: r,
          label: ROLE_SHORT[r],
          dim: !!champ && champ.roles.length > 0 && !champ.roles.includes(r),
          tip: champ && champ.roles.length > 0 && !champ.roles.includes(r)
            ? { title: ROLE_LABEL[r], body: `${champ.name} is rarely played ${ROLE_LABEL[r]} — data may be thin.` }
            : ROLE_LABEL[r],
        }))}
      >
        {#snippet item(o)}<RoleIcon role={o.value} size={14} /><span class="rl">{o.label}</span>{/snippet}
      </Segmented>
    </div>
    <div class="field">
      <span class="eyebrow">Opponent</span>
      <ChampPicker
        label="Opponent"
        value={L.opponentId}
        onchange={(id) => (L.opponentId = id)}
        placeholder="Any opponent"
        clearable
        exclude={L.championId}
        width={190}
      />
    </div>
    <div class="field">
      <span class="eyebrow">Queue</span>
      <select class="select" aria-label="Queue" value={L.queue} onchange={(e) => (L.queue = e.currentTarget.value as Queue)}>
        {#each LOOKUP_QUEUES as q (q.value)}<option value={q.value}>{q.label}</option>{/each}
      </select>
    </div>
  </div>

  <div class="tabs" role="tablist" aria-label="Lookup">
    {#each tabs as t (t.id)}
      <button role="tab" aria-selected={tab === t.id} class:active={tab === t.id} disabled={t.disabled} onclick={() => (L.tab = t.id)}
        >{t.label}</button
      >
    {/each}
  </div>

  {#if tab === "build" && L.championId}
    <BuildPanel
      championId={L.championId}
      {role}
      opponentId={L.opponentId}
      queue={L.queue}
      showRoleTabs={false}
    >
      {#snippet actions(build)}
        <button
          class="btn primary"
          disabled={!build || importing || !app.lcu.connected}
          onclick={() => build && doImport(build)}
          use:tip={app.lcu.connected
            ? { title: "Import runes & item set", body: "Summoner spells are only a recommendation and are never changed." }
            : { title: "League client isn't running", body: "Start League to import runes and item sets." }}
        >
          {#if importing}<Spinner size={12} />{/if}Import
        </button>
      {/snippet}
    </BuildPanel>
  {:else if tab === "counters" && L.championId}
    <CounterGrid enemyId={L.championId} {role} queue={L.queue} limit={30}>
      {#snippet title()}
        <span class="eyebrow">Who beats</span>
        <ChampIcon id={L.championId} size={22} />
        <span class="sec-name">{name}</span>
        <span class="faint">· {ROLE_LABEL[role]}</span>
      {/snippet}
    </CounterGrid>
  {:else if tab === "matchups" && L.championId}
    <MatchupList championId={L.championId} {role} queue={L.queue} />
  {:else if tab === "tiers"}
    <TierList {role} queue={L.queue}>
      {#snippet title()}
        <span class="eyebrow">Tier list — {ROLE_LABEL[role]}</span>
      {/snippet}
    </TierList>
  {:else}
    <Notice
      title="Pick a champion"
      detail="Search for any champion to see its build, who counters it and its best and worst matchups — works with League closed."
    >
      {#if app.pool.size}
        <div class="quick">
          {#each [...app.pool] as id (id)}
            <button class="quick-btn" onclick={() => setChampion(id)} aria-label="Look up {app.champName(id)}">
              <ChampIcon {id} size={36} />
            </button>
          {/each}
        </div>
      {/if}
    </Notice>
  {/if}
</div>

<style>
  .lookup {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
    padding: 10px 12px 12px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .field .eyebrow {
    font-size: 10px;
    color: var(--muted);
  }
  .rl {
    font-size: 11.5px;
  }
  .select {
    height: 32px;
    min-width: 140px;
  }
  .tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    position: relative;
    height: 32px;
    padding: 0 14px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font: 700 11.5px/1 var(--font-head);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .tabs button:hover:not(:disabled) {
    color: var(--text);
  }
  .tabs button:disabled {
    color: #3c3f44;
    cursor: default;
  }
  .tabs button.active {
    color: var(--gold-hi);
  }
  .tabs button.active::after {
    content: "";
    position: absolute;
    left: 8px;
    right: 8px;
    bottom: -1px;
    height: 2px;
    background: var(--gold);
    box-shadow: 0 0 8px rgba(200, 155, 60, 0.6);
  }
  .sec-name {
    font-weight: 700;
    font-size: 14px;
  }
  .quick {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }
  .quick-btn {
    padding: 0;
    border: 1px solid var(--line-gold);
    background: none;
    line-height: 0;
  }
  .quick-btn:hover {
    border-color: var(--gold-hi);
  }
</style>
