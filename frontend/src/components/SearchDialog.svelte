<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import MajdataSearchPanel from './MajdataSearchPanel.svelte';
  import WikiSearchPanel from './WikiSearchPanel.svelte';
  import { reducedMotion } from '../lib/press';

  interface Props {
    open: boolean;
  }

  let { open = $bindable() }: Props = $props();

  type Target = 'majdata' | 'wiki';

  const TARGETS: { id: Target; label: string }[] = [
    { id: 'wiki', label: 'simai Wiki' },
    { id: 'majdata', label: 'Majdata' },
  ];
  const TARGET_KEY = 'maimotion.searchTarget';

  function initialTarget(): Target {
    try {
      return localStorage.getItem(TARGET_KEY) === 'majdata' ? 'majdata' : 'wiki';
    } catch {
      return 'wiki';
    }
  }

  let dialog: HTMLDialogElement | null = $state(null);
  let target = $state<Target>(initialTarget());
  /** 兩個分頁共用關鍵字：切換目標時不必重打。 */
  let query = $state('');
  let majdataLocked = $state(false);
  let wikiLocked = $state(false);
  let majdataPanel: MajdataSearchPanel | null = $state(null);
  let wikiPanel: WikiSearchPanel | null = $state(null);
  const tabs: Partial<Record<Target, HTMLButtonElement>> = {};

  /** 匯入沒有取消機制：進行中時鎖住關閉與切換，避免結果在看不到的地方套用。 */
  const locked = $derived(majdataLocked || wikiLocked);

  /** 關閉動畫播放中；播完才真正呼叫 dialog.close()。 */
  let closing = $state(false);
  /** 按下滑鼠時就在框外，才算「點外面」；在框內開始選字、拖到框外放開不會誤關。 */
  let pressedOutside = false;

  const CLOSE_MS = 140;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!dialog) return;
    if (open) {
      clearTimeout(closeTimer);
      closing = false;
      if (!dialog.open) {
        dialog.showModal();
        void activateCurrent();
      }
    } else if (dialog.open && !closing) {
      if (reducedMotion()) {
        dialog.close();
        return;
      }
      closing = true;
      // 不等 animationend：視窗被遮住時動畫事件可能不觸發，隱形的 modal 會擋住整個頁面。
      closeTimer = setTimeout(finishClose, CLOSE_MS);
    }
  });

  function finishClose() {
    closing = false;
    if (!open) dialog?.close();
  }

  async function activateCurrent() {
    await tick();
    (target === 'majdata' ? majdataPanel : wikiPanel)?.activate();
  }

  function requestClose() {
    if (locked) return;
    open = false;
  }

  function onDialogClosed() {
    open = false;
    majdataPanel?.reset();
    wikiPanel?.reset();
  }

  function isOutside(event: MouseEvent): boolean {
    if (!dialog || event.target !== dialog) return false;
    const rect = dialog.getBoundingClientRect();
    return (
      event.clientX < rect.left ||
      event.clientX > rect.right ||
      event.clientY < rect.top ||
      event.clientY > rect.bottom
    );
  }

  function selectTarget(next: Target) {
    if (locked || target === next) return;
    target = next;
    try {
      localStorage.setItem(TARGET_KEY, next);
    } catch {
      // 無法寫入時只是下次不記得上次的目標。
    }
    void activateCurrent();
  }

  function onTabKeydown(event: KeyboardEvent) {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    const index = TARGETS.findIndex((item) => item.id === target);
    const step = event.key === 'ArrowRight' ? 1 : -1;
    const next = TARGETS[(index + step + TARGETS.length) % TARGETS.length].id;
    selectTarget(next);
    tabs[target]?.focus();
  }

  function onDialogKeydown(event: KeyboardEvent) {
    // 不依賴瀏覽器原生的 cancel 行為（WebView 之間不一致），Esc 直接關閉（匯入中忽略）。
    if (event.key === 'Escape') {
      event.preventDefault();
      requestClose();
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="search-title"
  onclose={onDialogClosed}
  oncancel={(event) => {
    event.preventDefault();
    requestClose();
  }}
  onkeydown={onDialogKeydown}
  onpointerdown={(event) => (pressedOutside = isOutside(event))}
  onclick={(event) => {
    if (pressedOutside && isOutside(event)) requestClose();
    pressedOutside = false;
  }}
>
  <header class="dialog-head">
    <h2 id="search-title">搜尋譜面</h2>
    <div class="targets" role="tablist" aria-label="搜尋目標">
      {#each TARGETS as item (item.id)}
        <button
          bind:this={tabs[item.id]}
          id={`search-tab-${item.id}`}
          class="btn target"
          class:is-active={target === item.id}
          role="tab"
          aria-selected={target === item.id}
          aria-controls={`search-panel-${item.id}`}
          tabindex={target === item.id ? 0 : -1}
          disabled={locked && target !== item.id}
          title={locked && target !== item.id ? '匯入完成後才能切換' : undefined}
          onclick={() => selectTarget(item.id)}
          onkeydown={onTabKeydown}
        >
          {item.label}
        </button>
      {/each}
    </div>
    <span class="spacer"></span>
    <button class="btn btn--icon" onclick={requestClose} disabled={locked} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <!-- 兩個分頁都保持掛載：切回來時結果、篩選與 Wiki 索引狀態都還在。 -->
  <div
    id="search-panel-majdata"
    class="tabpanel"
    role="tabpanel"
    aria-labelledby="search-tab-majdata"
    hidden={target !== 'majdata'}
  >
    <MajdataSearchPanel
      bind:this={majdataPanel}
      bind:query
      bind:locked={majdataLocked}
      active={open && target === 'majdata'}
      onClose={() => (open = false)}
    />
  </div>
  <div
    id="search-panel-wiki"
    class="tabpanel"
    role="tabpanel"
    aria-labelledby="search-tab-wiki"
    hidden={target !== 'wiki'}
  >
    <WikiSearchPanel
      bind:this={wikiPanel}
      bind:query
      bind:locked={wikiLocked}
      active={open && target === 'wiki'}
      onClose={() => (open = false)}
    />
  </div>

  <footer class="dialog-foot">
    <span class="muted xsmall">Enter 搜尋・Esc 關閉</span>
    <span class="spacer"></span>
    <button class="btn" onclick={requestClose} disabled={locked}>
      <Icon name="x" />關閉
    </button>
  </footer>
</dialog>

<style>
  .dialog {
    width: min(920px, calc(100vw - 32px));
    height: min(720px, calc(100vh - 32px));
    max-height: calc(100vh - 32px);
    padding: 0;
    color: var(--c-text);
    background: var(--c-panel);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
    box-shadow: 0 24px 64px #000000;
  }

  .dialog[open] {
    display: flex;
    flex-direction: column;
    animation: dialog-in 220ms cubic-bezier(0.2, 0.9, 0.3, 1.15);
  }

  .dialog[open].is-closing {
    animation: dialog-out 140ms ease-in forwards;
  }

  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.96);
    }
  }

  @keyframes dialog-out {
    to {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
    }
  }

  /* 規範不用半透明遮罩：背景維持原樣，靠邊框與陰影把對話框和盤面分開。 */
  .dialog::backdrop {
    background: none;
  }

  .dialog-head,
  .dialog-foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
  }

  .dialog-head {
    border-bottom: 1px solid var(--c-border);
  }

  .dialog-foot {
    border-top: 1px solid var(--c-border);
  }

  .targets {
    display: flex;
    gap: 2px;
    padding: 2px;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    /* 外框圓角＝按鈕圓角＋內距，兩層弧線才會同心。 */
    border-radius: calc(var(--radius-md) + 2px);
  }

  .target {
    min-height: 26px;
    padding: 0 var(--space-3);
    font-size: var(--fs-xs);
    border-color: transparent;
  }

  .target:not(.is-active) {
    background: transparent;
  }

  .tabpanel {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    animation: panel-in 160ms ease-out;
  }

  .tabpanel[hidden] {
    display: none;
  }

  @keyframes panel-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dialog[open],
    .dialog[open].is-closing,
    .tabpanel {
      animation: none;
    }
  }
</style>
