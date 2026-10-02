<script lang="ts">
  let { size = 16 }: { size?: number } = $props();
</script>

<span class="spinner" style:width="{size}px" style:height="{size}px" role="status" aria-label="Loading"></span>

<style>
  .spinner {
    display: inline-block;
    flex: none;
    border: 2px solid rgba(200, 170, 110, 0.18);
    border-top-color: var(--gold-hi);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
</style>
