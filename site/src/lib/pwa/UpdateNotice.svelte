<script lang="ts">
  /**
   * App 模式（standalone）下有新版本時的提示：一句話＋「重新整理」＋關閉。不擋畫面，點了才換新並重新載入。
   * 一般瀏覽器不顯示（register.ts 直接讓新版接手，下次整頁載入就是新版）。
   */
  import Icon from '$app/components/Icon.svelte';
  import { platform } from '$lib/app/platform.svelte';
  import Notice from './Notice.svelte';
  import { applyUpdate } from './register';
  import { pwa } from './state.svelte';

  interface Props {
    /** 搜尋或選單開著時先藏起來。 */
    hidden?: boolean;
  }

  let { hidden = false }: Props = $props();
</script>

{#if pwa.updateReady && platform.standalone && !hidden}
  <Notice label="有新版本">
    <div class="update" role="status">
      <span class="update-icon"><Icon name="refresh-cw" size={16} /></span>
      <p class="update-text">有新版本</p>
      <button class="btn btn--primary" onclick={applyUpdate}>重新整理</button>
      <button class="btn btn--round update-close" aria-label="關閉" onclick={() => (pwa.updateReady = false)}>
        <Icon name="x" />
      </button>
    </div>
  </Notice>
{/if}

<style>
  .update {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .update-icon {
    color: var(--c-primary);
  }

  .update-text {
    flex: 1 1 auto;
    min-width: 0;
    margin: 0;
    font-size: var(--fs-md);
    font-weight: 600;
  }

  .update-close {
    background: var(--c-control);
    color: var(--c-text);
  }
</style>
