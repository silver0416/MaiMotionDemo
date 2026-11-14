<script lang="ts">
  import { tick, untrack, type Snippet } from 'svelte';
  import Icon from '$app/components/Icon.svelte';
  import { reducedMotion } from '$app/lib/press';

  /**
   * 對話框／底部抽屜（做法照 BOTPu admin 的 Dialog，外觀照 App 的 SearchDialog）：
   * - 原生 <dialog> 的 showModal；不用半透明遮罩（::backdrop 透明），靠邊框與陰影分層。
   * - 電腦寬度置中（placement="top" 時靠上，給搜尋用）；手機寬度（< 768px）從底部滑入，上緣圓角 --radius-dialog；
   *   mobile="full" 時鋪滿整個畫面（搜尋）。
   * - Esc、點框外、右上角關閉鈕都會關；關閉動畫用計時器收尾，不依賴 animationend。
   * - 開啟時焦點進入（有 data-autofocus 的元素優先）；關閉時瀏覽器把焦點還給原本的按鈕。
   * - <dialog> 本身只是鋪滿視窗的透明層，外觀與動畫在裡面的 .sheet。
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

  $effect(() => () => clearTimeout(closeTimer));

  function requestClose() {
    open = false;
  }

  function onNativeClose() {
    clearTimeout(closeTimer);
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
