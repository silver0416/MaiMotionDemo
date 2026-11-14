<script lang="ts">
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { SITE } from '$lib/nav';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';
  import { ui } from '$lib/shell/ui.svelte';

  /** 404 與其他錯誤：一句話＋回首頁、搜尋。 */
  const notFound = $derived(page.status === 404);

  /** 從網址猜一個搜尋字（/play/loop-section → loop section）。 */
  function guess(): string {
    const last = page.url.pathname.split('/').filter(Boolean).pop() ?? '';
    try {
      return decodeURIComponent(last).replace(/[-_]+/g, ' ').trim();
    } catch {
      return '';
    }
  }
</script>

<svelte:head>
  <title>{notFound ? '找不到這一頁' : '出了點問題'}｜{SITE.name}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<div class="error-page">
  <p class="status mono">{page.status}</p>
  <h1>{notFound ? '找不到這一頁，可能已經搬走或改名了。' : '這一頁暫時打不開，請稍後再試。'}</h1>
  <div class="actions">
    <a class="btn" href="/"><ExtraIcon name="house" />回首頁</a>
    <button type="button" class="btn" onclick={() => ui.openSearch(guess())}>
      <Icon name="search" />搜尋手冊
    </button>
  </div>
</div>

<style>
  .error-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: var(--prose-max);
    margin-inline: auto;
    padding: var(--space-6) 0;
  }

  /* 寬螢幕：和內容頁的內文對齊（內容頁是「內文＋右側目錄」整塊置中）。 */
  @media (width >= 1280px) {
    .error-page {
      margin-left: max(0px, calc((100% - (var(--prose-max) + var(--space-6) + var(--toc-w))) / 2));
    }
  }

  .status {
    color: var(--c-text-dim);
    font-size: var(--fs-lg);
  }

  h1 {
    color: var(--c-text-strong);
    font-size: var(--fs-xl);
    line-height: var(--lh-normal);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
</style>
