<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { normalizePath, SITE } from '$lib/nav';
  import Logo from './Logo.svelte';
  import ThemeSwitch from './ThemeSwitch.svelte';
  import { isMac, ui } from './ui.svelte';

  /**
   * 頂列：站名、搜尋入口（顯示快捷鍵）、外觀切換、下載。
   * 手機寬度只留站名，其他動作移到底部分頁列的「搜尋」「更多」。
   */
  let mac = $state(false);
  /** 頁面是否已經往下捲：在頂端時頂列和背景同色，捲動後才浮起來。 */
  let scrolled = $state(false);
  onMount(() => {
    mac = isMac();
    const update = () => (scrolled = window.scrollY > 4);
    update();
    window.addEventListener('scroll', update, { passive: true });
    return () => window.removeEventListener('scroll', update);
  });

  const atDownload = $derived(normalizePath(page.url.pathname) === SITE.download);
</script>

<header class="topbar" class:is-scrolled={scrolled}>
  <a class="brand" href="/">
    <Logo size={26} />
    <span class="wordmark brand-name">{SITE.shortName}</span>
  </a>

  <span class="spacer"></span>

  <button type="button" class="search-entry" aria-haspopup="dialog" onclick={() => ui.openSearch()}>
    <Icon name="search" />
    <span class="search-label">搜尋手冊</span>
    <span class="search-keys" aria-hidden="true">
      {#if mac}<kbd class="kbd">⌘</kbd><kbd class="kbd">K</kbd>{:else}<kbd class="kbd">Ctrl</kbd><kbd class="kbd">K</kbd>{/if}
    </span>
  </button>

  <ThemeSwitch compact />

  <a class="btn" href={SITE.download} aria-current={atDownload ? 'page' : undefined}>
    <Icon name="download" />下載
  </a>
</header>

<style>
  .topbar {
    position: sticky;
    top: 0;
    z-index: var(--z-sticky);
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--topbar-h);
    padding: 0 var(--space-4);
    /* 在頁面頂端：和背景同色、沒有分隔線，像是頁面的一部分。 */
    border-bottom: 1px solid transparent;
    background: var(--c-bg);
    transition:
      background-color 200ms ease,
      border-color 200ms ease,
      box-shadow 200ms ease;
  }

  /* 往下捲之後：極輕微的毛玻璃加一點陰影，和內容分開。
     使用者 2026-11-15 指定（仿 react.dev），是「不用半透明面板、不用 backdrop-filter」規則的唯一例外，不要搬到別處。 */
  .topbar.is-scrolled {
    border-bottom-color: var(--c-border);
    background: color-mix(in srgb, var(--c-bg) 94%, transparent);
    -webkit-backdrop-filter: blur(6px);
    backdrop-filter: blur(6px);
    box-shadow: var(--topbar-shadow);
  }

  /* 不支援 backdrop-filter，或使用者要求降低透明度時，維持不透明。 */
  @supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {
    .topbar.is-scrolled {
      background: var(--c-bg);
    }
  }

  @media (prefers-reduced-transparency: reduce) {
    .topbar.is-scrolled {
      background: var(--c-bg);
      -webkit-backdrop-filter: none;
      backdrop-filter: none;
    }
  }

  .brand {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    min-height: var(--control-h);
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    color: var(--c-text-strong);
    text-decoration: none;
    white-space: nowrap;
  }

  /* 站名整串同一個字型、字重與顏色（.wordmark 在 docs.css）。 */
  .brand-name {
    font-size: 17px;
  }

  /* 搜尋入口：長得像一個膠囊輸入框，點了開搜尋對話框。 */
  .search-entry {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    width: min(280px, 32vw);
    min-height: 34px;
    padding: 0 6px 0 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-pill);
    background: var(--c-control);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    cursor: pointer;
    transition:
      border-color 120ms ease,
      transform 90ms ease-out;
  }

  @media (hover: hover) {
    .search-entry:hover {
      border-color: var(--c-border-strong);
    }
  }

  .search-entry:active {
    transform: scale(0.98);
  }

  .search-label {
    flex: 1 1 auto;
    text-align: left;
  }

  .search-keys {
    display: inline-flex;
    gap: 2px;
  }

  .search-keys :global(.kbd) {
    font-size: var(--fs-xs);
    min-height: 22px;
  }

  @media (width < 1024px) {
    .search-entry {
      width: auto;
    }
  }

  /* 手機寬度：只留站名，捲動時跟著捲走（動作都在底部分頁列）。 */
  @media (width < 768px) {
    .topbar,
    .topbar.is-scrolled {
      position: static;
      border-bottom-color: transparent;
      background: var(--c-bg);
      -webkit-backdrop-filter: none;
      backdrop-filter: none;
      box-shadow: none;
      height: calc(var(--topbar-h) + env(safe-area-inset-top, 0px));
      padding: env(safe-area-inset-top, 0px) var(--space-3) 0;
    }

    .search-entry,
    .topbar :global(.theme-switch),
    .topbar > .btn {
      display: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .topbar,
    .search-entry {
      transition-duration: 1ms;
    }
  }
</style>
