<script lang="ts">
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { SITE } from '$lib/nav';
  import ExtraIcon from './ExtraIcon.svelte';
  import NavTree from './NavTree.svelte';
  import Sheet from './Sheet.svelte';
  import ThemeSwitch from './ThemeSwitch.svelte';
  import { ui } from './ui.svelte';

  /**
   * 手機寬度（< 768px）的底部分頁列：首頁／目錄／搜尋／更多，高 56px 加 safe-area。
   * 「目錄」「更多」開成底部抽屜，「搜尋」開全螢幕的搜尋。做法照 BOTPu admin 的 BottomNav。
   */
  let tocOpen = $state(false);
  let moreOpen = $state(false);

  const atHome = $derived(page.url.pathname === '/');
</script>

<nav class="bottom-nav" aria-label="主要導覽">
  <ul class="tabs">
    <li>
      <a class="tab" href="/" aria-current={atHome ? 'page' : undefined}>
        <ExtraIcon name="house" size={20} />
        <span>首頁</span>
      </a>
    </li>
    <li>
      <button
        type="button"
        class={['tab', tocOpen && 'is-active']}
        aria-haspopup="dialog"
        aria-expanded={tocOpen}
        onclick={() => (tocOpen = true)}
      >
        <ExtraIcon name="book-open" size={20} />
        <span>目錄</span>
      </button>
    </li>
    <li>
      <button
        type="button"
        class={['tab', ui.searchOpen && 'is-active']}
        aria-haspopup="dialog"
        aria-expanded={ui.searchOpen}
        onclick={() => ui.openSearch()}
      >
        <Icon name="search" size={20} />
        <span>搜尋</span>
      </button>
    </li>
    <li>
      <button
        type="button"
        class={['tab', moreOpen && 'is-active']}
        aria-haspopup="dialog"
        aria-expanded={moreOpen}
        onclick={() => (moreOpen = true)}
      >
        <ExtraIcon name="menu" size={20} />
        <span>更多</span>
      </button>
    </li>
  </ul>
</nav>

<Sheet bind:open={tocOpen} title="目錄">
  <div class="drawer-pad">
    <NavTree focusCurrent onNavigate={() => (tocOpen = false)} />
  </div>
</Sheet>

<Sheet bind:open={moreOpen} title="更多" size="sm">
  <div class="more">
    <div class="more-group">
      <p class="more-heading">外觀</p>
      <ThemeSwitch />
    </div>
    <ul class="more-links">
      <li>
        <a class="more-link" href={SITE.download} onclick={() => (moreOpen = false)}>
          <Icon name="download" />
          <span>下載 App</span>
        </a>
      </li>
      <li>
        <a class="more-link" href={SITE.github} target="_blank" rel="noopener">
          <ExtraIcon name="github" />
          <span>GitHub</span>
          <span class="sr-only">（在新分頁開啟）</span>
        </a>
      </li>
    </ul>
  </div>
</Sheet>

<style>
  .bottom-nav {
    display: none;
  }

  @media (width < 768px) {
    .bottom-nav {
      position: fixed;
      right: 0;
      bottom: 0;
      left: 0;
      z-index: var(--z-nav);
      display: block;
      padding: 0 env(safe-area-inset-right, 0px) env(safe-area-inset-bottom, 0px) env(safe-area-inset-left, 0px);
      border-top: 1px solid var(--c-border);
      background: var(--c-panel);
    }
  }

  .tabs {
    display: flex;
    height: var(--bottomnav-h);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tabs > li {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
  }

  .tab {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 0;
    border: none;
    background: none;
    color: var(--c-text-dim);
    font-size: var(--fs-xs);
    text-decoration: none;
    cursor: pointer;
    -webkit-tap-highlight-color: transparent;
  }

  .tab :global(.icon) {
    transition: transform 90ms ease-out;
  }

  .tab:active :global(.icon) {
    transform: scale(0.9);
  }

  .tab[aria-current='page'],
  .tab.is-active {
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .tab[aria-current='page'] :global(.icon),
  .tab.is-active :global(.icon) {
    color: var(--c-primary);
  }

  .tab:focus-visible {
    outline-offset: -4px;
  }

  .drawer-pad {
    padding: var(--space-1) 0 var(--space-3);
  }

  .more {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-1) 0 var(--space-3);
  }

  .more-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: 0 var(--space-3);
  }

  .more-heading {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .more-links {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: var(--space-3) 0 0;
    border-top: 1px solid var(--c-border);
    list-style: none;
  }

  .more-link {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 48px;
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    color: var(--c-text);
    font-size: var(--fs-lg);
    text-decoration: none;
  }

  .more-link :global(.icon) {
    color: var(--c-text-dim);
  }

  @media (hover: hover) {
    .more-link:hover {
      background: var(--c-control);
    }
  }

  .more-link:focus-visible {
    outline-offset: -2px;
  }

  @media (prefers-reduced-motion: reduce) {
    .tab :global(.icon) {
      transition-duration: 1ms;
    }
  }
</style>
