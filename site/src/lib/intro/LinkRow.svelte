<script lang="ts">
  /**
   * <LinkRows> 裡的一列：整列是一個連結。上面一行是「想知道什麼」（children，寫在 .md 裡），
   * 下面是目標頁的頁名（依 href 從 nav.ts 取，永遠和站上的頁名一致），右邊一個箭頭。
   * 用法：<LinkRow href="/start/disc">盤面上的圖形代表什麼</LinkRow>
   */
  import type { Snippet } from 'svelte';
  import { findPage } from '$lib/nav';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';

  interface Props {
    href: string;
    children?: Snippet;
  }

  let { href, children }: Props = $props();

  const target = $derived.by(() => {
    const [, section = '', slug = ''] = href.split(/[/#?]/);
    return findPage(section, slug);
  });
</script>

<li class="link-row">
  <a class="row" {href}>
    <span class="row-text">
      <span class="row-question">{@render children?.()}</span>
      <span class="sr-only">，看</span>
      <span class="row-title">{target?.title ?? href}</span>
    </span>
    <span class="row-arrow" aria-hidden="true"><ExtraIcon name="arrow-right" size={18} /></span>
  </a>
</li>

<style>
  .link-row {
    margin: 0;
  }

  /* 選擇器寫成和 docs.css 的 `.prose a:not(.heading-anchor):not(.btn)` 一樣的形狀（再多一個 scoped class），
     才蓋得過內文連結的主色與底線。 */
  .row:not(.heading-anchor):not(.btn) {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 56px;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-item);
    background: var(--c-panel);
    color: var(--c-text);
    text-decoration: none;
    transition:
      background-color 120ms ease,
      border-color 120ms ease,
      transform 90ms ease-out;
  }

  .row-text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .row-question {
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    line-height: var(--lh-tight);
  }

  .row-title {
    color: var(--c-text-strong);
    font-size: var(--fs-lg);
    font-weight: 600;
    line-height: var(--lh-tight);
  }

  .row-arrow {
    color: var(--c-text-dim);
    transition: transform 220ms var(--ease-spring);
  }

  @media (hover: hover) {
    .row:not(.heading-anchor):not(.btn):hover {
      background: var(--c-control);
      border-color: var(--c-border-strong);
    }

    .row:hover .row-arrow {
      color: var(--c-text-strong);
      transform: translateX(4px);
    }
  }

  .row:focus-visible .row-arrow {
    transform: translateX(4px);
  }

  .row:not(.heading-anchor):not(.btn):active {
    transform: scale(0.99);
  }

  @media (prefers-reduced-motion: reduce) {
    .row:not(.heading-anchor):not(.btn),
    .row-arrow {
      transition-duration: 1ms;
    }

    .row:hover .row-arrow,
    .row:focus-visible .row-arrow,
    .row:not(.heading-anchor):not(.btn):active {
      transform: none;
    }
  }
</style>
