<script lang="ts">
  import type { Snippet } from "svelte";
  import * as api from "../api";
  import { app } from "../app.svelte";
  import { ROLE_LABEL, patchLabel, rankLabel, regionLabel, sourceLabel } from "../format";
  import { Loader } from "../loader.svelte";
  import type { Build, Queue, Role } from "../types";
  import ChampIcon from "./ChampIcon.svelte";
  import Img from "./Img.svelte";
  import Items from "./Items.svelte";
  import Notice from "./Notice.svelte";
  import RoleIcon from "./RoleIcon.svelte";
  import Runes from "./Runes.svelte";
  import SkillOrder from "./SkillOrder.svelte";
  import Spinner from "./Spinner.svelte";
  import WinRate from "./WinRate.svelte";

  let {
    championId,
    role,
    opponentId,
    queue,
    onRole,
    showRoleTabs = true,
    status,
    actions,
  }: {
    championId: number;
    role: Role | null;
    opponentId: number | null;
    queue: Queue;
    onRole?: (r: Role) => void;
    showRoleTabs?: boolean;
    /** e.g. "Hovering" / "Locked in" */
    status?: string;
    /** Right side of the header (Import button etc.). */
    actions?: Snippet<[Build | null]>;
  } = $props();

  const loader = new Loader<Build>();

  $effect(() => {
    const args = [championId, role, opponentId, queue] as const;
    void app.statsVersion; // refetch when rank/region/... change
    loader.run(() => api.getBuild(...args));
  });

  const build = $derived(loader.data);
  // Show the build's own role (the backend may pick the main role for `null`).
  const shownRole = $derived(build?.role ?? role);
  const spellTip = (id: number) => app.spells.get(id)?.name ?? `Spell ${id}`;
</script>

<section class="build panel gold" aria-busy={loader.loading}>
  <header class="head">
    <ChampIcon id={championId} size={46} />
    <div class="title-block">
      <div class="title">
        <span class="name">{app.champName(championId)}</span>
        {#if opponentId}
          <span class="vs">vs</span>
          <ChampIcon id={opponentId} size={22} />
          <span class="name opp">{app.champName(opponentId)}</span>
        {/if}
        {#if status}<span class="status">{status}</span>{/if}
        {#if loader.loading && build}<Spinner size={13} />{/if}
      </div>
      <div class="meta">
        {#if shownRole}
          <span class="role"><RoleIcon role={shownRole} size={13} />{ROLE_LABEL[shownRole]}</span>
        {/if}
        {#if shownRole && (build || loader.loading)}<span class="sep">·</span>{/if}
        {#if build}
          <WinRate wr={build.win_rate} games={build.games} />
          <span class="sep">·</span>
          <span>{rankLabel(build.rank)} {regionLabel(build.region)}</span>
          <span class="sep">·</span>
          <span class="nowrap">Patch {patchLabel(build.patch)} <span class="source">via {sourceLabel(build.source)}</span></span>
        {:else if loader.loading}
          <span class="skeleton bar" style:width="220px"></span>
        {/if}
      </div>
    </div>
    <!-- No build while the next one loads: the previous (still shown) build
         may be another champion's, and must not be imported by mistake. -->
    {#if actions}<div class="actions">{@render actions(loader.loading ? null : build)}</div>{/if}
  </header>

  {#if build && (build.fell_back_to_general || (showRoleTabs && build.available_roles.length > 1))}
    <div class="subbar">
      {#if showRoleTabs && build.available_roles.length > 1}
        <div class="roles" role="tablist" aria-label="Role">
          {#each build.available_roles as r (r)}
            <button
              role="tab"
              aria-selected={r === shownRole}
              class:active={r === shownRole}
              onclick={() => onRole?.(r)}
              disabled={!onRole}
            >
              <RoleIcon role={r} size={13} />{ROLE_LABEL[r]}
            </button>
          {/each}
        </div>
      {/if}
      {#if build.fell_back_to_general}
        <div class="fallback" role="note">
          <svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"
            ><circle cx="12" cy="12" r="9.5" fill="none" stroke="currentColor" stroke-width="2" /><path
              d="M11 7h2v7h-2zM11 15.5h2v2h-2z"
              fill="currentColor"
            /></svg
          >
          Not enough matchup data — showing general build
        </div>
      {/if}
    </div>
  {/if}

  {#if loader.error}
    <Notice
      kind="error"
      title={/no .*data|not enough|too few/i.test(loader.error)
        ? `No build data for ${app.champName(championId)}${role ? ` ${ROLE_LABEL[role]}` : ""}`
        : "Couldn't load the build"}
      detail={loader.error}
    >
      <button class="btn small" onclick={loader.retry}>Retry</button>
    </Notice>
  {:else if build}
    <div class="body" class:stale={loader.loading}>
      <div class="card runes-card">
        <div class="card-head">
          <span class="eyebrow">Runes</span>
          <WinRate wr={build.runes.win_rate} games={build.runes.games} size="sm" />
        </div>
        <Runes runes={build.runes} />
      </div>

      <div class="card side">
        <div class="side-top">
          <div>
            <div class="card-head">
              <span class="eyebrow">Summoner spells</span>
            </div>
            <div class="spells">
              {#each build.spells.ids as id, i (i)}
                <Img src={app.spells.get(id)?.icon} size={32} alt={spellTip(id)} tooltip={spellTip(id)} />
              {/each}
              <div class="spell-meta">
                <WinRate wr={build.spells.win_rate} games={build.spells.games} size="sm" />
                <span class="rec">Recommended · not imported</span>
              </div>
            </div>
          </div>
          {#if build.skill_priority}
            <div class="priority-block">
              <div class="card-head"><span class="eyebrow">Skill priority</span></div>
              <div class="priority" aria-label="Max order {build.skill_priority.split('').join(', ')}">
                {#each build.skill_priority.split("") as k, i (i)}
                  {#if i > 0}<span class="gt" aria-hidden="true">›</span>{/if}
                  <span class="pkey">{k}</span>
                {/each}
              </div>
            </div>
          {/if}
        </div>
        <div class="card-head skills-head"><span class="eyebrow">Skill order</span></div>
        <SkillOrder order={build.skill_order} />
      </div>

      <div class="card items-card">
        <Items {build} />
      </div>
    </div>
  {:else if loader.loading}
    <div class="body loading" aria-hidden="true">
      <div class="card runes-card"><div class="skeleton block" style:height="196px" style:width="300px"></div></div>
      <div class="card side"><div class="skeleton block" style:height="196px"></div></div>
      <div class="card items-card"><div class="skeleton block" style:height="82px"></div></div>
    </div>
  {/if}
</section>

<style>
  .build {
    container-type: inline-size;
    padding: 12px 14px 14px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .head > :global(.champ) {
    border: 1px solid var(--gold-dim);
  }
  .title-block {
    flex: 1;
    min-width: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .name {
    font: 700 17px/1.2 var(--font-head);
    color: var(--text);
    letter-spacing: 0.01em;
    white-space: nowrap;
  }
  .name.opp {
    color: var(--gold-hi);
  }
  .vs {
    font-size: 11px;
    font-weight: 700;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .status {
    margin-left: 4px;
    padding: 2px 7px;
    border: 1px solid var(--line-gold);
    color: var(--gold-hi);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 3px;
    color: var(--muted);
    font-size: 12px;
  }
  .meta .role {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text);
    font-weight: 600;
  }
  .sep {
    color: var(--faint);
  }
  .nowrap {
    white-space: nowrap;
  }
  .source {
    margin-left: 4px;
    color: var(--faint);
    font-size: 11px;
  }
  .skeleton.bar {
    display: inline-block;
    height: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: none;
  }

  .subbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 14px;
    margin-top: 10px;
  }
  .roles {
    display: flex;
    gap: 2px;
  }
  .roles button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--muted);
    font-size: 11.5px;
    font-weight: 600;
  }
  .roles button:hover:not(:disabled) {
    color: var(--text);
  }
  .roles button:disabled {
    cursor: default;
  }
  .roles button.active {
    color: var(--gold-hi);
    border-color: var(--line-gold);
    background: #0a1428;
  }
  .fallback {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 9px;
    font-size: 11.5px;
    color: #e2c27a;
    background: rgba(200, 155, 60, 0.08);
    border: 1px solid rgba(200, 155, 60, 0.3);
  }

  .body {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 10px;
    margin-top: 12px;
    transition: opacity 0.15s;
  }
  .body.stale {
    opacity: 0.55;
  }
  .card {
    background: rgba(1, 10, 19, 0.55);
    border: 1px solid var(--line);
    padding: 10px 12px 12px;
    min-width: 0;
  }
  .card-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 8px;
  }
  .items-card {
    grid-column: 1 / -1;
  }
  .side {
    display: flex;
    flex-direction: column;
  }
  .side-top {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 28px;
    margin-bottom: 12px;
  }
  .spells {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .spells > :global(.img) {
    border: 1px solid var(--line-gold);
  }
  .spell-meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin-left: 6px;
  }
  .rec {
    font-size: 10.5px;
    color: var(--faint);
    white-space: nowrap;
  }
  .priority {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 32px;
  }
  .pkey {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    font: 700 13px/1 var(--font-head);
    color: #e8fffc;
    background: linear-gradient(180deg, #0a7d86, #075a63);
    border: 1px solid var(--teal);
  }
  .gt {
    color: var(--gold-dim);
    font-size: 17px;
  }
  .skills-head {
    margin-bottom: 6px;
  }
  .block {
    width: 100%;
  }
  .loading .card {
    padding: 10px;
  }

  @container (max-width: 640px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
