<script lang="ts">
  import Icon from './Icon.svelte';
  import { cleanDraft, emptyDraft } from '../lib/annotation';
  import { reducedMotion } from '../lib/press';
  import { annotation } from '../state/annotation.svelte';
  import { recordTitle, records, type ChartRecord } from '../state/records.svelte';
  import { toasts } from '../state/toasts.svelte';

  /**
   * 整首備註：和右側「標註 → 備註」的「整體備註」是同一份資料，這裡只是從譜面紀錄右鍵直接打開的入口。
   * 紀錄沒有開啟也能寫；開啟中的紀錄改走標註草稿，才不會被稍後的自動存檔蓋回去。
   */
  interface Props {
    record: ChartRecord | null;
  }

  let { record = $bindable() }: Props = $props();

  let dialog: HTMLDialogElement | null = $state(null);
  let text = $state('');
  /** 開啟時的內容；沒改就不寫回、不跳提示。 */
  let original = '';

  let closing = $state(false);
  let pressedOutside = false;
  const CLOSE_MS = 140;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  /** 開啟中的紀錄以標註草稿為準，其他紀錄讀存檔。 */
  function currentMemo(target: ChartRecord): string {
    if (annotation.recordId === target.id) return annotation.draft.memo;
    return target.annotation?.memo ?? '';
  }

  $effect(() => {
    if (!dialog) return;
    if (record) {
      clearTimeout(closeTimer);
      closing = false;
      if (!dialog.open) {
        text = original = currentMemo(record);
        dialog.showModal();
        // 焦點直接放進輸入框，游標在最後，打開就能接著寫。
        const input = dialog.querySelector('textarea');
        input?.focus();
        input?.setSelectionRange(original.length, original.length);
      }
    } else if (dialog.open && !closing) {
      if (reducedMotion()) {
        dialog.close();
        return;
      }
      closing = true;
      closeTimer = setTimeout(() => {
        closing = false;
        dialog?.close();
      }, CLOSE_MS);
    }
  });

  function close() {
    record = null;
  }

  function isOutside(event: MouseEvent): boolean {
    if (!dialog || event.target !== dialog) return false;
    const rect = dialog.getBoundingClientRect();
    return (
      event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom
    );
  }

  function write(target: ChartRecord, memo: string) {
    if (annotation.recordId === target.id) {
      annotation.setOverallMemo(memo);
      annotation.flush();
      return;
    }
    const latest = records.get(target.id) ?? target;
    const draft = cleanDraft(latest.annotation) ?? emptyDraft(recordTitle(latest));
    draft.memo = memo;
    draft.updatedAt = Date.now();
    records.setAnnotation(target.id, draft);
  }

  function save() {
    const target = record;
    if (!target) return;
    const memo = text.trim() ? text : '';
    if (memo === original) {
      close();
      return;
    }
    const before = original;
    write(target, memo);
    close();
    toasts.show({
      id: 'overall-memo',
      tone: 'ok',
      title: memo ? '已儲存整首備註' : '已清除整首備註',
      body: `「${recordTitle(target)}」`,
      action: { label: '復原', icon: 'rotate-ccw', run: () => write(target, before) },
    });
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      save();
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="overall-memo-title"
  onclose={() => (record = null)}
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  onpointerdown={(event) => (pressedOutside = isOutside(event))}
  onclick={(event) => {
    if (pressedOutside && isOutside(event)) close();
    pressedOutside = false;
  }}
>
  <header class="dialog-head">
    <h2 id="overall-memo-title">整首備註</h2>
    {#if record}<span class="muted small title-name">{recordTitle(record)}</span>{/if}
    <span class="spacer"></span>
    <button class="btn btn--round" onclick={close} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div class="dialog-body scroll">
    <label class="field">
      <span class="sr-only">整首備註內容</span>
      <textarea
        class="input memo"
        rows="8"
        placeholder="例如：這份是 SSS 玩家的打法、手順的整體習慣、不確定的段落…"
        bind:value={text}
        onkeydown={onKeydown}
      ></textarea>
      <span class="field-hint">
        和右側「標註 → 備註」的整體備註是同一份，會跟著標註一起匯出。
      </span>
    </label>
  </div>

  <footer class="dialog-foot">
    <span class="muted xsmall"><kbd>Ctrl</kbd>+<kbd>Enter</kbd> 儲存</span>
    <span class="spacer"></span>
    <button class="btn" onclick={close}><Icon name="x" />取消</button>
    <button class="btn btn--primary" onclick={save}><Icon name="check" />儲存</button>
  </footer>
</dialog>

<style>
  .dialog {
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    padding: 0;
    color: var(--c-text);
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-dialog);
    box-shadow: var(--shadow-dialog);
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

  @media (prefers-reduced-motion: reduce) {
    .dialog[open],
    .dialog[open].is-closing {
      animation: none;
    }
  }

  .dialog::backdrop {
    background: none;
  }

  .dialog-head,
  .dialog-foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  /* Claude Design 定案的對話框：標題列左側多留空間。 */
  .dialog-head {
    padding: 16px 16px 16px 24px;
    border-bottom: 1px solid var(--c-border);
  }

  .dialog-foot {
    padding: 14px 16px 14px 20px;
    border-top: 1px solid var(--c-border);
  }

  .dialog-body {
    min-height: 0;
    padding: var(--space-4) var(--space-4) var(--space-4) var(--space-5);
  }

  .title-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .memo {
    resize: vertical;
    min-height: 0;
    font-family: var(--font-sans);
  }
</style>
