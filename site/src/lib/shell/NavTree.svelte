<script lang="ts">
  import { page } from '$app/state';
  import { normalizePath, sectionsWithPages } from '$lib/nav';

  /**
   * 章節目錄：每章一個小標題＋頁面清單，目前這一頁用 App 的「選中列」樣式（底色＋1px 框）。
   * 桌面的左側欄與手機的「目錄」抽屜共用。onNavigate：點了連結之後（手機要關抽屜）。
   */
  interface Props {
    onNavigate?: () => void;
    /** 抽屜打開時把焦點放在目前這一頁（瀏覽器會順便捲到看得見的位置）。 */
    focusCurrent?: boolean;
  }

  let { onNavigate, focusCurrent = false }: Props = $props();

  const uid = $props.id();
  const groups = sectionsWithPages();
  const current = $derived(normalizePath(page.url.pathname));
</script>

{#if groups.length === 0}
  <p class="nav-empty">內容整理中，目錄稍後就會出現。</p>
{:else}
  <div class="nav-tree">
    {#each groups as group (group.section.id)}
      <div class="nav-group">
        <p class="nav-heading" id={`${uid}-${group.section.id}`}>{group.section.title}</p>
        <ul class="nav-list" aria-labelledby={`${uid}-${group.section.id}`}>
          {#each group.pages as item (item.url)}
            <li>
              <a
                class="nav-link"
                href={item.url}
                aria-current={item.url === current ? 'page' : undefined}
                data-autofocus={focusCurrent && item.url === current ? '' : undefined}
                onclick={() => onNavigate?.()}
              >
                {item.title}
              </a>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
  </div>
{/if}

<style>
  .nav-tree {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .nav-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  /* 章節名：同 App 的 .section-title（12px、600、字距 .04em、次要色）。 */
  .nav-heading {
    padding: 0 var(--space-3);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .nav-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* 清單列：圓角 16、透明框保留高度；hover 淡底，選中是 --c-control-hover 底＋--c-border-strong 框。 */
  .nav-link {
    display: flex;
    align-items: center;
    min-height: 34px;
    padding: 6px var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    color: var(--c-text);
    font-size: var(--fs-md);
    line-height: var(--lh-tight);
    text-decoration: none;
    transition:
      background-color 120ms ease,
      transform 90ms ease-out;
  }

  @media (hover: hover) {
    .nav-link:hover {
      background: var(--c-control);
      color: var(--c-text-strong);
    }
  }

  .nav-link:active {
    transform: scale(0.99);
  }

  .nav-link[aria-current='page'] {
    background: var(--c-control-hover);
    border-color: var(--c-border-strong);
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .nav-link:focus-visible {
    outline-offset: -2px;
  }

  @media (pointer: coarse) {
    .nav-link {
      min-height: 44px;
      font-size: var(--fs-lg);
    }
  }

  .nav-empty {
    padding: 0 var(--space-3);
    color: var(--c-text-dim);
    font-size: var(--fs-md);
  }

  @media (prefers-reduced-motion: reduce) {
    .nav-link {
      transition-duration: 1ms;
    }
  }
</style>
