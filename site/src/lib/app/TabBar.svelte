<script lang="ts">
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { reducedMotion } from '$app/lib/press';
  import { findPage, normalizePath, SITE } from '$lib/nav';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';
  import { ui } from '$lib/shell/ui.svelte';
  import { PANEL_IDS } from './panel-ids';
  import TabFilled from './TabFilled.svelte';

  /**
   * App 外框（<html data-app>）的底部導覽列：首頁、目錄、搜尋、更多四顆，常駐、不隨捲動收起。
   * - 首頁是連結；目錄、搜尋、更多打開對應的抽屜或對話框（它們從底部滑出時蓋過這一列）。
   * - 選中：首頁在 `/`、目錄在任何內容頁、更多在 `/download`；抽屜或搜尋開著時改成那一顆。
   * - iOS：浮在內容上的實心膠囊，選中的那一顆背後有一塊內縮 3px 的膠囊，在項目間滑動。
   * - Android：Material 3 的 navigation bar，貼底滿寬，選中的那一顆圖示後面是 56×32 的指示器（從中心展開）。
   * 一般外框時整條不顯示（CSS 看 data-app）。
   */
  type Tab = 'home' | 'toc' | 'search' | 'more';
  const ORDER: Tab[] = ['home', 'toc', 'search', 'more'];

  const path = $derived(normalizePath(page.url.pathname));
  const atHome = $derived(path === '/');
  const atDownload = $derived(path === SITE.download);
  const atContent = $derived.by(() => {
    const match = /^\/([^/]+)\/([^/]+)$/.exec(path);
    return !!match && !!findPage(match[1], match[2]);
  });

  const active = $derived.by((): Tab | null => {
    if (ui.searchOpen) return 'search';
    if (ui.moreOpen) return 'more';
    if (ui.tocOpen) return 'toc';
    if (atHome) return 'home';
    if (atContent) return 'toc';
    if (atDownload) return 'more';
    return null;
  });

  /** iOS 的選中膠囊位置；沒有選中的時候（例如 404）停在原地、只是不顯示。 */
  let lastIndex = $state(0);
  $effect(() => {
    if (active) lastIndex = ORDER.indexOf(active);
  });
  const glideIndex = $derived(active ? ORDER.indexOf(active) : lastIndex);

  /**
   * 「首頁」：面板開著就先關掉。已經在首頁時捲回最上面（原生 App 的習慣；面板開著的話等它關掉、紀錄收掉再捲）。
   * 不在首頁時照一般連結換頁。
   */
  function onHomeClick(event: MouseEvent) {
    if (ui.anyPanel) ui.closePanels(atHome);
    if (!atHome) return;
    event.preventDefault();
    if (!ui.scrollTopAfterClose) window.scrollTo({ top: 0, behavior: reducedMotion() ? 'instant' : 'smooth' });
  }

  /** 開面板的三顆：點目前開著的那一顆就關掉（回到原本的頁面），點別顆直接切換。 */
  function toggle(panel: 'toc' | 'search' | 'more') {
    if (ui.panel === panel) {
      ui.closePanels();
      return;
    }
    if (panel === 'toc') ui.openToc();
    else if (panel === 'more') ui.openMore();
    else ui.openSearch();
  }
</script>

<nav class="tabbar" aria-label="主要導覽">
  <div class="tabbar-track">
    <span class={['tabbar-glider', !active && 'is-off']} style:--i={glideIndex} aria-hidden="true"></span>

    <a
      class={['tab', active === 'home' && 'is-active']}
      href="/"
      aria-current={atHome ? 'page' : undefined}
      onclick={onHomeClick}
    >
      <span class="tab-icon has-filled"><ExtraIcon name="house" size={24} /><TabFilled name="house" /></span>
      <span class="tab-label">首頁</span>
    </a>

    <button
      type="button"
      class={['tab', active === 'toc' && 'is-active']}
      data-panel="toc"
      aria-haspopup="dialog"
      aria-expanded={ui.tocOpen}
      aria-controls={PANEL_IDS.toc}
      onclick={() => toggle('toc')}
    >
      <span class="tab-icon has-filled"><ExtraIcon name="book-open" size={24} /><TabFilled name="book-open" /></span>
      <span class="tab-label">目錄</span>
    </button>

    <button
      type="button"
      class={['tab', active === 'search' && 'is-active']}
      data-panel="search"
      aria-haspopup="dialog"
      aria-expanded={ui.searchOpen}
      aria-controls={PANEL_IDS.search}
      onclick={() => toggle('search')}
    >
      <span class="tab-icon"><Icon name="search" size={24} /></span>
      <span class="tab-label">搜尋</span>
    </button>

    <button
      type="button"
      class={['tab', active === 'more' && 'is-active']}
      data-panel="more"
      aria-haspopup="dialog"
      aria-expanded={ui.moreOpen}
      aria-controls={PANEL_IDS.more}
      onclick={() => toggle('more')}
    >
      <span class="tab-icon"><Icon name="more-horizontal" size={24} /></span>
      <span class="tab-label">更多</span>
    </button>
  </div>
</nav>

<style>
  /* 一般外框：不顯示。 */
  .tabbar {
    display: none;
  }

  :global(:root[data-app]) .tabbar {
    position: fixed;
    z-index: var(--z-tabbar);
    display: block;
  }

  .tabbar-track {
    position: relative;
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    height: 100%;
  }

  .tabbar-glider {
    display: none;
  }

  .tab {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-width: 0;
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    font: inherit;
    text-decoration: none;
    cursor: pointer;
    -webkit-touch-callout: none;
    user-select: none;
  }

  .tab-icon {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .tab-icon :global(.icon) {
    position: relative;
  }

  /* 實心版圖示（TabFilled）：平常不顯示，Android 選中時換上（見下方 Android 區）。 */
  .tab-icon :global(.tab-filled) {
    display: none;
  }

  .tab-label {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ---------------------------------------------------------------------------
     iOS：浮在內容上的實心膠囊（「降低透明度」時的原生外觀）。
     高 62、圓角 31、左右離螢幕 21；有 home indicator 的機型離底 21（疊在 home indicator 區上方，原生就是這樣）。 */

  :global(:root[data-app='ios']) .tabbar {
    right: 21px;
    bottom: max(12px, calc(env(safe-area-inset-bottom, 0px) - 13px));
    left: 21px;
    max-width: calc(var(--app-col) - 42px);
    height: 62px;
    margin-inline: auto;
    padding: 3px;
    border: 1px solid var(--app-chrome-edge);
    border-radius: 31px;
    background: var(--c-panel);
    box-shadow: var(--app-chrome-shadow);
  }

  /* 選中的那一顆背後：內縮 3px 的較亮膠囊，換選中時滑過去（350ms、輕微回彈）。 */
  :global(:root[data-app='ios']) .tabbar-glider {
    position: absolute;
    top: 0;
    left: 0;
    display: block;
    width: 25%;
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--c-control-hover);
    transform: translateX(calc(var(--i, 0) * 100%));
    transition:
      transform 350ms var(--ease-spring),
      opacity 150ms ease;
  }

  :global(:root[data-app='ios']) .tabbar-glider.is-off {
    opacity: 0;
  }

  :global(:root[data-app='ios']) .tab {
    gap: 1px;
    border-radius: var(--radius-pill);
    color: var(--c-text);
    transition:
      color 160ms ease,
      transform 90ms ease-out;
  }

  :global(:root[data-app='ios']) .tab.is-active {
    color: var(--app-tab-active);
  }

  /* 按下：整顆微縮。 */
  :global(:root[data-app='ios']) .tab:active {
    transform: scale(0.94);
  }

  :global(:root[data-app='ios']) .tab:focus-visible {
    outline-offset: -2px;
  }

  :global(:root[data-app='ios']) .tab-label {
    font-size: 10px;
    font-weight: 600;
    line-height: 12px;
  }

  /* ---------------------------------------------------------------------------
     Android：Material 3 Expressive 的 navigation bar。貼底、滿寬、直角，高 64（底下再加系統手勢列的 inset）。
     inset 照 Chrome 的建議：bottom 用 inset − max-inset、padding 用 max-inset，手勢列高度變化時整條平移、不重排；
     不支援 safe-area-max-inset-bottom 的瀏覽器，巢狀的 env() 退回一般的 safe-area-inset-bottom（bottom 0、padding inset）。 */

  :global(:root[data-app='android']) .tabbar {
    right: 0;
    bottom: calc(
      env(safe-area-inset-bottom, 0px) - env(safe-area-max-inset-bottom, env(safe-area-inset-bottom, 0px))
    );
    left: 0;
    max-width: var(--app-col);
    margin-inline: auto;
    padding-bottom: env(safe-area-max-inset-bottom, env(safe-area-inset-bottom, 0px));
    /* M3 用色調分層：不用陰影，也不畫分隔線（參考 Google Play），靠面板色和內文的背景色分開。 */
    background: var(--c-panel);
  }

  :global(:root[data-app='android']) .tabbar-track {
    height: 64px;
  }

  /* 上留白 6 → 指示器 32 → 間距 4 → 標籤 16 → 下留白 6。整格都是觸控範圍。 */
  :global(:root[data-app='android']) .tab {
    justify-content: flex-start;
    gap: 4px;
    min-width: 48px;
    padding: 6px 0;
    color: var(--c-text-dim);
    transition: color 150ms ease;
  }

  /* 選中（參考 Google Play）：文字用主色（M3 的 secondary），圖示用主色的淡色（onSecondaryContainer）、換成實心版；都不是純白。 */
  :global(:root[data-app='android']) .tab.is-active {
    color: var(--app-tab-active);
  }

  :global(:root[data-app='android']) .tab.is-active .tab-icon {
    color: var(--app-on-indicator);
  }

  /* 實心版：內部和外框同色；房子的門、書的書脊是挖空的（TabFilled 的遮罩）。有實心版的那一顆，選中時藏起線條版。 */
  :global(:root[data-app='android']) .tab.is-active .tab-icon :global(.tab-filled) {
    display: block;
  }

  :global(:root[data-app='android']) .tab.is-active .tab-icon.has-filled :global(.icon:not(.tab-filled)) {
    display: none;
  }

  /* 放大鏡的鏡片要留空，所以不填，選中時只把線條加粗一點；「更多」的三個點本來就是實心。 */
  :global(:root[data-app='android']) .tab.is-active[data-panel='search'] .tab-icon :global(svg) {
    stroke-width: 2.6;
  }

  :global(:root[data-app='android']) .tab:focus-visible {
    outline: none;
  }

  :global(:root[data-app='android']) .tab-icon {
    width: 56px;
    height: 32px;
    border-radius: var(--radius-pill);
    transition: background-color 120ms ease;
  }

  /* 圖示比 24 小一級（使用者 2026-11-15 指定）：線條圖示幾乎畫滿外框，22 看起來才接近 Google Play 的大小。 */
  :global(:root[data-app='android']) .tab-icon :global(svg) {
    width: 22px;
    height: 22px;
  }

  /* 指示器：選中時從中心水平展開（從圖示的寬度 22/56 展到 56；240ms、輕微回彈），透明度 150ms。 */
  :global(:root[data-app='android']) .tab-icon::before {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--app-indicator);
    opacity: 0;
    transform: scaleX(0.39);
    transition:
      transform 240ms var(--ease-spring),
      opacity 150ms ease,
      background-color 120ms ease;
  }

  :global(:root[data-app='android']) .tab.is-active .tab-icon::before {
    opacity: 1;
    transform: none;
  }

  :global(:root[data-app='android']) .tab-label {
    font-size: 12px;
    font-weight: 500;
    line-height: 16px;
    letter-spacing: 0.5px;
  }

  /* 狀態層（只在指示器的範圍內）：hover 8%、按下與聚焦 10% 的前景色，混成不透明色。 */
  @media (hover: hover) {
    :global(:root[data-app='android']) .tab:hover .tab-icon {
      background: color-mix(in srgb, var(--c-text) 8%, var(--c-panel));
    }

    :global(:root[data-app='android']) .tab.is-active:hover .tab-icon::before {
      background: color-mix(in srgb, var(--c-text-strong) 8%, var(--app-indicator));
    }
  }

  :global(:root[data-app='android']) .tab:active .tab-icon,
  :global(:root[data-app='android']) .tab:focus-visible .tab-icon {
    background: color-mix(in srgb, var(--c-text) 10%, var(--c-panel));
  }

  :global(:root[data-app='android']) .tab.is-active:active .tab-icon::before,
  :global(:root[data-app='android']) .tab.is-active:focus-visible .tab-icon::before {
    background: color-mix(in srgb, var(--c-text-strong) 10%, var(--app-indicator));
  }

  :global(:root[data-app='android']) .tab:focus-visible .tab-icon {
    outline: 2px solid var(--c-focus);
    outline-offset: 2px;
  }

  /* 高對比：描邊加粗、改用較深的描邊色。 */
  @media (prefers-contrast: more) {
    :global(:root[data-app='ios']) .tabbar {
      border-width: 2px;
      border-color: var(--c-border-strong);
    }

    :global(:root[data-app='android']) .tabbar {
      border-top: 2px solid var(--c-border-strong);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(:root[data-app]) .tabbar-glider,
    :global(:root[data-app]) .tab,
    :global(:root[data-app]) .tab-icon,
    :global(:root[data-app]) .tab-icon::before {
      transition: none;
    }

    :global(:root[data-app]) .tab:active {
      transform: none;
    }
  }
</style>
