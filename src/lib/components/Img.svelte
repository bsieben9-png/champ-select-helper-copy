<script lang="ts">
  import { tip, type TipContent } from "../tooltip";

  let {
    src,
    size = 32,
    alt = "",
    round = false,
    crop = false,
    tooltip = null,
    class: cls = "",
  }: {
    src: string | null | undefined;
    size?: number;
    alt?: string;
    round?: boolean;
    /** Zoom slightly to hide the dark frame baked into champion icons. */
    crop?: boolean;
    tooltip?: TipContent;
    class?: string;
  } = $props();

  // Remember which src failed so a new src gets a fresh attempt.
  let failedSrc = $state<string | null>(null);
  const failed = $derived(!!src && failedSrc === src);
</script>

<span
  class="img {cls}"
  class:round
  class:crop
  style:width="{size}px"
  style:height="{size}px"
  use:tip={tooltip}
>
  {#if src && !failed}
    <img {src} {alt} width={size} height={size} loading="lazy" decoding="async" onerror={() => (failedSrc = src ?? null)} />
  {/if}
</span>

<style>
  .img {
    position: relative;
    display: inline-block;
    flex: none;
    overflow: hidden;
    background: #0b1626;
    vertical-align: middle;
  }
  .round {
    border-radius: 50%;
    background: transparent;
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .crop img {
    transform: scale(1.1);
  }
</style>
