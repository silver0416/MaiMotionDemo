<script lang="ts">
  import Icon from './Icon.svelte';
  import DiagnosticList from './DiagnosticList.svelte';
  import CopyDebugButton from './CopyDebugButton.svelte';
  import { parseFile } from '../lib/annotation';
  import { STATUS_HINT, STATUS_LABEL, SUPPORT_NOTES } from '../lib/contract';
  import { reducedMotion } from '../lib/press';
  import { byteOffsetToIndex } from '../lib/text';
  import {
    WIKI_TYPE_LABEL,
    recordDate,
    records,
    sourceHeading,
    wikiDifficultyLabel,
    type ChartRecord,
  } from '../state/records.svelte';
  import { annotation, checkSource } from '../state/annotation.svelte';
  import { errorLog } from '../state/errorLog.svelte';
  import { session } from '../state/session.svelte';
  import { toasts } from '../state/toasts.svelte';
  import type { AnalyzeResponse, AnalyzeStatus, Diagnostic } from '../lib/types';
  import type { AnalyzeFailure } from '../state/session.svelte';

  interface Props {
    open: boolean;
    /** 指定時以編輯模式打開這筆紀錄；關閉後自動歸零。 */
    editing?: ChartRecord | null;
    /** 匯入了真人手順標註檔（切到標註分頁用）。 */
    onAnnotationImported?: () => void;
  }

  let { open = $bindable(), editing = $bindable(null), onAnnotationImported }: Props = $props();

  /** 打開當下決定的模式；關閉動畫期間維持不變，畫面不會中途換標題。 */
  let mode = $state<'new' | 'edit'>('new');
  let editId = $state<string | null>(null);
  /** 進入編輯前的新增草稿，離開編輯時放回去。 */
  let stashedDraft = '';
  let stashedName = '';

  let dialog: HTMLDialogElement | null = $state(null);
  let textarea: HTMLTextAreaElement | null = $state(null);
  /** 取消時保留草稿，下次打開接著編輯；成功新增後才清空。 */
  let draft = $state('');
  /** 自訂名稱；留空時紀錄標題用譜面開頭。 */
  let name = $state('');
  /** 被退回（語法錯誤或未支援）的核心回應，以及當時送出的原文。 */
  let rejected = $state<{ response: AnalyzeResponse; source: string } | null>(null);
  /** 呼叫核心本身失敗（不是譜面問題）。 */
  let failed = $state<AnalyzeFailure | null>(null);

  const analyzing = $derived(session.phase === 'analyzing');
  /** 貼上的是真人手順標註檔（JSON）：新增時改用檔案附的原譜，並匯入標註。 */
  const annotationFile = $derived(draft.trimStart().startsWith('{') ? parseFile(draft) : null);
  const importing = $derived(annotationFile?.ok ? annotationFile : null);
  /** 匯入時原譜被改過之類的錯誤；改草稿就清掉。 */
  let importError = $state<{ text: string; source: string } | null>(null);
  const draftHeading = $derived(
    importing ? importing.file.title || sourceHeading(importing.file.chart.source) : draft.trim().length > 0 ? sourceHeading(draft) : '',
  );
  const canAnalyze = $derived(
    session.desktop &&
      !analyzing &&
      draft.trim().length > 0 &&
      session.configIssues.length === 0 &&
      (annotationFile === null || (mode === 'new' && importing !== null)),
  );
  /** 編輯中的紀錄；對話框開著時被刪掉會變成 null。 */
  const record = $derived(mode === 'edit' ? records.get(editId) : null);
  const sourceChanged = $derived(record !== null && draft !== record.source);
  const nameChanged = $derived(record !== null && name.trim() !== (record.name?.trim() ?? ''));
  const dirty = $derived(sourceChanged || nameChanged);
  /** 只改名稱不必重跑核心；原文改了才要分析，而且要分析得過才存。 */
  const canSave = $derived(
    record !== null && dirty && !analyzing && (!sourceChanged || canAnalyze),
  );
  const canGenerate = $derived(mode === 'edit' ? canSave : canAnalyze);
  const rejectedStatus = $derived<AnalyzeStatus | null>(
    (rejected?.response.status as AnalyzeStatus) ?? null,
  );
  /** 草稿改過之後，舊診斷的行列位置可能已經不準。 */
  const rejectedStale = $derived(
    rejected !== null && rejected.source !== (importing?.file.chart.source ?? draft),
  );

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
        enterMode();
        dialog.showModal();
        textarea?.focus();
        if (mode === 'edit' && textarea) {
          textarea.setSelectionRange(0, 0);
          textarea.scrollTop = 0;
        }
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

  function enterMode() {
    rejected = null;
    failed = null;
    if (editing) {
      if (mode === 'new') {
        stashedDraft = draft;
        stashedName = name;
      }
      mode = 'edit';
      editId = editing.id;
      draft = editing.source;
      name = editing.name ?? '';
    } else if (mode === 'edit') {
      leaveEdit();
    }
  }

  /** 對話框真正關上後：編輯模式放棄未儲存的修改，放回新增草稿。 */
  function leaveEdit() {
    if (mode !== 'edit') return;
    mode = 'new';
    editId = null;
    editing = null;
    draft = stashedDraft;
    name = stashedName;
    rejected = null;
    failed = null;
  }

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

  function logFailure(source: string, response: AnalyzeResponse | null, failure: AnalyzeFailure | null) {
    const origin = mode === 'edit' ? '編輯譜面' : '新增譜面';
    const status = response?.status as AnalyzeStatus | undefined;
    void errorLog.record({
      kind: response ? 'rejected' : 'analysis',
      origin,
      title: name.trim() || sourceHeading(source),
      summary: response
        ? `${(status && STATUS_LABEL[status]) ?? response.status}：${response.diagnostics[0]?.message ?? '沒有診斷'}`
        : (failure?.message ?? '呼叫核心失敗'),
      debug: $state.snapshot({
        context: origin,
        source,
        config: failure?.request.solverConfig ?? session.requestConfig,
        firstSeconds: failure?.request.firstSeconds ?? session.firstSeconds,
        requestId: failure?.request.requestId ?? null,
        response,
        error: failure?.message ?? null,
      }),
    });
  }

  /** 送核心分析；不可用時留下診斷並回傳 false。 */
  async function analyzeDraft(source: string): Promise<boolean> {
    failed = null;
    const response = await session.analyze(source, usable);
    if (!response) {
      if (session.lastFailure?.request.source === source) {
        failed = session.lastFailure;
        rejected = null;
        logFailure(source, null, failed);
      }
      return false;
    }
    if (!usable(response)) {
      rejected = { response, source };
      logFailure(source, response, null);
      return false;
    }
    return true;
  }

  async function save() {
    const target = record;
    if (!canSave || !target) return;
    const source = draft;
    const changed = sourceChanged;
    if (changed && !(await analyzeDraft(source))) return;
    const saved = await records.update(target.id, { name, source });
    // 分析結果已經套到盤面，紀錄清單跟著指到這一筆。
    if (saved && changed) records.activeId = saved.id;
    rejected = null;
    failed = null;
    open = false;
  }

  /** 標註檔：先分析檔案附的原譜（失敗時照常顯示診斷），再新增或切到同一份原譜的紀錄並合併標註。 */
  async function importAnnotation() {
    if (!importing) return;
    const { file, skipped } = importing;
    const text = draft;
    importError = null;
    const tampered = await checkSource(file);
    if (tampered) {
      importError = { text: tampered, source: text };
      return;
    }
    if (!(await analyzeDraft(file.chart.source))) return;
    const result = await annotation.importFile(file, { mode: 'fill', name, skipped });
    if (!result.ok) {
      importError = { text: result.text, source: text };
      return;
    }
    toasts.show({ id: 'annotation-import', tone: 'ok', title: '已匯入標註檔', body: result.text });
    draft = '';
    name = '';
    rejected = null;
    failed = null;
    open = false;
    onAnnotationImported?.();
  }

  async function generate() {
    if (mode === 'edit') {
      await save();
      return;
    }
    if (!canGenerate) return;
    if (importing) {
      await importAnnotation();
      return;
    }
    const source = draft;
    if (!(await analyzeDraft(source))) return;
    await records.add(source, { name });
    draft = '';
    name = '';
    rejected = null;
    failed = null;
    open = false;
  }

  function locate(diagnostic: Diagnostic) {
    const span = diagnostic.sourceSpan;
    // 標註檔的原譜不在輸入框裡，沒有位置可以選。
    if (!span || !textarea || !rejected || rejected.source !== draft) return;
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
  onclose={() => {
    open = false;
    leaveEdit();
  }}
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
    <h2 id="new-chart-title">{mode === 'edit' ? '編輯譜面' : '新增譜面'}</h2>
    {#if mode === 'edit' && dirty}
      <span class="badge badge--quiet">未儲存</span>
    {/if}
    <span class="spacer"></span>
    <button class="btn btn--icon" onclick={() => (open = false)} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div class="dialog-body">
    <div class="editor">
      {#if mode === 'edit'}
        {#if record}
          <dl class="meta">
            <dt>新增時間</dt>
            <dd class="mono">{recordDate(record)}</dd>
            <dt>來源</dt>
            {#if record.majdata}
              <dd>Majdata・{record.majdata.title} <span class="muted">{record.majdata.designer}</span></dd>
            {:else if record.wiki}
              <dd>
                simai Wiki・{record.wiki.title}
                <span class="muted">
                  {WIKI_TYPE_LABEL[record.wiki.chartType] ?? record.wiki.chartType}
                  {wikiDifficultyLabel(record.wiki.difficulty)}
                </span>
              </dd>
            {:else}
              <dd>手動貼上</dd>
            {/if}
          </dl>
          {#if sourceChanged && (record.majdata || record.wiki)}
            <p class="field-hint">
              來源資訊會保留，但原文已和來源不同；之後從搜尋匯入同一張會直接開啟這筆修改過的紀錄。
            </p>
          {/if}
        {:else}
          <p class="field-error">這筆紀錄已被刪除，修改無法儲存。</p>
        {/if}
      {/if}
      <label class="field-label" for="new-chart-name">名稱（選填）</label>
      <input
        id="new-chart-name"
        class="input"
        type="text"
        autocomplete="off"
        maxlength="120"
        bind:value={name}
        onkeydown={onKeydown}
        placeholder={draftHeading || '留空時使用譜面開頭'}
      />
      <label class="field-label name-gap" for="new-chart-source">simai 原文、maidata.txt 或標註檔</label>
      <textarea
        id="new-chart-source"
        class="textarea source"
        spellcheck="false"
        bind:this={textarea}
        bind:value={draft}
        onkeydown={onKeydown}
        placeholder="(120)&#123;4&#125;1,2,3,4,E"
      ></textarea>
      {#if annotationFile}
        {#if !annotationFile.ok}
          <p class="field-error">{annotationFile.error}</p>
        {:else if mode === 'edit'}
          <p class="field-error">這是真人手順標註檔，不能當成原文儲存。請關閉後用「新增譜面」匯入。</p>
        {:else}
          <p class="field-hint">
            偵測到真人手順標註檔「{annotationFile.file.title || '未命名'}」：{annotationFile.file.notes.length} 顆標註{annotationFile
              .file.video
              ? '，含對照影片'
              : ''}。會用檔案附的原譜新增紀錄並匯入標註；已經有同一份原譜的紀錄時直接合併進去，你已標的保留不動。
          </p>
        {/if}
        {#if importError && importError.source === draft}
          <p class="field-error">{importError.text}</p>
        {/if}
      {/if}

      {#if !session.desktop}
        <p class="field-error">
          {mode === 'edit'
            ? '瀏覽器預覽沒有 Rust 核心：可以查看原文與修改名稱，但改過的原文無法儲存。'
            : '瀏覽器預覽沒有 Rust 核心，無法生成。請改用桌面版。'}
        </p>
      {:else if session.configIssues.length > 0}
        <p class="field-error">「參數」分頁有超出範圍的數值，修正後才能生成。</p>
      {/if}
      {#if mode === 'edit' && record && session.desktop}
        <p class="field-hint">只改名稱會直接儲存；原文改了要先通過核心分析才會儲存，失敗時紀錄維持原狀。</p>
      {/if}

      {#if failed}
        <section class="result" aria-live="polite">
          <div class="row row-wrap">
            <span class="badge badge--danger">分析未完成</span>
            <span class="xsmall muted">呼叫核心失敗，不是譜面本身的問題。</span>
          </div>
          <p class="small">{failed.message}</p>
          <div class="row row-wrap">
            <CopyDebugButton
              input={() => ({
                context: `${mode === 'edit' ? '編輯譜面' : '新增譜面'}（呼叫核心失敗）`,
                source: failed?.request.source ?? draft,
                config: failed?.request.solverConfig ?? null,
                firstSeconds: failed?.request.firstSeconds ?? null,
                requestId: failed?.request.requestId ?? null,
                error: failed?.message ?? null,
              })}
            />
          </div>
        </section>
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
          <div class="row row-wrap">
            <CopyDebugButton
              input={() => ({
                context: mode === 'edit' ? '編輯譜面' : '新增譜面',
                source: rejected?.source ?? draft,
                config: session.requestConfig,
                firstSeconds: session.firstSeconds,
                response: rejected?.response ?? null,
              })}
            />
          </div>
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
    <span class="muted xsmall">Ctrl + Enter {mode === 'edit' ? '儲存' : '生成'}</span>
    <span class="spacer"></span>
    <button class="btn" onclick={() => (open = false)}>
      <Icon name="x" />{mode === 'edit' && !dirty ? '關閉' : '取消'}
    </button>
    <button class="btn btn--primary" onclick={generate} disabled={!canGenerate}>
      {#if mode === 'edit'}
        <Icon name={analyzing ? 'loader' : sourceChanged ? 'zap' : 'check'} spin={analyzing} />
        {analyzing ? '分析中' : sourceChanged ? '重新分析並儲存' : '儲存'}
      {:else}
        <Icon name={analyzing ? 'loader' : importing ? 'file-text' : 'zap'} spin={analyzing} />
        {analyzing ? '分析中' : importing ? '新增並匯入標註' : '生成並新增'}
      {/if}
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

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-3);
    margin: 0 0 var(--space-2);
    font-size: var(--fs-sm);
  }

  .meta dt {
    color: var(--c-text-dim);
  }

  .meta dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .name-gap {
    margin-top: var(--space-2);
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
