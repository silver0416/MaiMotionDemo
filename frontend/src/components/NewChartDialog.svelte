<script lang="ts">
  import Icon from './Icon.svelte';
  import DiagnosticList from './DiagnosticList.svelte';
  import { STATUS_HINT, STATUS_LABEL, SUPPORT_NOTES } from '../lib/contract';
  import { reducedMotion } from '../lib/press';
  import { byteOffsetToIndex } from '../lib/text';
  import { records } from '../state/records.svelte';
  import { session } from '../state/session.svelte';
  import type { AnalyzeResponse, AnalyzeStatus, Diagnostic } from '../lib/types';

  interface Props {
    open: boolean;
  }

  let { open = $bindable() }: Props = $props();

  let dialog: HTMLDialogElement | null = $state(null);
  let textarea: HTMLTextAreaElement | null = $state(null);
  /** 取消時保留草稿，下次打開接著編輯；成功新增後才清空。 */
  let draft = $state('');
  /** 被退回（語法錯誤或未支援）的核心回應，以及當時送出的原文。 */
  let rejected = $state<{ response: AnalyzeResponse; source: string } | null>(null);

  const analyzing = $derived(session.phase === 'analyzing');
  const canGenerate = $derived(
    session.desktop && !analyzing && draft.trim().length > 0 && session.configIssues.length === 0,
  );
  const rejectedStatus = $derived<AnalyzeStatus | null>(
    (rejected?.response.status as AnalyzeStatus) ?? null,
  );
  /** 草稿改過之後，舊診斷的行列位置可能已經不準。 */
  const rejectedStale = $derived(rejected !== null && rejected.source !== draft);

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
        textarea?.focus();
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

  function usable(response: AnalyzeResponse): boolean {
    return response.chart !== null && response.status !== 'invalid' && response.status !== 'unsupported';
  }

  async function generate() {
    if (!canGenerate) return;
    const source = draft;
    const response = await session.analyze(source, usable);
    if (!response) return;
    if (!usable(response)) {
      rejected = { response, source };
      return;
    }
    records.add(source);
    draft = '';
    rejected = null;
    open = false;
  }

  function locate(diagnostic: Diagnostic) {
    const span = diagnostic.sourceSpan;
    if (!span || !textarea || !rejected) return;
    const start = byteOffsetToIndex(rejected.source, span.start);
    const end = byteOffsetToIndex(rejected.source, span.end);
    textarea.focus();
    textarea.setSelectionRange(start, Math.max(end, start + 1));
  }

  function onDialogKeydown(event: KeyboardEvent) {
    // 不依賴瀏覽器原生的 cancel 行為（WebView 之間不一致），Esc 一律直接關閉。
    if (event.key === 'Escape') {
      event.preventDefault();
      open = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void generate();
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="new-chart-title"
  onclose={() => (open = false)}
  oncancel={(event) => {
    event.preventDefault();
    open = false;
  }}
  onkeydown={onDialogKeydown}
  onpointerdown={(event) => (pressedOutside = isOutside(event))}
  onclick={(event) => {
    if (pressedOutside && isOutside(event)) open = false;
    pressedOutside = false;
  }}
>
  <header class="dialog-head">
    <h2 id="new-chart-title">新增譜面</h2>
    <span class="spacer"></span>
    <button class="btn btn--icon" onclick={() => (open = false)} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div class="dialog-body">
    <div class="editor">
      <label class="field-label" for="new-chart-source">simai 原文或 maidata.txt</label>
      <textarea
        id="new-chart-source"
        class="textarea source"
        spellcheck="false"
        bind:this={textarea}
        bind:value={draft}
        onkeydown={onKeydown}
        placeholder="(120)&#123;4&#125;1,2,3,4,E"
      ></textarea>

      {#if !session.desktop}
        <p class="field-error">瀏覽器預覽沒有 Rust 核心，無法生成。請改用桌面版。</p>
      {:else if session.configIssues.length > 0}
        <p class="field-error">「參數」分頁有超出範圍的數值，修正後才能生成。</p>
      {/if}

      {#if rejected && rejectedStatus}
        <section class="result" aria-live="polite">
          <div class="row row-wrap">
            <span class="badge badge--danger">{STATUS_LABEL[rejectedStatus] ?? rejectedStatus}</span>
            {#if rejectedStale}
              <span class="xsmall muted">原文已修改，位置可能不準；重新生成以更新。</span>
            {:else if STATUS_HINT[rejectedStatus]}
              <span class="xsmall muted">{STATUS_HINT[rejectedStatus]}</span>
            {/if}
          </div>
          {#if rejected.response.diagnostics.length > 0}
            <div class="result-list scroll">
              <DiagnosticList
                diagnostics={rejected.response.diagnostics}
                onLocate={rejectedStale ? undefined : locate}
              />
            </div>
          {/if}
        </section>
      {/if}
    </div>

    <aside class="support" aria-labelledby="support-title">
      <h3 id="support-title" class="support-title">支援語法</h3>
      <dl class="support-list">
        {#each SUPPORT_NOTES as item (item.title)}
          <dt>{item.title}</dt>
          <dd class="mono">{item.body}</dd>
        {/each}
      </dl>
    </aside>
  </div>

  <footer class="dialog-foot">
    <span class="muted xsmall mono">{draft.length} 字元</span>
    <span class="muted xsmall">Ctrl + Enter 生成</span>
    <span class="spacer"></span>
    <button class="btn" onclick={() => (open = false)}>
      <Icon name="x" />取消
    </button>
    <button class="btn btn--primary" onclick={generate} disabled={!canGenerate}>
      <Icon name={analyzing ? 'loader' : 'zap'} spin={analyzing} />
      {analyzing ? '分析中' : '生成並新增'}
    </button>
  </footer>
</dialog>

<style>
  .dialog {
    width: min(960px, calc(100vw - 32px));
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

  @media (prefers-reduced-motion: reduce) {
    .dialog[open],
    .dialog[open].is-closing {
      animation: none;
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

  .dialog-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 280px;
    min-height: 0;
    overflow: auto;
  }

  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    padding: var(--space-4);
  }

  .source {
    min-height: 260px;
    height: 40vh;
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-top: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--c-border);
  }

  .result-list {
    max-height: 200px;
  }

  .support {
    padding: var(--space-4);
    border-left: 1px solid var(--c-border);
    background: var(--c-bg);
  }

  .support-title {
    margin-bottom: var(--space-3);
    font-size: var(--fs-sm);
    color: var(--c-text-dim);
  }

  .support-list {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-2) var(--space-3);
    margin: 0;
    font-size: var(--fs-xs);
  }

  .support-list dt {
    color: var(--c-text-dim);
  }

  .support-list dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  @media (max-width: 719px) {
    .dialog-body {
      grid-template-columns: minmax(0, 1fr);
    }

    .support {
      border-left: none;
      border-top: 1px solid var(--c-border);
    }

    .source {
      height: 30vh;
      min-height: 160px;
    }
  }
</style>
