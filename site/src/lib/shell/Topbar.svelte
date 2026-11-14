<script lang="ts">
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { normalizePath, SITE } from '$lib/nav';
  import ExtraIcon from './ExtraIcon.svelte';
  import Logo from './Logo.svelte';
  import ThemeSwitch from './ThemeSwitch.svelte';
  import { isMac, ui } from './ui.svelte';

  /**
   * 頂列：站名、搜尋入口（顯示快捷鍵）、外觀切換、下載。
   * 手機寬度（< 768px）：站名＋「搜尋」「選單」兩顆圓鈕；往下捲時收起、往上捲時滑回來（規則見下方 onScroll）。
   */
  let mac = $state(false);
  /** 頁面是否已經往下捲：在頂端時頂列和背景同色，捲動後才浮起來。 */
  let scrolled = $state(false);
  /** 手機寬度往下捲時收起（只在 < 768px 生效，CSS 也只在這個寬度套用）。 */
  let hidden = $state(false);
  let header = $state<HTMLElement | null>(null);

  /** 這次捲動方向累積超過這個距離才收起／出現，手指輕微晃動不會讓頂列閃來閃去。 */
  const TOGGLE_PX = 8;
  /** 捲動位置還在頂端這個距離以內（約一個頂列高）一定顯示。 */
  const reveal = () => (header?.offsetHeight ?? 56);

  let lastY = 0;
  /** 同方向累積的捲動距離：往下為正、往上為負，換方向時歸零重算。 */
  let travel = 0;
  let frame = 0;
  let mobile: MediaQueryList | null = null;

  /** 鍵盤焦點在頂列裡時不收起（看不見的東西不能拿著焦點）。 */
  function holdsKeyboardFocus(): boolean {
    return !!header?.querySelector(':focus-visible');
  }

  function show() {
    hidden = false;
    travel = 0;
  }

  /**
   * 焦點進到頂列：立刻出現（不等轉場）。
   * 另外，用 Tab 聚焦時瀏覽器會把元素捲到 html 的 scroll-padding-top（含頂列高度）以下；
   * 頂列是 sticky，永遠在那條線上方，瀏覽器就會把整頁往上捲一大段（而且是在 focusin 之前就捲了）。
   * 鍵盤焦點進來時，在下一次繪製前捲回原位：原位是上一次捲動處理記下的 lastY（這次跳動的捲動事件還沒處理）。
   */
  function onFocusIn() {
    if (hidden && header) {
      header.style.transition = 'none';
      header.classList.remove('is-hidden');
      void header.offsetHeight;
      header.style.transition = '';
    }
    show();
    if (!header?.querySelector(':focus-visible')) return;
    const before = lastY;
    const href = location.href;
    requestAnimationFrame(() => {
      if (location.href === href && Math.abs(window.scrollY - before) > 1) {
        window.scrollTo({ top: before, behavior: 'instant' });
        lastY = before;
      }
    });
  }

  function update() {
    frame = 0;
    const root = document.documentElement;
    // iOS 回彈時 scrollY 會超出範圍：夾在 0～最底，回彈不算一次捲動。
    const y = Math.min(Math.max(window.scrollY, 0), Math.max(root.scrollHeight - window.innerHeight, 0));
    const delta = y - lastY;
    lastY = y;
    scrolled = y > 4;

    if (!mobile?.matches || y <= reveal()) {
      show();
      return;
    }
    if (delta === 0) return;
    travel = Math.sign(delta) === Math.sign(travel) ? travel + delta : delta;
    if (!hidden && travel > TOGGLE_PX) {
      if (ui.searchOpen || ui.menuOpen || holdsKeyboardFocus()) return;
      hidden = true;
      travel = 0;
    } else if (hidden && travel < -TOGGLE_PX) {
      show();
    }
  }

  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(update);
  };

  onMount(() => {
    mac = isMac();
    mobile = window.matchMedia('(width < 768px)');
    lastY = Math.max(window.scrollY, 0);
    update();
    window.addEventListener('scroll', schedule, { passive: true });
    mobile.addEventListener('change', schedule);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('scroll', schedule);
      mobile?.removeEventListener('change', schedule);
    };
  });

  // 換頁之後一定顯示；從現在的位置重新起算，跳到錨點的那一下不算「往下捲」。
  afterNavigate(() => {
    lastY = Math.max(window.scrollY, 0);
    show();
  });

  // 搜尋或選單開著時一定顯示。
  $effect(() => {
    if (ui.searchOpen || ui.menuOpen) show();
  });

  const atDownload = $derived(normalizePath(page.url.pathname) === SITE.download);
</script>

<header bind:this={header} class="topbar" class:is-scrolled={scrolled} class:is-hidden={hidden} onfocusin={onFocusIn}>
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

  <a class="btn desktop-only" href={SITE.download} aria-current={atDownload ? 'page' : undefined}>
    <Icon name="download" />下載
  </a>

  <div class="mobile-actions">
    <button
      type="button"
      class="btn btn--round"
      aria-label="搜尋手冊"
      aria-haspopup="dialog"
      onclick={() => ui.openSearch()}
    >
      <Icon name="search" size={18} />
    </button>
    <button
      type="button"
      class="btn btn--round"
      aria-label="目錄與選單"
      aria-haspopup="dialog"
      aria-expanded={ui.menuOpen}
      onclick={() => (ui.menuOpen = true)}
    >
      <ExtraIcon name="menu" size={18} />
    </button>
  </div>
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

  .mobile-actions {
    display: none;
  }

  /* 手機寬度：站名＋搜尋、選單兩顆圓鈕。頂列一樣黏在上方，往下捲時整條往上滑出（只動 transform），往上捲滑回來。 */
  @media (width < 768px) {
    .topbar {
      gap: var(--space-2);
      height: calc(var(--topbar-h) + env(safe-area-inset-top, 0px));
      padding: env(safe-area-inset-top, 0px) var(--space-2) 0 var(--space-3);
      transition:
        transform 220ms var(--ease-out),
        background-color 200ms ease,
        border-color 200ms ease,
        box-shadow 200ms ease;
    }

    /* 收起：陰影在滑出去之後才拿掉，不會在畫面最上緣留下一條影子。 */
    .topbar.is-hidden {
      transform: translateY(-100%);
      box-shadow: none;
      transition:
        transform 220ms var(--ease-out),
        box-shadow 0s linear 220ms;
    }

    .search-entry,
    .topbar :global(.theme-switch),
    .desktop-only {
      display: none;
    }

    .mobile-actions {
      display: flex;
      flex: none;
      gap: var(--space-2);
    }
  }

  /* 減少動態：照樣收起與出現，只是瞬間完成。 */
  @media (prefers-reduced-motion: reduce) {
    .topbar,
    .topbar.is-hidden,
    .search-entry {
      transition: none;
    }
  }
</style>
