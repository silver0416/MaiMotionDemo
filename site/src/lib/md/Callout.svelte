<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '$app/components/Icon.svelte';

  /**
   * 內文裡的提示框：<Callout tone="info|warn" title="選填">內文</Callout>。
   * 用 App 的 .alert（info 是中性底、warn 混入強調色），前面加圖示，不只靠顏色區分。一頁最多兩三個。
   */
  interface Props {
    tone?: 'info' | 'warn';
    title?: string;
    children?: Snippet;
  }

  let { tone = 'info', title, children }: Props = $props();
</script>

<aside class={['alert', 'callout', tone === 'warn' && 'alert--warn']} aria-label={title ?? (tone === 'warn' ? '注意' : '提示')}>
  <span class="callout-icon">
    <Icon name={tone === 'warn' ? 'triangle-alert' : 'info'} size={18} />
  </span>
  <div class="callout-body">
    {#if title}<p class="alert-title">{title}</p>{/if}
    {@render children?.()}
  </div>
</aside>

<style>
  .callout {
    display: flex;
    gap: var(--space-3);
  }

  .callout-icon {
    display: inline-flex;
    flex: none;
    padding-top: 0.3em;
    color: var(--c-text-dim);
  }

  .alert--warn .callout-icon {
    color: var(--c-accent);
  }

  .callout-body {
    min-width: 0;
  }

  .callout-body > :global(* + *) {
    margin-top: 0.5em;
  }

  .callout-body :global(p) {
    margin: 0;
  }
</style>
