<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { reducedMotion } from '$app/lib/press';
  import { afterNavigate, beforeNavigate, goto } from '$app/navigation';
  import { page } from '$app/state';
  import { ui } from '$lib/shell/ui.svelte';
  import { platform } from './platform.svelte';

  /**
   * App 外框的面板（目錄、搜尋、更多）：底部導覽列要一直看得到、按得到，所以面板不是 modal
   * （Sheet 在 App 外框改用 dialog.show()）。modal 原本給的東西在這裡補上：
   * - 面板開著時，面板與底部導覽列以外的整頁（頂列、內容、頁尾…）設 inert；捲動由 Sheet 鎖住。
   * - Tab 只在「面板 → 底列」之間循環；焦點在底列時按 Esc 也會關（焦點在面板裡的 Esc 由 Sheet 處理）。
   * - 關閉後焦點回到打開它的那一顆（從內文用 Ctrl+K 打開的就回到原本的地方）。
   * - 系統返回（Android 返回鍵、iOS 邊緣返回）先關面板：打開時推一筆淺層紀錄（page.state.appPanel），
   *   按返回回到前一筆就關面板；從介面關掉時自己 history.back() 把那一筆收掉。
   *   在面板裡點連結換頁時改成「取代」那一筆，換頁後按返回直接回到原本那一頁，不會停在「面板開著」的那一筆。
   * 一般外框（data-app 沒有值）時什麼都不做，抽屜與搜尋照舊是 modal。
   */
  const PANEL_KEY = 'appPanel';
  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
  const SKIP_TAGS = new Set(['SCRIPT', 'STYLE', 'LINK', 'META', 'TEMPLATE', 'NOSCRIPT']);

  const active = $derived(platform.app !== null && ui.anyPanel);

  /** 正在換頁（beforeNavigate 到 afterNavigate 之間）：這時關掉面板不收紀錄、不搬焦點，交給換頁。 */
  let navigating = false;
  let replacing = false;
  /** 打開面板之前焦點所在（底列的按鈕，或從內文用快捷鍵打開時的那個元素）。 */
  let opener: HTMLElement | null = null;
  let lastPanel: 'toc' | 'search' | 'more' | null = null;
  let wasActive = false;
  const inerted = new Set<Element>();

  function onPanelEntry(): boolean {
    return !!(page.state as Record<string, unknown>)[PANEL_KEY];
  }

  beforeNavigate((navigation) => {
    if (navigation.shallow || navigation.type === 'popstate' || navigation.willUnload) return;
    navigating = true;
    if (replacing || !onPanelEntry() || !navigation.to) return;
    if (navigation.type !== 'link' && navigation.type !== 'goto') return;
    // 從面板裡換頁：取代「面板開著」那一筆紀錄。
    navigation.cancel();
    replacing = true;
    void goto(navigation.to.url, { replace: true }).finally(() => {
      replacing = false;
    });
  });

  afterNavigate((navigation) => {
    if (navigation.shallow) return;
    navigating = false;
    linkNavigation = false;
    ui.navigatingAway = false;
  });

  /**
   * 點了站內連結（目錄裡的頁面、更多裡的下載、底列的首頁…）：SvelteKit 要等一下才開始換頁（beforeNavigate 是非同步的），
   * 在那之前關面板的收尾就會先 history.back()，把換頁蓋掉。所以在點下去的當下（capture）就記起來。
   */
  let linkNavigation = false;
  let linkTimer: ReturnType<typeof setTimeout> | undefined;

  function onClickCapture(event: MouseEvent) {
    if (!platform.app || event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const link = (event.target as Element | null)?.closest?.('a[href]');
    if (!(link instanceof HTMLAnchorElement) || link.target || link.hasAttribute('download')) return;
    const url = new URL(link.href, location.href);
    if (url.origin !== location.origin) return;
    if (url.pathname === location.pathname && url.search === location.search) return;
    linkNavigation = true;
    clearTimeout(linkTimer);
    // 萬一沒有真的換頁（被別的程式擋下），過一陣子就不再當成換頁中。
    linkTimer = setTimeout(() => (linkNavigation = false), 3000);
  }

  $effect(() => {
    document.addEventListener('click', onClickCapture, true);
    return () => {
      document.removeEventListener('click', onClickCapture, true);
      clearTimeout(linkTimer);
    };
  });

  function tabFor(panel: typeof lastPanel): HTMLElement | null {
    return panel ? document.querySelector<HTMLElement>(`.tabbar [data-panel="${panel}"]`) : null;
  }

  function isUsable(element: Element | null): element is HTMLElement {
    return (
      element instanceof HTMLElement &&
      element.isConnected &&
      element.getClientRects().length > 0 &&
      !element.closest('[inert], [hidden], dialog:not([open])')
    );
  }

  /** Tab 的循環：目前面板裡可聚焦的元素，接著底列的四顆。 */
  function cycle(): HTMLElement[] {
    const surface = document.querySelector<HTMLElement>('dialog.sheet-layer[open] .sheet:not(.is-closing)');
    const inPanel = surface ? [...surface.querySelectorAll<HTMLElement>(FOCUSABLE)] : [];
    const tabs = [...document.querySelectorAll<HTMLElement>('.tabbar .tab')];
    return [...inPanel, ...tabs].filter(isUsable);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.isComposing) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      ui.closePanels();
      return;
    }
    if (event.key !== 'Tab' || event.altKey || event.ctrlKey || event.metaKey) return;
    const list = cycle();
    if (list.length === 0) return;
    event.preventDefault();
    const index = list.indexOf(document.activeElement as HTMLElement);
    const step = event.shiftKey ? -1 : 1;
    const next = index < 0 ? (event.shiftKey ? list.length - 1 : 0) : (index + step + list.length) % list.length;
    list[next].focus();
  }

  /** 面板與底列以外的整頁設 inert（從 body 往下走：包含它們的元素往裡走，其他整塊設 inert）。 */
  function updateInert() {
    for (const element of inerted) element.removeAttribute('inert');
    inerted.clear();
    if (!active) return;
    const keep = [
      ...document.querySelectorAll('dialog.sheet-layer[open]'),
      document.querySelector('.tabbar'),
    ].filter((element): element is Element => !!element);
    const walk = (parent: Element) => {
      for (const child of parent.children) {
        if (keep.includes(child) || SKIP_TAGS.has(child.tagName)) continue;
        if (keep.some((element) => child.contains(element))) walk(child);
        else if (!child.hasAttribute('inert')) {
          child.setAttribute('inert', '');
          inerted.add(child);
        }
      }
    };
    walk(document.body);
  }

  function onOpen() {
    const focused = document.activeElement;
    opener = focused instanceof HTMLElement && focused !== document.body ? focused : null;
    document.addEventListener('keydown', onKeydown);
    if (!onPanelEntry()) {
      void goto(location.href, { state: { ...page.state, [PANEL_KEY]: true }, shallow: true }).catch(() => {
        // 推不進紀錄時，系統返回就照一般的返回（離開這一頁），面板本身照常運作。
      });
    }
  }

  function scrollToTop() {
    window.scrollTo({ top: 0, behavior: reducedMotion() ? 'instant' : 'smooth' });
  }

  function onClose(panel: typeof lastPanel) {
    document.removeEventListener('keydown', onKeydown);
    const scrollTop = ui.scrollTopAfterClose;
    ui.scrollTopAfterClose = false;
    // 等這次點擊（例如目錄裡的連結）的換頁開始之後再決定：換頁的話什麼都不做。
    setTimeout(() => {
      if (navigating || linkNavigation || ui.navigatingAway) return;
      if (onPanelEntry()) {
        // 收掉「面板開著」那一筆；回到前一筆時瀏覽器會還原捲動位置，要捲到頂就等它還原完。
        if (scrollTop) {
          window.addEventListener('popstate', () => requestAnimationFrame(() => requestAnimationFrame(scrollToTop)), {
            once: true,
          });
        }
        history.back();
      } else if (scrollTop) {
        scrollToTop();
      }
      // 焦點還在面板裡（或掉到 body）時，還給打開它的那一顆。
      const focused = document.activeElement;
      if (focused && focused !== document.body && !focused.closest('dialog.sheet-layer')) return;
      const fromTabbar = !opener || !!opener.closest('.tabbar');
      const target = fromTabbar ? tabFor(panel) : opener;
      (isUsable(target) ? target : tabFor(panel))?.focus({ preventScroll: true });
    }, 0);
  }

  $effect(() => {
    const now = active;
    const panel = ui.panel;
    untrack(() => {
      if (now && !wasActive) onOpen();
      else if (!now && wasActive) onClose(lastPanel);
      if (now) lastPanel = panel;
      wasActive = now;
      void tick().then(updateInert);
    });
  });

  // 系統返回回到「面板開著」的前一筆：關掉面板。
  let hadEntry = false;
  $effect(() => {
    const entry = onPanelEntry();
    untrack(() => {
      if (hadEntry && !entry && ui.anyPanel && platform.app) ui.closePanels();
      hadEntry = entry;
    });
  });

  $effect(() => () => {
    document.removeEventListener('keydown', onKeydown);
    for (const element of inerted) element.removeAttribute('inert');
    inerted.clear();
  });
</script>
