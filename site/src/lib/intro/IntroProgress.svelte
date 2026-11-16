<script lang="ts">
  /**
   * 導覽進度：「認識 MaiMotionDemo」這一章的每一頁排成一排編號圓點，中間以線連起來，目前這頁是主色（aria-current）。
   * 頁面清單直接取 nav.ts 的 intro 章節（依 order），新增或改名頁面不用改這裡。
   * 欄寬不夠時只顯示編號（頁名留給輔助技術），寬的時候編號旁加頁名。
   * 用法：每一頁內文最上面放 <IntroProgress />。
   */
  import { page } from '$app/state';
  import { normalizePath, pagesIn } from '$lib/nav';

  const pages = pagesIn('intro');
  const current = $derived(normalizePath(page.url.pathname));
</script>

<nav class="tour" aria-label="導覽進度">
  <ol class="tour-list">
    {#each pages as item, index (item.url)}
      {@const active = item.url === current}
      <li class={['tour-item', active && 'is-current']}>
        <a class="tour-link" href={item.url} aria-current={active ? 'page' : undefined}>
          <span class="tour-num mono">{index + 1}</span>
          <span class="tour-title">{item.title}</span>
        </a>
        {#if index < pages.length - 1}<span class="tour-line" aria-hidden="true"></span>{/if}
      </li>
    {/each}
  </ol>
</nav>

<style>
  .tour {
    container-type: inline-size;
  }

  .tour-list {
    display: flex;
    align-items: center;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tour-item {
    display: flex;
    flex: 1 1 0;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
  }

  .tour-item:last-child {
    flex: none;
  }

  /* 和 LinkRow 一樣：寫成 .prose 內文連結的形狀才蓋得過它的主色與底線。 */
  .tour-link:not(.heading-anchor):not(.btn) {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--space-2);
    min-height: 32px;
    border-radius: var(--radius-pill);
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    line-height: var(--lh-tight);
    text-decoration: none;
    white-space: nowrap;
  }

  .tour-num {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-pill);
    background: var(--c-control-hover);
    color: var(--c-text);
    font-size: var(--fs-sm);
    font-weight: 600;
    transition:
      background-color 120ms ease,
      transform 90ms ease-out;
  }

  .is-current .tour-num {
    background: var(--c-primary);
    color: var(--c-on-primary);
  }

  .is-current .tour-title {
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .tour-line {
    flex: 1 1 auto;
    min-width: var(--space-2);
    height: 2px;
    border-radius: var(--radius-pill);
    background: var(--c-border);
  }

  @media (hover: hover) {
    .tour-item:not(.is-current) .tour-link:hover .tour-num {
      background: var(--c-border-strong);
    }

    .tour-link:not(.heading-anchor):not(.btn):hover .tour-title {
      color: var(--c-text-strong);
    }
  }

  .tour-link:active .tour-num {
    transform: scale(0.96);
  }

  /* 窄的時候只留編號：頁名改成只給輔助技術（同 .sr-only 的做法）。 */
  /* 五個頁名加編號約需 640px，留一點餘裕。 */
  @container (width < 700px) {
    .tour-title {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
  }

  @media (pointer: coarse) {
    .tour-link:not(.heading-anchor):not(.btn) {
      min-height: 44px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .tour-num {
      transition-duration: 1ms;
    }

    .tour-link:active .tour-num {
      transform: none;
    }
  }
</style>
