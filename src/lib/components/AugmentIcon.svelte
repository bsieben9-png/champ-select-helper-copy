<script lang="ts">
  import { rarityOf } from "../format";

  let { name, icon, rarity, size = 30 }: { name: string; icon: string; rarity: string; size?: number } = $props();

  // Icons may come from a host the CSP doesn't allow (yet) or be missing:
  // fall back to a rarity-coloured letter badge.
  let failedSrc = $state<string | null>(null);
  const showImg = $derived(!!icon && failedSrc !== icon);
  const letters = $derived(
    name
      .split(/[\s:'-]+/)
      .filter((w) => /^[A-Za-z0-9]/.test(w))
      .slice(0, 2)
      .map((w) => w[0].toUpperCase())
      .join("") || "?",
  );
</script>

<span class="aug {rarityOf(rarity)}" style:width="{size}px" style:height="{size}px" aria-hidden="true">
  {#if showImg}
    <img src={icon} alt="" width={size} height={size} loading="lazy" onerror={() => (failedSrc = icon)} />
  {:else}
    <span class="letters" style:font-size="{Math.round(size * 0.4)}px">{letters}</span>
  {/if}
</span>

<style>
  .aug {
    position: relative;
    display: inline-grid;
    place-items: center;
    flex: none;
    overflow: hidden;
    border: 1.5px solid var(--rc);
    background: radial-gradient(circle at 50% 30%, color-mix(in srgb, var(--rc) 30%, #0a1428), #050b16 75%);
    box-shadow: 0 0 6px color-mix(in srgb, var(--rc) 35%, transparent);
  }
  .silver {
    --rc: #9fb0c2;
    background: linear-gradient(160deg, #4a5a6c, #1c2633 80%);
  }
  .gold {
    --rc: #e0b153;
    background: linear-gradient(160deg, #8a6420, #2e210b 80%);
  }
  .prismatic {
    --rc: #c58cf2;
    border-image: linear-gradient(135deg, #f08ad8, #8cc8f5, #f5dc8c) 1;
    background: linear-gradient(135deg, #7a3f8f, #2c4f7c 55%, #6f6230);
  }
  .other {
    --rc: var(--gold-dim);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .letters {
    font-family: var(--font-head);
    font-weight: 700;
    letter-spacing: 0.02em;
    color: #f4ecdc;
    text-shadow: 0 1px 2px #000;
  }
  .prismatic .letters {
    background: linear-gradient(135deg, #ffc4ef, #bfe6ff, #fff1c4);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    text-shadow: none;
  }
</style>
