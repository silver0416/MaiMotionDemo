<script lang="ts">
  import '../styles/docs.css';
  import type { Snippet } from 'svelte';
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { SITE } from '$lib/nav';
  import MenuSheet from '$lib/shell/MenuSheet.svelte';
  import NavTree from '$lib/shell/NavTree.svelte';
  import Topbar from '$lib/shell/Topbar.svelte';
  import { theme } from '$lib/shell/theme.svelte';
  import { ui } from '$lib/shell/ui.svelte';
  import SearchDialog from '$lib/search/SearchDialog.svelte';

  /**
   * 文件站外框（斷點照 BOTPu admin）：
   * - < 768px：頂列（站名＋搜尋＋選單，往下捲時收起）＋內容；選單抽屜裡是外觀、下載與章節目錄。
   * - ≥ 768px：sticky 頂列＋左側章節目錄（sticky）＋內容。
   * - ≥ 1280px：內容頁再加右側「本頁內容」（由內容頁自己排）。
   */
  let { children }: { children: Snippet } = $props();

  let sidebar = $state<HTMLElement | null>(null);

  // 側欄目錄很長：換頁後讓目前這一頁留在側欄的可視範圍（只捲側欄，不動整頁）。
  afterNavigate(() => {
    requestAnimationFrame(() => {
      const current = sidebar?.querySelector<HTMLElement>('[aria-current="page"]');
      if (!sidebar || !current) return;
      const box = sidebar.getBoundingClientRect();
      const item = current.getBoundingClientRect();
      if (item.top < box.top + 48 || item.bottom > box.bottom - 48) {
        sidebar.scrollTop += item.top - box.top - box.height / 3;
      }
    });
  });

  let copyStatus = $state('');
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  function isEditable(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    return target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.isComposing || event.defaultPrevented) return;
    const key = event.key.toLowerCase();
    if (key === 'k' && (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey) {
      event.preventDefault();
      if (!ui.searchOpen) ui.openSearch();
      return;
    }
    if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && !isEditable(event.target)) {
      if (ui.searchOpen) return;
      event.preventDefault();
      ui.openSearch();
    }
  }

  /** 程式碼區塊的複製鈕（建置管線產生的靜態 HTML，統一在這裡接點擊）。 */
  async function onCopyClick(event: MouseEvent) {
    const button = (event.target as Element | null)?.closest<HTMLButtonElement>('.code-copy');
    if (!button) return;
    const code = button.closest('.code-block')?.querySelector('code');
    if (!code) return;
    const text = code.textContent ?? '';
    try {
      await navigator.clipboard.writeText(text);
      button.classList.add('is-copied');
      button.setAttribute('aria-label', '已複製');
      button.title = '已複製';
      copyStatus = '已複製到剪貼簿';
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        button.classList.remove('is-copied');
        button.setAttribute('aria-label', '複製');
        button.title = '複製';
        copyStatus = '';
      }, 1600);
    } catch {
      // 剪貼簿不能用（例如非安全連線）：選取內容，讓使用者自己按 Ctrl+C。
      const range = document.createRange();
      range.selectNodeContents(code);
      const selection = getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      copyStatus = '沒辦法直接複製，已選取內容，請按 Ctrl+C';
    }
  }

  onMount(() => {
    theme.start();
    document.addEventListener('click', onCopyClick);
    return () => {
      document.removeEventListener('click', onCopyClick);
      clearTimeout(copyTimer);
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<a class="skip-link" href="#main">跳到主要內容</a>

<Topbar />

<div class="shell">
  <aside class="sidebar scroll" aria-label="章節目錄" bind:this={sidebar}>
    <NavTree />
  </aside>
  <div class="shell-main">
    <main id="main" tabindex="-1">
      {@render children()}
    </main>
    <footer class="site-foot">
      <p>依規則推算的打法建議，不代表官方判定或唯一正確打法。</p>
      <p>本手冊對應 App 版本 <span class="mono">v{SITE.appVersion}</span></p>
    </footer>
  </div>
</div>

<MenuSheet />

<SearchDialog bind:open={ui.searchOpen} seed={ui.searchSeed} />

<p class="sr-only" role="status" aria-live="polite">{copyStatus}</p>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-w) minmax(0, 1fr);
    min-height: calc(100dvh - var(--topbar-h));
  }

  .sidebar {
    position: sticky;
    top: var(--topbar-h);
    height: calc(100dvh - var(--topbar-h));
    padding: var(--space-5) var(--space-3) var(--space-6);
    border-right: 1px solid var(--c-border);
    background: var(--c-bg);
    overscroll-behavior: contain;
  }

  .shell-main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  main {
    flex: 1 1 auto;
    min-width: 0;
    padding: var(--space-6) var(--gutter) var(--space-6);
  }

  main:focus-visible {
    outline: none;
  }

  .site-foot {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--space-1) var(--space-4);
    padding: var(--space-4) var(--gutter) var(--space-5);
    border-top: 1px solid var(--c-border);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  @media (width < 768px) {
    /* 內容很短的頁面（404）頁尾也貼在畫面底部，下面不留一大片空白。 */
    .shell {
      display: flex;
      min-height: calc(100dvh - var(--topbar-h) - env(safe-area-inset-top, 0px));
    }

    .shell-main {
      flex: 1 1 auto;
    }

    .sidebar {
      display: none;
    }

    main {
      padding: var(--space-4) var(--gutter) var(--space-5);
    }

    /* 頁尾在最底下：一般的留白，再加上 safe-area（瀏海機型的底部橫條）。 */
    .site-foot {
      flex-direction: column;
      padding-bottom: calc(var(--space-5) + env(safe-area-inset-bottom, 0px));
    }
  }
</style>
