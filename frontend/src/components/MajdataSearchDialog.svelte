<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import DiagnosticList from './DiagnosticList.svelte';
  import CopyDebugButton from './CopyDebugButton.svelte';
  import { STATUS_HINT, STATUS_LABEL } from '../lib/contract';
  import { fetchMajdataChart, searchMajdataCharts } from '../lib/api';
  import { hashText } from '../lib/db';
  import { reducedMotion } from '../lib/press';
  import {
    maidataField,
    recordDate,
    recordLevels,
    recordSearchText,
    recordTitle,
    records,
    type ChartRecord,
    type MajdataOrigin,
  } from '../state/records.svelte';
  import { session } from '../state/session.svelte';
  import { toasts } from '../state/toasts.svelte';
  import type { AnalyzeResponse, AnalyzeStatus, MajdataChartSummary } from '../lib/types';

  interface Props {
    open: boolean;
  }

  let { open = $bindable() }: Props = $props();

  /** Majdata 的 levels 依 &inote_1… 排列。 */
  const LEVEL_NAMES = ['Easy', 'Basic', 'Advanced', 'Expert', 'Master', 'Re:Master', 'Original'];
  const LEVEL_SHORT = ['EAS', 'BAS', 'ADV', 'EXP', 'MAS', 'ReM', 'ORG'];
  /** 與 Rust 端的搜尋字數上限一致。 */
  const MAX_QUERY = 100;

  type SearchState =
    | { kind: 'idle' }
    | { kind: 'searching'; query: string }
    | { kind: 'error'; query: string; message: string }
    | { kind: 'done'; query: string; items: MajdataChartSummary[] };

  type Failure =
    | { kind: 'download'; chart: MajdataChartSummary; message: string }
    | { kind: 'analysis'; chart: MajdataChartSummary; message: string; source: string }
    | { kind: 'rejected'; chart: MajdataChartSummary; response: AnalyzeResponse; source: string };

  let dialog: HTMLDialogElement | null = $state(null);
  let input: HTMLInputElement | null = $state(null);
  let feedback: HTMLElement | null = $state(null);

  let query = $state('');
  let search = $state<SearchState>({ kind: 'idle' });
  /** 最後一次送出的關鍵字；本地紀錄即時依這個字篩選，刪改紀錄也會跟著更新。 */
  let submitted = $state('');
  /** 難度分類篩選：null 為全部，否則是 &inote_ 的索引（0 = Easy）。 */
  let levelFilter = $state<number | null>(null);
  /** 正在匯入的譜面與目前步驟。 */
  let importing = $state<{ id: string; step: 'download' | 'analyze' } | null>(null);
  let failure = $state<Failure | null>(null);

  /** 已下載的原文；分析失敗後重試不必再下載一次。對話框完全關閉時清空，下次打開一律重新下載。 */
  const downloaded = new Map<string, string>();
  /** 只採用最後一次搜尋的結果。 */
  let searchToken = 0;

  const searching = $derived(search.kind === 'searching');
  const analyzing = $derived(session.phase === 'analyzing');
  const busy = $derived(searching || importing !== null);
  // 本地搜尋不需要核心，瀏覽器預覽也能用；線上搜尋只在桌面版送出。
  const canSearch = $derived(!busy && query.trim().length > 0);
  const canImport = $derived(
    session.desktop && !busy && !analyzing && session.configIssues.length === 0,
  );
  const rejectedStatus = $derived<AnalyzeStatus | null>(
    failure?.kind === 'rejected' ? (failure.response.status as AnalyzeStatus) : null,
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
        dialog.showModal();
        input?.focus();
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

  /** 匯入沒有取消機制：進行中時忽略所有關閉入口，避免對話框消失後結果才在背景套用。 */
  function requestClose() {
    if (importing !== null) return;
    open = false;
  }

  function onDialogClosed() {
    open = false;
    if (importing === null) downloaded.clear();
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

  function messageOf(error: unknown): string {
    if (typeof error === 'string' && error.trim().length > 0) return error;
    if (error instanceof Error && error.message) return error.message;
    return '發生未知錯誤，請稍後再試。';
  }

  /** 按鈕在工作中會停用，焦點若因此掉出對話框就拉回來。 */
  async function keepFocus(target: () => HTMLElement | null) {
    await tick();
    if (open && dialog && !dialog.contains(document.activeElement)) target()?.focus();
  }

  /** 錯誤區塊在 failure 設值後才掛載，必須等 tick 之後再取元素。 */
  function focusFeedback() {
    void keepFocus(() => feedback);
  }

  function hasLevel(levels: (string | null)[], index: number): boolean {
    return (levels[index]?.trim() ?? '').length > 0;
  }

  const localMatches = $derived.by<ChartRecord[]>(() => {
    const words = submitted.toLowerCase().split(/\s+/).filter((word) => word.length > 0);
    if (words.length === 0) return [];
    return records.items.filter((record) => {
      if (levelFilter !== null && !hasLevel(recordLevels(record), levelFilter)) return false;
      const text = recordSearchText(record);
      return words.every((word) => text.includes(word));
    });
  });

  const onlineItems = $derived.by<MajdataChartSummary[]>(() => {
    if (search.kind !== 'done') return [];
    const filter = levelFilter;
    return filter === null ? search.items : search.items.filter((chart) => hasLevel(chart.levels, filter));
  });

  async function runSearch(event?: SubmitEvent) {
    event?.preventDefault();
    const text = query.trim();
    if (!canSearch || text.length === 0) return;
    submitted = text;
    failure = null;
    if (!session.desktop) {
      void keepFocus(() => input);
      return;
    }
    const token = ++searchToken;
    search = { kind: 'searching', query: text };
    failure = null;
    try {
      const items = await searchMajdataCharts(text);
      if (token !== searchToken) return;
      search = { kind: 'done', query: text, items };
    } catch (error) {
      if (token !== searchToken) return;
      search = { kind: 'error', query: text, message: messageOf(error) };
    }
    void keepFocus(() => input);
  }

  function originOf(chart: MajdataChartSummary): MajdataOrigin {
    return {
      id: chart.id,
      title: chart.title,
      artist: chart.artist,
      designer: chart.designer,
      uploader: chart.uploader,
      levels: [...chart.levels],
    };
  }

  /** 關閉對話框並開啟本地紀錄；分析過就直接從資料庫載入。 */
  async function openLocal(record: ChartRecord, reason?: string) {
    if (analyzing || importing !== null) return;
    failure = null;
    open = false;
    records.activeId = record.id;
    if (reason) {
      toasts.show({ id: 'majdata-local', tone: 'info', title: '已在本地', body: reason });
    }
    await session.load(record.source);
  }

  async function importChart(chart: MajdataChartSummary) {
    if (!canImport) return;
    failure = null;
    // Majdata 的 song id 每次上傳都不同：同 id 必定是同一份，不必再下載。
    const known = records.findByMajdataId(chart.id);
    if (known) {
      await openLocal(known, `「${recordTitle(known)}」已經匯入過，直接開啟本地紀錄。`);
      return;
    }
    let source = downloaded.get(chart.id);
    if (source === undefined) {
      importing = { id: chart.id, step: 'download' };
      try {
        source = await fetchMajdataChart(chart.id);
        downloaded.set(chart.id, source);
      } catch (error) {
        importing = null;
        failure = { kind: 'download', chart, message: messageOf(error) };
        focusFeedback();
        return;
      }
    }
    // 同名不同作者的譜面原文不會相同；只有原文逐字一致才視為同一張。
    const same = records.findByHash(await hashText(source));
    if (same) {
      importing = null;
      if (!same.majdata) records.attachMajdata(same.id, originOf(chart));
      await openLocal(same, `本地已有內容完全相同的譜面「${recordTitle(same)}」，直接開啟。`);
      return;
    }
    importing = { id: chart.id, step: 'analyze' };
    const response = await session.analyze(source, usable);
    importing = null;
    if (!response) {
      failure = {
        kind: 'analysis',
        chart,
        source,
        message: session.errorMessage ?? '分析沒有完成，盤面維持原狀。',
      };
      focusFeedback();
      return;
    }
    if (!usable(response)) {
      failure = { kind: 'rejected', chart, response, source };
      focusFeedback();
      return;
    }
    await records.add(source, { majdata: originOf(chart) });
    failure = null;
    open = false;
  }

  function levelsOf(source: (string | null)[]): { index: number; value: string }[] {
    return source
      .map((value, index) => ({ index, value: value?.trim() ?? '' }))
      .filter((item) => item.value.length > 0);
  }

  function tagsOf(chart: MajdataChartSummary): string[] {
    return [...new Set([...chart.tags, ...chart.publicTags].map((tag) => tag.trim()))].filter(
      (tag) => tag.length > 0,
    );
  }

  function formatDate(timestamp: string): string {
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return '';
    const pad = (value: number) => String(value).padStart(2, '0');
    return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())}`;
  }

  function orDash(value: string): string {
    return value.trim().length > 0 ? value : '—';
  }

  function onDialogKeydown(event: KeyboardEvent) {
    // 不依賴瀏覽器原生的 cancel 行為（WebView 之間不一致），Esc 直接關閉（匯入中忽略）。
    if (event.key === 'Escape') {
      event.preventDefault();
      requestClose();
    }
  }
</script>

{#snippet levelList(levels: { index: number; value: string }[])}
  <ul class="levels" aria-label="難度">
    {#each levels as level (level.index)}
      <li
        class="badge badge--quiet mono"
        class:is-match={levelFilter === level.index}
        title={LEVEL_NAMES[level.index] ?? `&inote_${level.index + 1}`}
      >
        {LEVEL_SHORT[level.index] ?? `#${level.index + 1}`}
        {level.value}
      </li>
    {/each}
  </ul>
{/snippet}

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="majdata-title"
  aria-busy={busy}
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
    <h2 id="majdata-title">搜尋譜面</h2>
    <span class="spacer"></span>
    <button
      class="btn btn--icon"
      onclick={requestClose}
      disabled={importing !== null}
      aria-label="關閉"
    >
      <Icon name="x" />
    </button>
  </header>

  <form class="search-bar" role="search" onsubmit={runSearch}>
    <label class="sr-only" for="majdata-query">搜尋歌曲名稱、作者、譜師或紀錄名稱</label>
    <input
      id="majdata-query"
      class="input"
      type="search"
      autocomplete="off"
      spellcheck="false"
      maxlength={MAX_QUERY}
      placeholder="歌曲名稱、作者、譜師或紀錄名稱"
      bind:this={input}
      bind:value={query}
    />
    <button class="btn btn--primary" type="submit" disabled={!canSearch}>
      <Icon name={searching ? 'loader' : 'search'} spin={searching} />
      {searching ? '搜尋中' : '搜尋'}
    </button>
  </form>

  <div class="filter-bar" role="group" aria-labelledby="majdata-level-label">
    <span id="majdata-level-label" class="xsmall muted">難度</span>
    <button
      class="btn filter"
      class:is-active={levelFilter === null}
      aria-pressed={levelFilter === null}
      onclick={() => (levelFilter = null)}>全部</button
    >
    {#each LEVEL_SHORT as short, index (short)}
      <button
        class="btn filter mono"
        class:is-active={levelFilter === index}
        aria-pressed={levelFilter === index}
        title={`只顯示有 ${LEVEL_NAMES[index]} 的譜面`}
        onclick={() => (levelFilter = levelFilter === index ? null : index)}>{short}</button
      >
    {/each}
  </div>

  {#if !session.desktop}
    <p class="notice field-error">瀏覽器預覽沒有 Rust 核心，只能搜尋本地紀錄，無法搜尋 Majdata 或匯入。</p>
  {:else if session.configIssues.length > 0}
    <p class="notice field-error">「參數」分頁有超出範圍的數值，修正後才能匯入。</p>
  {/if}

  {#if failure}
    <section class="feedback" tabindex="-1" bind:this={feedback} aria-labelledby="majdata-failure">
      <div class="row row-wrap">
        {#if failure.kind === 'rejected' && rejectedStatus}
          <span class="badge badge--danger">{STATUS_LABEL[rejectedStatus] ?? rejectedStatus}</span>
        {:else}
          <span class="badge badge--danger">
            {failure.kind === 'download' ? '下載失敗' : '分析未完成'}
          </span>
        {/if}
        <span id="majdata-failure" class="small feedback-title">
          「{orDash(failure.chart.title)}」沒有匯入
        </span>
        <span class="spacer"></span>
        <button class="btn btn--icon" onclick={() => (failure = null)} aria-label="關閉匯入錯誤">
          <Icon name="x" size={14} />
        </button>
      </div>
      {#if failure.kind === 'rejected'}
        <p class="xsmall muted">
          {rejectedStatus && STATUS_HINT[rejectedStatus] ? `${STATUS_HINT[rejectedStatus]} ` : ''}盤面與譜面紀錄維持原狀，可以改選別筆。
        </p>
        {#if failure.response.diagnostics.length > 0}
          <div class="feedback-list scroll">
            <DiagnosticList diagnostics={failure.response.diagnostics} />
          </div>
        {/if}
      {:else}
        <p class="small">{failure.message}</p>
        <p class="xsmall muted">盤面與譜面紀錄維持原狀，可以重試或改選別筆。</p>
      {/if}
      {#if failure.kind !== 'download'}
        {@const current = failure}
        <div class="row row-wrap">
          <CopyDebugButton
            input={() => ({
              context: 'Majdata 匯入',
              source: current.source,
              config:
                current.kind === 'analysis'
                  ? (session.lastFailure?.request.solverConfig ?? session.requestConfig)
                  : session.requestConfig,
              firstSeconds: session.firstSeconds,
              requestId:
                current.kind === 'analysis' ? (session.lastFailure?.request.requestId ?? null) : null,
              response: current.kind === 'rejected' ? current.response : null,
              error: current.kind === 'analysis' ? current.message : null,
              extra: {
                'Majdata song id': current.chart.id,
                'Majdata 標題': current.chart.title,
                'Majdata 譜師': current.chart.designer,
                'Majdata 上傳者': current.chart.uploader,
              },
            })}
          />
        </div>
      {/if}
    </section>
  {/if}

  <div class="results scroll" aria-live="polite">
    {#if submitted.length === 0}
      <p class="state small muted">
        輸入關鍵字後按 Enter，會同時搜尋本地譜面紀錄與 Majdata。匯入會下載 maidata.txt，交給核心分析成功後才加入譜面紀錄；有多個難度時分析編號最大的那一個。已經匯入過的譜面不會重複下載，會直接開啟本地紀錄。
      </p>
    {:else}
      <section class="group" aria-labelledby="local-group-title">
        <h3 id="local-group-title" class="group-title">
          <span>本地紀錄</span>
          <span class="mono">{localMatches.length}</span>
        </h3>
        {#if localMatches.length === 0}
          <p class="group-empty small muted">
            沒有符合「{submitted}」{levelFilter !== null ? `且有 ${LEVEL_NAMES[levelFilter]} 難度` : ''}的本地紀錄。
          </p>
        {:else}
          <ul class="list">
            {#each localMatches as record (record.id)}
              {@const levels = levelsOf(recordLevels(record))}
              {@const artist = record.majdata?.artist || maidataField(record.source, 'artist')}
              {@const designer = record.majdata?.designer || maidataField(record.source, 'des')}
              <li class="item" class:is-current={records.activeId === record.id}>
                <div class="info">
                  <span class="title">{recordTitle(record)}</span>
                  {#if artist}
                    <span class="small muted artist">{artist}</span>
                  {/if}
                  <dl class="meta xsmall">
                    {#if designer}
                      <dt>譜師</dt>
                      <dd>{designer}</dd>
                    {/if}
                    <dt>新增</dt>
                    <dd class="mono">{recordDate(record)}</dd>
                  </dl>
                  {#if levels.length > 0}
                    {@render levelList(levels)}
                  {/if}
                </div>
                <button
                  class="btn"
                  onclick={() => openLocal(record)}
                  disabled={analyzing || importing !== null || !session.desktop}
                  aria-label={`開啟本地紀錄 ${recordTitle(record)}`}
                >
                  <Icon name="external-link" />開啟
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      {#if session.desktop}
        <section class="group" aria-labelledby="online-group-title">
          <h3 id="online-group-title" class="group-title">
            <span>Majdata</span>
            {#if search.kind === 'done'}
              <span class="mono">
                {levelFilter === null ? search.items.length : `${onlineItems.length}／${search.items.length}`}
              </span>
            {/if}
          </h3>
          {#if search.kind === 'searching'}
            <p class="group-empty small muted row">
              <Icon name="loader" spin />正在搜尋「{search.query}」
            </p>
          {:else if search.kind === 'error'}
            <div class="group-empty stack-sm">
              <p class="small row"><Icon name="circle-alert" />搜尋「{search.query}」失敗</p>
              <p class="small muted">{search.message}</p>
              <div>
                <button class="btn" onclick={() => runSearch()} disabled={!canSearch}>
                  <Icon name="refresh-cw" />重試
                </button>
              </div>
            </div>
          {:else if search.kind === 'done' && search.items.length === 0}
            <p class="group-empty small muted">Majdata 找不到符合「{search.query}」的譜面，換個關鍵字試試。</p>
          {:else if search.kind === 'done' && onlineItems.length === 0}
            <p class="group-empty small muted">
              「{search.query}」的 {search.items.length} 筆結果都沒有 {levelFilter !== null
                ? LEVEL_NAMES[levelFilter]
                : ''} 難度。
            </p>
          {:else if search.kind === 'done'}
            <ul class="list">
              {#each onlineItems as chart (chart.id)}
                {@const levels = levelsOf(chart.levels)}
                {@const tags = tagsOf(chart)}
                {@const date = formatDate(chart.timestamp)}
                {@const current = importing?.id === chart.id}
                {@const local = records.findByMajdataId(chart.id)}
                <li class="item" class:is-current={current || failure?.chart.id === chart.id}>
                  <div class="info">
                    <span class="title">
                      {orDash(chart.title)}
                      {#if local}
                        <span class="badge badge--quiet local-badge">已在本地</span>
                      {/if}
                    </span>
                    <span class="small muted artist">{orDash(chart.artist)}</span>
                    <dl class="meta xsmall">
                      <dt>譜師</dt>
                      <dd>{orDash(chart.designer)}</dd>
                      <dt>上傳</dt>
                      <dd>{orDash(chart.uploader)}{date ? `・${date}` : ''}</dd>
                    </dl>
                    {#if levels.length > 0}
                      {@render levelList(levels)}
                    {/if}
                    {#if tags.length > 0}
                      <span class="xsmall muted tags">{tags.join('・')}</span>
                    {/if}
                  </div>
                  {#if local}
                    <button
                      class="btn"
                      onclick={() => importChart(chart)}
                      disabled={!canImport}
                      aria-label={`開啟已匯入的 ${orDash(chart.title)}`}
                    >
                      <Icon name="external-link" />開啟
                    </button>
                  {:else}
                    <button
                      class="btn"
                      onclick={() => importChart(chart)}
                      disabled={!canImport}
                      aria-label={`匯入 ${orDash(chart.title)}`}
                    >
                      {#if current}
                        <Icon name="loader" spin />{importing?.step === 'download' ? '下載中' : '分析中'}
                      {:else}
                        <Icon name="download" />匯入
                      {/if}
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/if}
    {/if}
  </div>

  <footer class="dialog-foot">
    <span class="muted xsmall">Enter 搜尋・Esc 關閉</span>
    <span class="spacer"></span>
    <button class="btn" onclick={requestClose} disabled={importing !== null}>
      <Icon name="x" />關閉
    </button>
  </footer>
</dialog>

<style>
  .dialog {
    width: min(760px, calc(100vw - 32px));
    height: min(680px, calc(100vh - 32px));
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

  .search-bar {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-4) var(--space-3);
  }

  .search-bar .input {
    min-height: 30px;
  }

  .notice {
    padding: 0 var(--space-4) var(--space-3);
  }

  /* 匯入失敗固定在清單上方，捲動結果時不會被捲走。 */
  .feedback {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: 0 var(--space-4) var(--space-3);
  }

  .feedback-title {
    font-weight: 600;
  }

  .feedback .btn--icon {
    min-height: 26px;
    min-width: 26px;
    padding: 0;
  }

  .feedback-list {
    max-height: 160px;
  }

  .results {
    flex: 1 1 auto;
    min-height: 0;
    border-top: 1px solid var(--c-border);
  }

  .state {
    padding: var(--space-4);
    line-height: var(--lh-normal);
  }

  .filter-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-4) var(--space-3);
  }

  .filter-bar > span {
    margin-right: var(--space-1);
  }

  .filter {
    min-height: 26px;
    padding: 0 var(--space-2);
    font-size: var(--fs-xs);
  }

  .group + .group {
    border-top: 1px solid var(--c-border);
  }

  .group-title {
    display: flex;
    justify-content: space-between;
    margin: 0;
    padding: var(--space-3) var(--space-4) 0;
    font-size: var(--fs-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--c-text-dim);
  }

  .group-empty {
    padding: var(--space-2) var(--space-4) var(--space-4);
  }

  .local-badge {
    margin-left: var(--space-1);
    font-size: var(--fs-xs);
    font-weight: 400;
    vertical-align: middle;
  }

  .levels .badge.is-match {
    color: var(--c-text-strong);
    border-color: var(--c-text);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: var(--space-2) var(--space-2) var(--space-3);
  }

  .item {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-2);
    border-bottom: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
  }

  .item:last-child {
    border-bottom: none;
  }

  .item.is-current {
    background: var(--c-control);
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 auto;
    min-width: 0;
  }

  .title {
    font-size: var(--fs-md);
    font-weight: 600;
    color: var(--c-text-strong);
    overflow-wrap: anywhere;
  }

  .artist {
    overflow-wrap: anywhere;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0 var(--space-2);
    margin: var(--space-1) 0 0;
  }

  .meta dt {
    color: var(--c-text-dim);
  }

  .meta dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .levels {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin: var(--space-2) 0 0;
    padding: 0;
    list-style: none;
  }

  .levels .badge {
    font-weight: 400;
  }

  .tags {
    margin-top: var(--space-1);
    overflow-wrap: anywhere;
  }
</style>
