<script lang="ts">
  import { onMount } from 'svelte';
  import { afterNavigate, goto } from '$app/navigation';
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { normalizePath, SITE } from '$lib/nav';
  import { AutoHide } from '$lib/shell/autohide.svelte';
  import Logo from '$lib/shell/Logo.svelte';
  import { theme } from '$lib/shell/theme.svelte';
  import { ui } from '$lib/shell/ui.svelte';
  import { platform } from './platform.svelte';

  /**
   * App 外框（從主畫面開啟，<html data-app>）的頂列：只有標誌與站名，不是連結、沒有搜尋與選單（都在底部導覽列）。
   * 收起與出現照一般外框的手機頂列（autohide.svelte.ts），在頁面頂端和背景同色。
   * - iOS：標誌與站名靠左（使用者 2026-11-15 指定，不照 iOS 慣例置中）；不在首頁時最左邊多一顆返回圓鈕
   *   （只有往左的 chevron，不寫字），站名接在它右邊。
   *   有站內的上一頁就 history.back()，沒有（直接從主畫面開到內頁）就回首頁。
   * - Android：標誌與站名靠左（Material 的 top app bar，高 64px）；不放返回鈕（交給系統返回手勢）。
   *   離開頂端後換成面板色（和底部導覽列同色），系統狀態列跟著換。
   * 一般外框時整條不顯示（CSS 看 data-app，第一次繪製就決定，不閃）。
   */
  const bar = new AutoHide(() => platform.app !== null);

  const atHome = $derived(normalizePath(page.url.pathname) === '/');

  /**
   * 有沒有站內的上一頁：
   * - 有 Navigation API（Chrome、Safari 26 起）時看 navigation.canGoBack：它只算同一個網站的瀏覽紀錄。
   * - 沒有的話自己數：每次站內換頁（推進一筆紀錄）加一，記在那一筆紀錄的 page.state（persistState，重新整理後也還在），
   *   用返回鍵回到那一筆時讀回來。讀不到時當成沒有上一頁（返回鈕改成回首頁，不會離開網站）。
   * 從主畫面直接開到內頁時沒有上一頁。
   */
  const DEPTH_KEY = 'appDepth';
  let depth = 0;

  function savedDepth(): number | null {
    const value = (page.state as Record<string, unknown>)[DEPTH_KEY];
    return typeof value === 'number' ? value : null;
  }

  afterNavigate((navigation) => {
    bar.afterNavigate();
    if (navigation.shallow) return;
    const saved = savedDepth();
    if (saved !== null || navigation.type === 'enter' || navigation.type === 'popstate') {
      depth = saved ?? 0;
      return;
    }
    depth += 1;
    void goto(location.href, {
      state: { ...page.state, [DEPTH_KEY]: depth },
      shallow: true,
      replace: true,
      persistState: true,
    }).catch(() => {
      // 記不下來時最壞的情況是返回鈕改成回首頁。
    });
  });

  function canGoBack(): boolean {
    const nav = (window as Window & { navigation?: { canGoBack?: boolean } }).navigation;
    if (typeof nav?.canGoBack === 'boolean') return nav.canGoBack;
    return depth > 0;
  }

  function back() {
    if (canGoBack()) history.back();
    else void goto('/');
  }

  onMount(() => bar.start());

  // 對話框或抽屜開著時一定顯示。
  $effect(() => {
    if (ui.anyOpen) bar.show();
  });

  // Android：頂列離開頂端換成面板色時，系統狀態列（theme-color）一起換；離開 App 外框時還原。
  $effect(() => {
    theme.setChrome(platform.app === 'android' && bar.scrolled ? 'panel' : 'bg');
  });
</script>

<header
  bind:this={bar.header}
  class="app-topbar"
  class:is-scrolled={bar.scrolled}
  class:is-hidden={bar.hidden}
  onfocusin={bar.onFocusIn}
>
  <div class="app-topbar-inner">
    {#if !atHome}
      <button type="button" class="app-back" aria-label="返回" onclick={back}>
        <Icon name="chevron-left" size={24} />
      </button>
    {/if}
    <span class="app-title">
      <Logo size={24} />
      <span class="wordmark app-title-name">{SITE.shortName}</span>
    </span>
  </div>
</header>

<style>
  /* 一般外框：不顯示。 */
  .app-topbar {
    display: none;
  }

  :global(:root[data-app]) .app-topbar {
    position: sticky;
    top: 0;
    z-index: var(--z-sticky);
    display: block;
    padding-top: env(safe-area-inset-top, 0px);
    /* 在頁面頂端：和背景同色、沒有分隔線（狀態列的 theme-color 也是這個色，接得起來）。 */
    border-bottom: 1px solid transparent;
    background: var(--c-bg);
    transition:
      transform 220ms var(--ease-out),
      background-color 200ms ease,
      border-color 200ms ease,
      box-shadow 200ms ease;
  }

  /* 收起：陰影在滑出去之後才拿掉，不會在畫面最上緣留下一條影子。 */
  :global(:root[data-app]) .app-topbar.is-hidden {
    transform: translateY(-100%);
    box-shadow: none;
    transition:
      transform 220ms var(--ease-out),
      box-shadow 0s linear 220ms;
  }

  /* 由左到右：返回鈕（iOS 的內頁才有）、標誌與站名，一律靠左。 */
  .app-topbar-inner {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    max-width: var(--app-col);
    height: var(--topbar-h);
    margin-inline: auto;
    padding: 0 var(--space-4);
  }

  .app-back {
    flex: none;
  }

  .app-title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    color: var(--c-text-strong);
    white-space: nowrap;
  }

  .app-title-name {
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 17px;
    line-height: 1.2;
  }

  /* ---- iOS ---- */

  /* 往下捲之後：和一般外框的頂列一樣，極輕微的毛玻璃加一點陰影（README「設計」的頂列例外，不另立新例外）。 */
  :global(:root[data-app='ios']) .app-topbar.is-scrolled {
    border-bottom-color: var(--c-border);
    background: color-mix(in srgb, var(--c-bg) 94%, transparent);
    -webkit-backdrop-filter: blur(6px);
    backdrop-filter: blur(6px);
    box-shadow: var(--topbar-shadow);
  }

  @supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {
    :global(:root[data-app='ios']) .app-topbar.is-scrolled {
      background: var(--c-bg);
    }
  }

  @media (prefers-reduced-transparency: reduce) {
    :global(:root[data-app='ios']) .app-topbar.is-scrolled {
      background: var(--c-bg);
      -webkit-backdrop-filter: none;
      backdrop-filter: none;
    }
  }

  /* 返回鈕：44×44 的圓鈕，只有 chevron（Apple 的規範：圖示按鈕不寫字）。
     和底部導覽列同一種實心樣子：不透明面板色＋1px 描邊＋浮起的陰影（「降低透明度」時的原生外觀）。 */
  .app-back {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 1px solid var(--app-chrome-edge);
    border-radius: var(--radius-pill);
    background: var(--c-panel);
    box-shadow: var(--app-chrome-shadow);
    color: var(--c-text-strong);
    cursor: pointer;
    transition:
      background-color 120ms ease,
      transform 90ms ease-out;
  }

  /* chevron 的視覺重心偏右，往左挪一點才看起來置中。 */
  .app-back :global(.icon) {
    margin-left: -2px;
  }

  @media (hover: hover) {
    .app-back:hover {
      background: var(--c-control);
    }
  }

  .app-back:active {
    background: var(--c-control);
    transform: scale(0.96);
  }

  /* Android 不放返回鈕（系統的返回手勢就是返回）。 */
  :global(:root[data-app='android']) .app-back {
    display: none;
  }

  /* ---- Android（Material 3 的 top app bar：高 64px、標題 22px 靠左） ---- */

  :global(:root[data-app='android']) .app-title {
    gap: 10px;
  }

  :global(:root[data-app='android']) .app-title-name {
    font-size: 22px;
  }

  /* 離開頂端後換成面板色（M3 用色調分層，不用陰影與毛玻璃）；和底部導覽列同色，狀態列也跟著換。 */
  :global(:root[data-app='android']) .app-topbar.is-scrolled {
    background: var(--c-panel);
  }

  @media (prefers-contrast: more) {
    .app-back {
      border-width: 2px;
      border-color: var(--c-border-strong);
    }
  }

  /* 減少動態：照樣收起與出現，只是瞬間完成。 */
  @media (prefers-reduced-motion: reduce) {
    :global(:root[data-app]) .app-topbar,
    :global(:root[data-app]) .app-topbar.is-hidden,
    .app-back {
      transition: none;
    }
  }
</style>
