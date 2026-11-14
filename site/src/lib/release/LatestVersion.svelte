<script lang="ts">
  import { onMount } from 'svelte';
  import { formatDate } from './core';
  import { latestRelease } from './release.svelte';

  /** 首頁主要動作旁的一行：最新版本 v1.2.1 · 2026-11-13 發布。先用建置時的快照，掛載後向 /api/release 更新。 */
  onMount(() => latestRelease.refresh());

  const info = $derived(latestRelease.info);
  const date = $derived(formatDate(info?.publishedAt ?? null));
</script>

{#if info}
  <p class="latest">
    <span class="label">最新版本</span>
    <span class="mono version">v{info.version}</span>
    {#if date}<span><time class="mono" datetime={info.publishedAt}>{date}</time> 發布</span>{/if}
  </p>
{/if}

<style>
  .latest {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-2);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  .version {
    color: var(--c-text-strong);
    font-weight: 600;
  }
</style>
