<script lang="ts">
  import * as api from "../lib/api";
  import { app } from "../lib/app.svelte";
  import BuildPanel from "../lib/components/BuildPanel.svelte";
  import ChampIcon from "../lib/components/ChampIcon.svelte";
  import CounterGrid from "../lib/components/CounterGrid.svelte";
  import Notice from "../lib/components/Notice.svelte";
  import RoleIcon from "../lib/components/RoleIcon.svelte";
  import Segmented from "../lib/components/Segmented.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TierList from "../lib/components/TierList.svelte";
  import Toggle from "../lib/components/Toggle.svelte";
  import {
    ROLE_LABEL,
    errorMessage,
    isInGamePhase,
    isLobbyPhase,
    isLobbyPick,
    phaseLabel,
    queueLabel,
  } from "../lib/format";
  import { tip } from "../lib/tooltip";
  import { ROLES, type Build, type Queue, type Role } from "../lib/types";

  const cs = $derived(app.cs);

  /** Enemy clicked by the user as matchup opponent (null = automatic). */
  let override = $state<number | null>(null);
  /** Role chosen in the build panel / role picker (null = assigned role). */
  let roleOverride = $state<Role | null>(null);
  let importing = $state(false);
  let hintBusy = $state(false);
  let hintDismissed = $state<number | null>(null);
  /** Champion for which the "imported runes are for another champion" hint was dismissed. */
  let otherDismissed = $state<number | null>(null);
  /** Swiftplay lobby: the slot whose build is shown. */
  let slotPick = $state(0);

  $effect(() => {
    if (!cs.in_champ_select) {
      override = null;
      roleOverride = null;
      hintDismissed = null;
      otherDismissed = null;
    }
  });
  $effect(() => {
    if (override !== null && !cs.enemies.some((e) => e.champion_id === override)) override = null;
  });

  const queue = $derived<Queue>(cs.queue ?? "ranked_solo");
  const unsupported = $derived(cs.in_champ_select && cs.queue == null && cs.queue_id != null);
  const lobbyPick = $derived(isLobbyPick(queue));
  const myChamp = $derived(cs.my_champion_id || null);
  const opponentId = $derived(override ?? cs.lane_opponent_id ?? null);
  const opponentPick = $derived(cs.enemies.find((e) => e.champion_id === opponentId));
  // Swiftplay: the position of my lobby slot with this champion, if the
  // skip-champ-select step doesn't say.
  const slotRole = $derived(
    lobbyPick ? (app.lastSlots.find((s) => s.champion_id === myChamp)?.role ?? null) : null,
  );
  const myRole = $derived<Role | null>(
    roleOverride ?? cs.my_role ?? slotRole ?? opponentPick?.role ?? null,
  );
  const counterRole = $derived<Role | null>(
    myRole ?? (opponentId ? (app.champs.get(opponentId)?.roles[0] ?? null) : null),
  );
  const bans = $derived(cs.bans.filter((b) => b > 0));
  // While the game runs, keep showing the build picked in champ select.
  const inGame = $derived(!cs.in_champ_select && isInGamePhase(app.lcu.phase));
  // Swiftplay / Quickplay lobby: champions are picked here, per position.
  const lobby = $derived(app.lobby);
  const inLobby = $derived(
    !cs.in_champ_select && isLobbyPhase(app.lcu.phase) && lobby.in_lobby && lobby.slots.length > 0,
  );
  const shownSlot = $derived(
    lobby.slots.find((s) => s.index === slotPick && s.champion_id) ?? lobby.slots.find((s) => s.champion_id),
  );
  // A new slot: show its own position's build.
  $effect(() => {
    void shownSlot?.index;
    void shownSlot?.champion_id;
    roleOverride = null;
  });
  // Before locking in (draft queues): counters vs the lane opponent, or the
  // tier list for my role while the opponent is unknown, under the build.
  const beforePick = $derived(!!myChamp && !cs.my_champion_locked && !lobbyPick);

  // Auto-import runs once at lock-in. If the lane opponent shows up later,
  // offer (never perform) a manual import of the matchup build.
  const lateOpponent = $derived.by(() => {
    const imp = app.imported;
    const opp = cs.lane_opponent_id;
    if (!imp || !myChamp || !cs.my_champion_locked || !opp) return null;
    if (imp.championId !== myChamp || imp.opponentId === opp || hintDismissed === opp) return null;
    return opp;
  });

  // The last import was for another champion (a trade after lock-in):
  // offer — never perform — an import for the champion I have now.
  const otherImport = $derived.by(() => {
    const imp = app.imported;
    if (!imp || !myChamp || !cs.my_champion_locked || imp.championId === myChamp) return null;
    if (otherDismissed === myChamp) return null;
    return imp.championId;
  });

  function clickEnemy(id: number) {
    if (!id) return;
    override = override === id || (override === null && id === cs.lane_opponent_id) ? null : id;
  }

  async function importShown(build: Build) {
    importing = true;
    await app.importBuild(build);
    importing = false;
  }

  async function importLate(opp: number | null) {
    if (!myChamp) return;
    hintBusy = true;
    try {
      const build = await api.getBuild(myChamp, myRole, opp, queue);
      await app.importBuild(build);
    } catch (e) {
      app.toast("error", "Couldn't load the matchup build", errorMessage(e));
    } finally {
      hintBusy = false;
    }
  }
</script>

{#snippet quickLookup()}
  <div class="quick-wrap">
    <button class="btn primary" onclick={() => (app.view = "lookup")}>Open Lookup</button>
    {#if app.pool.size}
      <div class="quick">
        <span class="faint">or jump to</span>
        {#each [...app.pool].slice(0, 8) as id (id)}
          <button class="quick-btn" onclick={() => app.openLookup(id)} use:tip={`Look up ${app.champName(id)}`}>
            <ChampIcon {id} size={30} tooltip={false} />
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#if !app.lcu.connected}
  <div class="center">
    <Notice
      big
      title="League client isn't running"
      detail="Start League of Legends and this page fills in by itself when you enter champ select. Meanwhile you can look up any champion's build, counters and matchups."
    >
      {@render quickLookup()}
    </Notice>
  </div>
{:else if unsupported}
  <div class="center">
    <Notice
      big
      title="This mode isn't supported"
      detail="Champ Select Helper covers Ranked and Normals, including Swiftplay and Quickplay, and Practice Tool. ARAM and ARAM Mayhem are not shown or imported."
    />
  </div>
{:else if inGame && app.lastPick}
  {@const p = app.lastPick}
  <div class="live">
    <div class="cs-bar">
      <span class="eyebrow">In game · {queueLabel(p.queue)}</span>
      <span class="faint small">Build from champ select</span>
    </div>
    <BuildPanel
      championId={p.championId}
      role={p.role}
      opponentId={p.opponentId}
      queue={p.queue}
      showRoleTabs={false}
      status="In game"
    />
  </div>
{:else if inLobby}
  <div class="live">
    <div class="cs-bar">
      <span class="eyebrow">{queueLabel(lobby.queue, lobby.queue_id)} · {phaseLabel(app.lcu.phase)}</span>
      <span class="faint small">No champ select in this mode — your champions are picked here, per position</span>
    </div>
    <div class="slots" role="tablist" aria-label="Your positions">
      {#each lobby.slots as s (s.index)}
        <button
          role="tab"
          class="slot"
          class:active={shownSlot?.index === s.index}
          aria-selected={shownSlot?.index === s.index}
          disabled={!s.champion_id}
          onclick={() => (slotPick = s.index)}
        >
          <span class="portrait small-portrait">
            {#if s.champion_id}
              <ChampIcon id={s.champion_id} size={34} tooltip={false} />
            {:else}
              <RoleIcon role={s.role} size={18} />
            {/if}
          </span>
          <span class="slot-text">
            <span class="tname">{s.champion_id ? app.champName(s.champion_id) : "No champion yet"}</span>
            <span class="trole">
              {#if s.role}<RoleIcon role={s.role} size={11} />{ROLE_LABEL[s.role]}{:else}Fill{/if}
              · {s.index === 0 ? "1st" : "2nd"} position
            </span>
          </span>
        </button>
      {/each}
    </div>
    {#if shownSlot?.champion_id}
      <BuildPanel
        championId={shownSlot.champion_id}
        role={roleOverride ?? shownSlot.role}
        opponentId={null}
        queue={lobby.queue ?? "swiftplay"}
        onRole={(r) => (roleOverride = r)}
        status="Picked in lobby"
      >
        {#snippet actions(build)}
          <button
            class="btn primary"
            disabled={!build || importing}
            onclick={() => build && importShown(build)}
            use:tip={{
              title: "Import rune page & item set",
              body: "Creates a \"CSH:\" rune page and an item set. Then choose that rune page for this position in the client's lobby. The app never changes your lobby picks or summoner spells.",
            }}
          >
            {#if importing}<Spinner size={12} />{/if}Import
          </button>
        {/snippet}
      </BuildPanel>
      <p class="faint small lobby-note">
        Auto-import doesn't run in the lobby. Press <b>Import</b>, then pick the <b>CSH:</b> rune page for this
        position in the client.
      </p>
    {:else}
      <Notice
        title="Pick a champion for each position"
        detail="Choose your champions in the client's lobby: their builds show up here."
      />
    {/if}
  </div>
{:else if !cs.in_champ_select && isLobbyPhase(app.lcu.phase) && lobby.in_lobby && lobby.queue === "practice_tool"}
  <div class="center">
    <Notice
      big
      title="Practice Tool"
      detail="Summoner's Rift training uses ranked solo stats. The build and Import button show up when champ select starts. Import writes a rune page and an item set, and never changes summoner spells."
    >
      {@render quickLookup()}
    </Notice>
  </div>
{:else if !cs.in_champ_select}
  <div class="center">
    <Notice
      big
      title="Waiting for champ select"
      detail={`Connected${app.lcu.summoner_name ? ` as ${app.lcu.summoner_name}` : ""} · ${phaseLabel(app.lcu.phase)}. Your lane opponent, counter picks and build show up here as soon as champ select starts.`}
    >
      {@render quickLookup()}
    </Notice>
  </div>
{:else}
  <div class="live">
    <div class="cs-bar">
      <span class="eyebrow">{cs.queue ? queueLabel(cs.queue, cs.queue_id) : "Champ select"}</span>
      {#if cs.my_role}
        <span class="you"><RoleIcon role={cs.my_role} size={14} /> You're {ROLE_LABEL[cs.my_role]}</span>
      {/if}
      {#if override !== null}
        <span class="override">
          Matchup set manually
          <button class="link" onclick={() => (override = null)}>Reset to auto</button>
        </span>
      {/if}
      <span class="spacer"></span>
      {#if bans.length}
        <span class="bans" aria-label="Bans">
          <span class="faint">Bans</span>
          {#each bans as b, i (i)}
            <span class="ban"><ChampIcon id={b} size={20} /></span>
          {/each}
        </span>
      {/if}
    </div>

    <div class="teams" class:solo={cs.enemies.length === 0}>
      <div class="team allies" aria-label="Your team">
        {#each cs.allies as a, i (i)}
          <div class="tile" class:me={a.is_me} class:empty={!a.champion_id} class:hover={a.is_me && !cs.my_champion_locked}>
            <span class="portrait">
              {#if a.champion_id}
                <ChampIcon id={a.champion_id} size={44} />
              {:else}
                <RoleIcon role={a.role} size={20} />
              {/if}
            </span>
            <span class="tname">{a.champion_id ? app.champName(a.champion_id) : "Picking…"}</span>
            <span class="trole">
              {#if a.is_me}<b>You</b>{:else if a.role}{ROLE_LABEL[a.role]}{/if}
            </span>
          </div>
        {/each}
      </div>

      {#if cs.enemies.length}
      <div class="vs" aria-hidden="true"><span class="diamond"><span>VS</span></span></div>

      <div class="team enemies" aria-label="Enemy team">
        {#each cs.enemies as e, i (i)}
          {@const isOpp = !!e.champion_id && e.champion_id === opponentId}
          <button
            class="tile"
            class:opp={isOpp}
            class:manual={isOpp && override !== null}
            class:empty={!e.champion_id}
            disabled={!e.champion_id}
            onclick={() => clickEnemy(e.champion_id)}
            use:tip={e.champion_id
              ? isOpp
                ? override !== null
                  ? "Matchup opponent (manual) — click to reset to auto"
                  : "Lane opponent"
                : `Use ${app.champName(e.champion_id)} as matchup opponent`
              : null}
          >
            <span class="portrait">
              {#if e.champion_id}
                <ChampIcon id={e.champion_id} size={44} tooltip={false} />
              {:else}
                <span class="q">?</span>
              {/if}
            </span>
            <span class="tname">{e.champion_id ? app.champName(e.champion_id) : "Picking…"}</span>
            <span class="trole" class:inferred={e.role_inferred}>
              {#if e.role}
                <RoleIcon role={e.role} size={11} />{ROLE_LABEL[e.role]}{#if e.role_inferred}<span class="guess">?</span>{/if}
              {/if}
            </span>
          </button>
        {/each}
      </div>
      {/if}
    </div>

    {#if myChamp}
      {#if lateOpponent}
        <div class="hint" role="status">
          <ChampIcon id={lateOpponent} size={24} />
          <span><b>{app.champName(lateOpponent)}</b> locked in — Import matchup build?</span>
          <span class="spacer"></span>
          <button class="btn small primary" disabled={hintBusy} onclick={() => importLate(lateOpponent)}>
            {#if hintBusy}<Spinner size={11} />{/if}Import
          </button>
          <button class="icon-btn" aria-label="Dismiss" onclick={() => (hintDismissed = lateOpponent)}>
            <svg viewBox="0 0 24 24" width="11" height="11"><path d="M5 5l14 14M19 5 5 19" stroke="currentColor" stroke-width="2.6" /></svg>
          </button>
        </div>
      {/if}
      {#if otherImport && !lateOpponent}
        <div class="hint" role="status">
          <ChampIcon id={myChamp} size={24} />
          <span>Your imported runes are for <b>{app.champName(otherImport)}</b> — Import <b>{app.champName(myChamp)}</b>?</span>
          <span class="spacer"></span>
          <button class="btn small primary" disabled={hintBusy} onclick={() => importLate(opponentId)}>
            {#if hintBusy}<Spinner size={11} />{/if}Import
          </button>
          <button class="icon-btn" aria-label="Dismiss" onclick={() => (otherDismissed = myChamp)}>
            <svg viewBox="0 0 24 24" width="11" height="11"><path d="M5 5l14 14M19 5 5 19" stroke="currentColor" stroke-width="2.6" /></svg>
          </button>
        </div>
      {/if}
      <BuildPanel
        championId={myChamp}
        role={myRole}
        {opponentId}
        {queue}
        onRole={(r) => (roleOverride = r)}
        status={cs.my_champion_locked ? "Locked in" : lobbyPick ? "Picked in lobby" : "Hovering"}
      >
        {#snippet actions(build)}
          {#if !lobbyPick}
            <span
              use:tip={{
                title: "Auto-import",
                body: "Imports runes & item set once when you lock in. After that, nothing changes unless you press Import.",
              }}
            >
              <Toggle
                label="Auto-import"
                checked={app.settings?.auto_import ?? false}
                disabled={!app.settings}
                onchange={(v) => app.updateSettings({ auto_import: v })}
              />
            </span>
          {/if}
          <button
            class="btn primary"
            disabled={!build || importing}
            onclick={() => build && importShown(build)}
            use:tip={{ title: "Import runes & item set", body: "Summoner spells are only a recommendation and are never changed." }}
          >
            {#if importing}<Spinner size={12} />{/if}Import
          </button>
        {/snippet}
      </BuildPanel>
      {#if beforePick && opponentId && counterRole}
        <CounterGrid enemyId={opponentId} role={counterRole} {queue} limit={10}>
          {#snippet title()}
            <span class="eyebrow">Not locked yet · counter picks vs</span>
            <ChampIcon id={opponentId} size={22} />
            <span class="sec-name">{app.champName(opponentId)}</span>
            <span class="faint">· {ROLE_LABEL[counterRole]}</span>
          {/snippet}
        </CounterGrid>
      {:else if beforePick && myRole}
        <TierList role={myRole} {queue} limit={10}>
          {#snippet title()}
            <span class="eyebrow">Not locked yet · tier list — {ROLE_LABEL[myRole]}</span>
            <span class="faint small">Enemy {ROLE_LABEL[myRole].toLowerCase()} hasn't picked yet</span>
          {/snippet}
        </TierList>
      {/if}
    {:else if opponentId && counterRole}
      <CounterGrid enemyId={opponentId} role={counterRole} {queue}>
        {#snippet title()}
          <span class="eyebrow">Counter picks vs</span>
          <ChampIcon id={opponentId} size={22} />
          <span class="sec-name">{app.champName(opponentId)}</span>
          <span class="faint">· {ROLE_LABEL[counterRole]}</span>
        {/snippet}
      </CounterGrid>
    {:else if myRole}
      <TierList role={myRole} {queue} limit={20}>
        {#snippet title()}
          <span class="eyebrow">Tier list — {ROLE_LABEL[myRole]}</span>
          <span class="faint small">Enemy {ROLE_LABEL[myRole].toLowerCase()} hasn't picked yet · click an enemy to pick a matchup</span>
        {/snippet}
      </TierList>
    {:else}
      <Notice title="Which role are you playing?" detail="No role was assigned. Pick one to see the tier list, or click an enemy for counters.">
        <Segmented
          label="Your role"
          value={myRole}
          onchange={(r) => (roleOverride = r)}
          options={ROLES.map((r) => ({ value: r, label: ROLE_LABEL[r] }))}
        />
      </Notice>
    {/if}
  </div>
{/if}

<style>
  .center {
    display: grid;
    place-items: center;
    min-height: calc(100vh - 110px);
  }
  .quick-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }
  .quick {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
  }
  .quick .faint {
    margin-right: 4px;
  }
  .quick-btn {
    padding: 0;
    line-height: 0;
    border: 1px solid var(--line-gold);
    background: none;
  }
  .quick-btn:hover {
    border-color: var(--gold-hi);
  }
  .live {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .cs-bar {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 24px;
  }
  .you {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text);
    font-weight: 600;
    font-size: 12px;
  }
  .override {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--gold-hi);
  }
  .link {
    border: 0;
    padding: 0;
    background: none;
    color: var(--teal);
    font-size: 12px;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .spacer {
    flex: 1;
  }
  .bans {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
  }
  .bans .faint {
    margin-right: 4px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 10px;
    font-weight: 700;
  }
  .ban {
    position: relative;
    display: inline-flex;
    filter: grayscale(1);
    opacity: 0.55;
  }
  .ban::after {
    content: "";
    position: absolute;
    left: -2px;
    right: -2px;
    top: 50%;
    height: 1.5px;
    background: var(--red);
    transform: rotate(-45deg);
  }

  .teams {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 34px minmax(0, 1fr);
    align-items: stretch;
  }
  .team {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 4px;
    padding: 8px 6px;
    background: linear-gradient(180deg, var(--panel), var(--panel-2));
    border: 1px solid var(--line);
  }
  .enemies {
    border-color: #2a1b20;
    background: linear-gradient(180deg, #120f1d, #0b0d1a);
  }
  .teams.solo {
    grid-template-columns: minmax(0, 1fr);
    max-width: 560px;
  }
  .vs {
    display: grid;
    place-items: center;
  }
  .slots {
    display: flex;
    gap: 8px;
  }
  .slot {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 200px;
    padding: 6px 12px 6px 6px;
    border: 1px solid var(--line);
    background: linear-gradient(180deg, var(--panel), var(--panel-2));
    color: inherit;
    text-align: left;
  }
  .slot.active {
    border-color: var(--gold);
    box-shadow: 0 0 10px rgba(200, 155, 60, 0.3);
  }
  .slot:disabled {
    cursor: default;
    opacity: 0.7;
  }
  .small-portrait {
    width: 36px;
    height: 36px;
  }
  .slot-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .lobby-note {
    margin: 0;
  }
  .lobby-note b {
    color: var(--text);
  }
  .diamond {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    transform: rotate(45deg);
    border: 1px solid var(--gold-dim);
    background: #010a13;
  }
  .diamond span {
    transform: rotate(-45deg);
    font: 700 9.5px/1 var(--font-head);
    letter-spacing: 0.04em;
    color: var(--gold-hi);
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    min-width: 0;
    padding: 4px 2px 3px;
    border: 1px solid transparent;
    background: transparent;
    color: inherit;
    text-align: center;
  }
  button.tile:not(:disabled):hover {
    background: rgba(255, 255, 255, 0.03);
    border-color: var(--line-2);
  }
  button.tile:disabled {
    cursor: default;
  }
  .portrait {
    display: grid;
    place-items: center;
    width: 46px;
    height: 46px;
    border: 1px solid #2a3038;
    background: #010a13;
    color: var(--faint);
  }
  .tile.me .portrait {
    border: 1px solid var(--gold-hi);
    box-shadow: 0 0 10px rgba(200, 170, 110, 0.35);
  }
  .tile.me.hover .portrait {
    border-style: dashed;
  }
  .tile.me.hover .portrait :global(.champ) {
    opacity: 0.7;
  }
  .tile.opp .portrait {
    border: 1px solid var(--gold);
    box-shadow: 0 0 12px rgba(200, 155, 60, 0.45);
  }
  .tile.opp {
    background: linear-gradient(180deg, rgba(200, 155, 60, 0.12), rgba(200, 155, 60, 0.02));
    border-color: var(--line-gold);
  }
  .tile.opp.manual .portrait {
    border-color: var(--teal);
    box-shadow: 0 0 12px rgba(10, 200, 185, 0.45);
  }
  .tile.opp.manual {
    border-color: #1d4b4c;
    background: linear-gradient(180deg, rgba(10, 200, 185, 0.1), transparent);
  }
  .tile.empty .portrait {
    border-style: dashed;
    border-color: #262b31;
  }
  .q {
    font-size: 16px;
    color: #3a3f45;
  }
  .tname {
    max-width: 100%;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tile.empty .tname {
    color: var(--faint);
    font-weight: 400;
    font-style: italic;
  }
  .trole {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 14px;
    font-size: 10.5px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .trole b {
    color: var(--gold-hi);
    font-weight: 700;
  }
  .trole.inferred {
    color: var(--faint);
  }
  .tile.opp .trole {
    color: var(--gold-hi);
  }
  .guess {
    opacity: 0.8;
  }

  .hint {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 6px 10px;
    background: linear-gradient(90deg, rgba(10, 200, 185, 0.1), rgba(10, 200, 185, 0.02));
    border: 1px solid #1d4b4c;
    font-size: 12.5px;
  }
  .hint b {
    color: var(--text);
  }
  .sec-name {
    font-weight: 700;
    font-size: 14px;
  }
  .small {
    font-size: 11.5px;
  }
</style>
