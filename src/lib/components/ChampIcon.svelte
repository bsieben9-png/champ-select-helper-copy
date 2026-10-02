<script lang="ts">
  import { app } from "../app.svelte";
  import type { TipContent } from "../tooltip";
  import Img from "./Img.svelte";

  let {
    id,
    size = 32,
    tooltip = true,
    class: cls = "",
  }: {
    id: number | null | undefined;
    size?: number;
    /** true → champion name; or custom tooltip content; false → none. */
    tooltip?: boolean | TipContent;
    class?: string;
  } = $props();

  const champ = $derived(id ? app.champs.get(id) : undefined);
  const tipContent = $derived<TipContent>(
    tooltip === true ? (champ?.name ?? null) : tooltip === false ? null : tooltip,
  );
</script>

<Img src={champ?.icon} {size} alt={champ?.name ?? ""} crop tooltip={tipContent} class="champ {cls}" />
