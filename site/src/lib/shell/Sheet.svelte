<script lang="ts" module>
  /**
   * 開著對話框時鎖住整頁捲動（觸控拖曳不會捲到後面的頁面）。
   * 只在 html 上設 overflow: hidden，捲動位置不動；原本有捲軸的話用 scrollbar-gutter 保留它的寬度，版面不會橫移。
   * 用計數：搜尋與選單交接（一個關、一個開）時不會提早解鎖。
   */
  let locks = 0;

  function lockScroll() {
    locks += 1;
    if (locks > 1) return;
    const root = document.documentElement;
    const gutter = window.innerWidth - root.clientWidth;
    if (gutter > 0) root.style.scrollbarGutter = 'stable';
    root.style.overflow = 'hidden';
  }

  function unlockScroll() {
    if (locks === 0) return;
    locks -= 1;
    if (locks > 0) return;
    const root = document.documentElement;
    root.style.overflow = '';
    root.style.scrollbarGutter = '';
  }
</script>

<script lang="ts">
  import { tick, untrack, type Snippet } from 'svelte';
  import Icon from '$app/components/Icon.svelte';
  import { reducedMotion } from '$app/lib/press';
  import { afterNavigate } from '$app/navigation';

  /**
   * 對話框／底部抽屜（做法照 BOTPu admin 的 Dialog，外觀照 App 的 SearchDialog）：
   * - 原生 <dialog> 的 showModal；不用半透明遮罩（::backdrop 透明），靠邊框與陰影分層。
   * - 電腦寬度置中（placement="top" 時靠上，給搜尋用）；手機寬度（< 768px）從底部滑入，上緣圓角 --radius-dialog；
   *   mobile="full" 時鋪滿整個畫面（搜尋）。
   * - Esc、點框外、右上角關閉鈕都會關；關閉動畫用計時器收尾，不依賴 animationend。
   * - 開啟時焦點進入（有 data-autofocus 的元素優先）；關閉時瀏覽器把焦點還給原本的按鈕
   *   （開著時換了頁就改成聚焦新頁面的 #main）。
   * - <dialog> 本身只是鋪滿視窗的透明層，外觀與動畫在裡面的 .sheet。
   * - 開著時鎖住整頁捲動（見上方 lockScroll）。
   */
  interface Props {
    open: boolean;
    title: string;
    /** 標題只給輔助技術（自訂標題列時用）。 */
    hideTitle?: boolean;
    /** 寬度：sm 420、md 560、lg 640。 */
    size?: 'sm' | 'md' | 'lg';
    placement?: 'center' | 'top';
    mobile?: 'sheet' | 'full';
    /** 取代預設標題列右側的內容（例如搜尋框）。 */
    head?: Snippet;
    /** 標題列下方、不跟著內容捲動的一排（例如手機選單的工具列）。 */
    top?: Snippet;
    children: Snippet;
    foot?: Snippet;
    onClose?: () => void;
  }

  let {
    open = $bindable(),
    title,
    hideTitle = false,
    size = 'md',
    placement = 'center',
    mobile = 'sheet',
    head,
    top,
    children,
    foot,
    onClose,
  }: Props = $props();

  const uid = $props.id();
  const CLOSE_MS = 140;
  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  let dialog = $state<HTMLDialogElement | null>(null);
  let surface = $state<HTMLDivElement | null>(null);
  let rendered = $state(false);
  let closing = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  /** 按下時就在框外才算「點外面」：在框內開始選字、拖到框外放開不會誤關。 */
  let pressedOutside = false;

  async function focusInitial(element: HTMLDialogElement) {
    await tick();
    const target =
      element.querySelector<HTMLElement>('[data-autofocus]') ??
      element.querySelector('.sheet-body')?.querySelector<HTMLElement>(FOCUSABLE) ??
      element;
    target.focus();
  }

  $effect(() => {
    const element = dialog;
    const wantOpen = open;
    if (!element) return;
    untrack(() => {
      if (wantOpen) {
        clearTimeout(closeTimer);
        closing = false;
        if (!element.open) {
          rendered = true;
          element.showModal();
          setLocked(true);
          void focusInitial(element);
        }
      } else if (element.open && !closing) {
        const finish = () => {
          closing = false;
          if (element.open) element.close();
        };
        if (reducedMotion()) {
          finish();
        } else {
          closing = true;
          closeTimer = setTimeout(finish, CLOSE_MS);
        }
      }
    });
  });

  $effect(() => () => {
    clearTimeout(closeTimer);
    setLocked(false);
  });

  let locked = false;
  function setLocked(next: boolean) {
    if (next === locked) return;
    locked = next;
    if (next) lockScroll();
    else unlockScroll();
  }

  function requestClose() {
    open = false;
  }

  // 開著時換了頁（點目錄連結、選搜尋結果）：關閉後焦點交給新頁面的主要內容，不回到頂列的按鈕。
  let navigated = false;
  afterNavigate(() => {
    if (dialog?.open) navigated = true;
  });

  function onNativeClose() {
    clearTimeout(closeTimer);
    setLocked(false);
    if (navigated) {
      navigated = false;
      document.getElementById('main')?.focus({ preventScroll: true });
    }
    closing = false;
    rendered = false;
    if (open) open = false;
    onClose?.();
  }

  function isOutside(event: MouseEvent): boolean {
    if (!dialog || !surface || event.target !== dialog) return false;
    const rect = surface.getBoundingClientRect();
    return (
      event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom
    );
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    // Esc 自己處理（不靠瀏覽器的 cancel，連按兩次時行為不一致）；輸入法組字中的 Esc 不關。
    if (event.key === 'Escape' && !event.isComposing) {
      event.preventDefault();
      requestClose();
    }
  }
</script>

<dialog
  bind:this={dialog}
  class={['sheet-layer', `sheet-layer--${placement}`, `sheet-layer--m-${mobile}`]}
  tabindex="-1"
  aria-labelledby={`${uid}-title`}
  oncancel={(event) => {
    event.preventDefault();
    requestClose();
  }}
  onclose={onNativeClose}
  onpointerdown={(event) => (pressedOutside = isOutside(event))}
  onclick={(event) => {
    if (pressedOutside && isOutside(event)) requestClose();
    pressedOutside = false;
  }}
  onkeydown={onKeydown}
>
  {#if rendered}
    <div bind:this={surface} class={['sheet', `sheet--${size}`, closing && 'is-closing']}>
      <header class={['dialog-head', 'sheet-head', head && 'has-custom']}>
        <h2 id={`${uid}-title`} class:sr-only={hideTitle}>{title}</h2>
        {#if head}{@render head()}{/if}
        <button type="button" class="btn btn--round" aria-label="關閉" onclick={requestClose}>
          <Icon name="x" />
        </button>
      </header>
      {#if top}
        <div class="sheet-top">{@render top()}</div>
      {/if}
      <div class="sheet-body scroll">
        {@render children()}
      </div>
      {#if foot}
        <footer class="sheet-foot">{@render foot()}</footer>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  .sheet-layer {
    max-width: none;
    max-height: none;
    padding: 0;
    overflow: hidden;
    overscroll-behavior: contain;
    border: none;
    background: transparent;
    color: var(--c-text);
  }

  .sheet-layer[open] {
    display: grid;
    place-items: center;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: var(--space-4);
  }

  .sheet-layer--top[open] {
    place-items: start center;
    padding-top: min(12vh, 96px);
  }

  .sheet-layer:focus-visible {
    outline: none;
  }

  /* 不用半透明遮罩，捲動也不會傳到後面的頁面。 */
  .sheet-layer::backdrop {
    background: none;
    overscroll-behavior: contain;
  }

  .sheet {
    display: flex;
    flex-direction: column;
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    overflow: hidden;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-dialog);
    background: var(--c-panel);
    box-shadow: var(--shadow-dialog);
    animation: sheet-dialog-in 220ms cubic-bezier(0.2, 0.9, 0.3, 1.15);
  }

  .sheet-layer--top .sheet {
    max-height: min(640px, calc(100dvh - min(12vh, 96px) - 32px));
  }

  .sheet--sm {
    width: min(420px, calc(100vw - 32px));
  }

  .sheet--lg {
    width: min(640px, calc(100vw - 32px));
  }

  .sheet.is-closing {
    animation: sheet-dialog-out 140ms ease-in forwards;
  }

  @keyframes sheet-dialog-in {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.96);
    }
  }

  @keyframes sheet-dialog-out {
    to {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
    }
  }

  .sheet-head {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--c-border);
  }

  .sheet-head h2 {
    flex: 1 1 auto;
    min-width: 0;
  }

  .sheet-head.has-custom {
    padding-left: var(--space-4);
  }

  .sheet-top {
    flex: none;
    padding: var(--space-3);
    border-bottom: 1px solid var(--c-border);
  }

  .sheet-body {
    flex: 1 1 auto;
    min-height: 0;
    padding: var(--space-3);
    overscroll-behavior: contain;
  }

  .sheet-foot {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4) var(--space-3) var(--space-5);
    border-top: 1px solid var(--c-border);
  }

  /* 手機寬度：從底部滑入的抽屜；mobile="full" 鋪滿整個畫面（搜尋）。 */
  @media (width < 768px) {
    .sheet-layer[open],
    .sheet-layer--top[open] {
      place-items: end stretch;
      padding: var(--space-6) 0 0;
    }

    .sheet,
    .sheet--sm,
    .sheet--lg {
      width: 100%;
      max-height: 100%;
      border-bottom: none;
      border-radius: var(--radius-dialog) var(--radius-dialog) 0 0;
      animation: sheet-up-in 220ms var(--ease-out);
    }

    .sheet.is-closing {
      animation: sheet-up-out 140ms ease-in forwards;
    }

    .sheet-layer--m-full[open] {
      place-items: stretch;
      padding: 0;
    }

    .sheet-layer--m-full .sheet {
      height: 100dvh;
      max-height: none;
      border: none;
      border-radius: 0;
      padding-top: env(safe-area-inset-top, 0px);
    }

    .sheet-head {
      padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
    }

    .sheet-body {
      padding-bottom: max(var(--space-3), env(safe-area-inset-bottom, 0px));
    }

    .sheet-foot {
      padding: var(--space-3) var(--space-4) max(var(--space-3), env(safe-area-inset-bottom, 0px));
    }

    .sheet-layer--m-full .sheet-foot {
      display: none;
    }
  }

  @keyframes sheet-up-in {
    from {
      opacity: 0;
      transform: translateY(32px);
    }
  }

  @keyframes sheet-up-out {
    to {
      opacity: 0;
      transform: translateY(32px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .sheet,
    .sheet.is-closing {
      animation: none;
    }
  }
</style>
