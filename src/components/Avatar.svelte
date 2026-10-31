<script lang="ts">
  import { initials } from "../lib/format";
  let { name, color = "#64748b", url = null, size = 28 }: { name: string; color?: string | null; url?: string | null; size?: number } = $props();
</script>

{#if url}
  <img class="avatar" src={url} alt="" width={size} height={size} style="width:{size}px;height:{size}px" />
{:else}
  <span class="avatar initials" style="width:{size}px;height:{size}px;background:{color ?? '#64748b'};font-size:{Math.round(size * 0.4)}px">{initials(name)}</span>
{/if}

<style>
  .avatar {
    border-radius: 50%;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    object-fit: cover;
  }
  .initials {
    color: #fff;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
</style>
