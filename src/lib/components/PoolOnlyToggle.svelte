<script lang="ts">
  import { app } from "../app.svelte";
  import Toggle from "./Toggle.svelte";
</script>

<Toggle
  label="Only my pool"
  checked={app.settings?.counters_pool_only ?? false}
  disabled={!app.settings}
  onchange={(v) => app.updateSettings({ counters_pool_only: v })}
  title="Only suggest champions from your champion pool"
/>
