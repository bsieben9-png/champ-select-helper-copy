<script lang="ts">
  import type { Role } from "../types";

  let { role, size = 16 }: { role: Role | null | undefined; size?: number } = $props();
</script>

<svg
  class="role-icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="currentColor"
  aria-hidden="true"
>
  {#if role === "top"}
    <path d="M3 3h15l-3.5 3.5H6.5v8L3 18z" />
    <path d="M9.5 9.5h5v5h-5z" />
    <path d="M21 6v15H6l3.5-3.5h8v-8z" opacity=".35" />
  {:else if role === "adc"}
    <path d="M21 21H6l3.5-3.5h8v-8L21 6z" />
    <path d="M9.5 9.5h5v5h-5z" />
    <path d="M3 18V3h15l-3.5 3.5h-8v8z" opacity=".35" />
  {:else if role === "mid"}
    <path d="M16.5 3H21v4.5L7.5 21H3v-4.5z" />
    <path d="M3 3h9l-3 3H6v3l-3 3z" opacity=".35" />
    <path d="M21 21h-9l3-3h3v-3l3-3z" opacity=".35" />
  {:else if role === "jungle"}
    <path
      d="M6 2.5c2.9 3.2 4.2 6.9 4.3 11.2L12 21.5l1.7-7.8c.1-4.3 1.4-8 4.3-11.2-3 1.6-4.9 4.3-6 7.6-1.1-3.3-3-6-6-7.6z"
    />
    <path d="M3.5 8.5c1.8 1.2 3 3 3.6 5.4l1.4 3.4-.4-4.6C7.6 10.6 6 9.2 3.5 8.5zM20.5 8.5c-1.8 1.2-3 3-3.6 5.4l-1.4 3.4.4-4.6c.5-2.1 2.1-3.5 4.6-4.2z" opacity=".55" />
  {:else if role === "support"}
    <path d="M12 3.5l2.6 2.6L12 9.6 9.4 6.1z" />
    <path
      d="M2.5 7.5h7l2.5 3.2 2.5-3.2h7l-4 4.5h-3.2l1.2 2.2L12 21l-3.5-6.8 1.2-2.2H6.5z"
    />
  {:else}
    <circle cx="12" cy="12" r="4" opacity=".5" />
  {/if}
</svg>

<style>
  .role-icon {
    flex: none;
    display: inline-block;
    vertical-align: middle;
  }
</style>
