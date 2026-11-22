<script lang="ts">
  import { page } from '$app/state';
  import { normalizePath, sectionsWithPages } from '$lib/nav';

  /**
   * 章節目錄：每章一個標題（加主色下劃線，入門導覽藍、細節粉）＋頁面清單，目前這一頁用 App 的「選中列」樣式（底色＋1px 框）。
   * 桌面的左側欄與手機的選單抽屜共用。onNavigate：點了連結之後（手機要關抽屜）。
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
        <p class="nav-heading section-mark" data-tone={group.section.tone} id={`${uid}-${group.section.id}`}>
          {group.section.title}
        </p>
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
  /* 章與章之間空一大段（32px），章節標題到它的頁面清單只留 8px，一眼看得出分組。 */
  .nav-tree {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .nav-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* 章節名：比頁面連結高一階（15px／700、最亮的字色、標題字型），底下一條主色下劃線（.section-mark，
     顏色依章節的 tone）。用 margin 而不是 padding 內縮，線才只畫在字底下、和連結文字左緣對齊。 */
  .nav-heading {
    margin-inline: var(--space-3);
    color: var(--c-text-strong);
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 700;
    line-height: var(--lh-tight);
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

  /* 觸控沒有 hover：按下時除了微縮，也給同樣的底色，才看得出按到哪一列。 */
  .nav-link:active {
    background: var(--c-control);
    color: var(--c-text-strong);
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

  /* 觸控：連結放大到 15px，章節名跟著再高一階。 */
  @media (pointer: coarse) {
    .nav-link {
      min-height: 44px;
      font-size: var(--fs-lg);
    }

    .nav-heading {
      font-size: var(--fs-xl);
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
