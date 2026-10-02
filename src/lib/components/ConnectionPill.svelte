<script lang="ts">
  import { app } from "../app.svelte";
  import { phaseLabel } from "../format";
  import { tip } from "../tooltip";

  const mode = $derived(!app.lcu.connected ? "off" : app.cs.in_champ_select ? "cs" : "on");
  const label = $derived.by(() => {
    if (mode === "off") return "League not running";
    if (mode === "cs") return "In champ select";
    const p = phaseLabel(app.lcu.phase);
    return p === "Home" ? "Connected" : `Connected – ${p}`;
  });
  const tipText = $derived(
    mode === "off"
      ? { title: "League client not found", body: "Start League of Legends — the app connects automatically." }
      : { title: label, meta: app.lcu.summoner_name ?? undefined },
  );
</script>

<div class="pill {mode}" use:tip={tipText} aria-live="polite">
  <span class="dot" aria-hidden="true"></span>
  <span class="label">{label}</span>
</div>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--line-2);
    background: #010a13;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--muted);
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #4a4f55;
  }
  .on {
    border-color: #1d4b4c;
    color: #b9d6d3;
  }
  .on .dot {
    background: var(--teal);
    box-shadow: 0 0 6px var(--teal);
  }
  .cs {
    border-color: var(--gold-dim);
    color: var(--gold-hi);
  }
  .cs .dot {
    background: var(--gold);
    box-shadow: 0 0 7px var(--gold);
    animation: pulse 1.6s ease-in-out infinite;
  }
</style>
