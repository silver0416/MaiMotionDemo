<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import DiagnosticList from './DiagnosticList.svelte';
  import CopyDebugButton from './CopyDebugButton.svelte';
  import { STATUS_HINT, STATUS_LABEL } from '../lib/contract';
  import { fetchWikiChart, openExternalUrl, refreshWikiIndex, searchWikiSongs } from '../lib/api';
  import { hashText } from '../lib/db';
  import {
    WIKI_DIFFICULTIES,
    WIKI_TYPE_LABEL,
    recordDate,
    recordSearchText,
    recordTitle,
    records,
    type ChartRecord,
    type WikiDifficultyKey,
    type WikiOrigin,
  } from '../state/records.svelte';
  import { session } from '../state/session.svelte';
  import { toasts } from '../state/toasts.svelte';
  import type {
    AnalyzeResponse,
    AnalyzeStatus,
    WikiChartPayload,
    WikiChartType,
    WikiSong,
  } from '../lib/types';

  interface Props {
    /** 對話框開著且目前切到這個分頁。 */
    active: boolean;
    /** 與 Majdata 分頁共用的關鍵字。 */
    query: string;
    /** 匯入進行中；外層據此鎖住關閉與切換。 */
    locked: boolean;
    onClose: () => void;
  }

  let {
    active,
    query = $bindable(),
    locked = $bindable(false),
    onClose,
  }: Props = $props();

  type TypeFilter = 'all' | WikiChartType;
  type Difficulty = (typeof WIKI_DIFFICULTIES)[number];

  const TYPE_FILTERS: { id: TypeFilter; label: string }[] = [
    { id: 'all', label: '全部' },
    { id: 'standard', label: 'Standard' },
    { id: 'deluxe', label: 'DX' },
  ];
  const DIFFICULTY_SHORT: Record<WikiDifficultyKey, string> = {
    easy: 'EAS',
    basic: 'BAS',
    advanced: 'ADV',
    expert: 'EXP',
    master: 'MAS',
    reMaster: 'ReM',
  };
  /** 與 Rust 端的搜尋字數上限一致。 */
  const MAX_QUERY = 100;
  /** 太短的關鍵字可能命中上千首，清單只畫前面這些。 */
  const MAX_SHOWN = 200;

  type IndexInfo = { count: number; fetchedAt: number; fromCache: boolean; stale: boolean };

  type SearchState =
    | { kind: 'idle' }
    /** 送出時索引還在載入，載入完成後自動搜尋一次。 */
    | { kind: 'waiting'; query: string; chartType: TypeFilter }
    | { kind: 'searching'; query: string; chartType: TypeFilter }
    | { kind: 'error'; query: string; chartType: TypeFilter; message: string }
    | { kind: 'done'; query: string; chartType: TypeFilter; items: WikiSong[] };

  type Failure =
    | { kind: 'download'; song: WikiSong; difficulty: Difficulty; message: string }
    | {
        kind: 'analysis';
        song: WikiSong;
        difficulty: Difficulty;
        message: string;
        payload: WikiChartPayload;
      }
    | {
        kind: 'rejected';
        song: WikiSong;
        difficulty: Difficulty;
        response: AnalyzeResponse;
        payload: WikiChartPayload;
      };

  let root: HTMLElement | null = $state(null);
  let input: HTMLInputElement | null = $state(null);
  let feedback: HTMLElement | null = $state(null);

  let typeFilter = $state<TypeFilter>('all');
  let search = $state<SearchState>({ kind: 'idle' });
  /** 最後一次送出的關鍵字；本機紀錄即時依這個字篩選。 */
  let submitted = $state('');

  let index = $state<IndexInfo | null>(null);
  let indexLoading = $state(false);
  let indexError = $state<string | null>(null);

  /** 右側預覽的歌曲：pageId 與種類一起才唯一（Standard／DX 同名是兩筆）。 */
  let selectedKey = $state<string | null>(null);
  let importing = $state<{ key: string; difficulty: Difficulty; step: 'download' | 'analyze' } | null>(
    null,
  );
  let failure = $state<Failure | null>(null);

  /** 已取得的文字譜；分析失敗後重試不必再抓一次。對話框完全關閉時清空。 */
  const downloaded = new Map<string, WikiChartPayload>();
  let searchToken = 0;
  let indexToken = 0;

  const searching = $derived(search.kind === 'searching');
  const analyzing = $derived(session.phase === 'analyzing');
  const busy = $derived(searching || importing !== null);
  const canSearch = $derived(!busy && query.trim().length > 0);
  const canImport = $derived(
    session.desktop && !busy && !analyzing && session.configIssues.length === 0,
  );
  const rejectedStatus = $derived<AnalyzeStatus | null>(
    failure?.kind === 'rejected' ? (failure.response.status as AnalyzeStatus) : null,
  );

  $effect(() => {
    locked = importing !== null;
  });

  /**
   * 對話框打開或切到這個分頁時呼叫：聚焦輸入框、索引還沒載入才自動載入；
   * 關鍵字在別的分頁改過就用新字重搜（索引未就緒時 runSearch 會先排隊）。
   */
  export function activate() {
    input?.focus();
    if (session.desktop && index === null && !indexLoading) void loadIndex(false);
    const text = query.trim();
    if (text.length > 0 && text !== submitted && canSearch) void runSearch();
  }

  /** 對話框完全關閉時呼叫；下次打開一律重新抓取。 */
  export function reset() {
    if (importing === null) downloaded.clear();
  }

  function usable(response: AnalyzeResponse): boolean {
    return response.chart !== null && response.status !== 'invalid' && response.status !== 'unsupported';
  }

  function messageOf(error: unknown): string {
    if (typeof error === 'string' && error.trim().length > 0) return error;
    if (error instanceof Error && error.message) return error.message;
    return '發生未知錯誤，請稍後再試。';
  }

  async function keepFocus(target: () => HTMLElement | null) {
    await tick();
    if (active && root && !root.contains(document.activeElement)) target()?.focus();
  }

  function focusFeedback() {
    void keepFocus(() => feedback);
  }

  function songKey(song: WikiSong): string {
    return `${song.chartType}:${song.pageId}`;
  }

  function typeLabel(chartType: string): string {
    return WIKI_TYPE_LABEL[chartType as WikiChartType] ?? chartType;
  }

  /** 依固定順序列出這首歌在索引裡出現的難度（DX 沒有 easy）。 */
  function difficultiesOf(song: WikiSong) {
    return WIKI_DIFFICULTIES.filter((item) => song.difficulties[item.key] !== undefined).map((item) => ({
      ...item,
      info: song.difficulties[item.key],
    }));
  }

  function formatFetchedAt(seconds: number): string {
    const date = new Date(seconds * 1000);
    if (Number.isNaN(date.getTime())) return '';
    const pad = (value: number) => String(value).padStart(2, '0');
    return (
      `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ` +
      `${pad(date.getHours())}:${pad(date.getMinutes())}`
    );
  }

  async function loadIndex(force: boolean) {
    if (!session.desktop) return;
    const token = ++indexToken;
    indexLoading = true;
    indexError = null;
    try {
      const payload = await refreshWikiIndex(force);
      if (token !== indexToken) return;
      index = {
        count: payload.songs.length,
        fetchedAt: payload.fetchedAt,
        fromCache: payload.fromCache,
        stale: payload.stale,
      };
    } catch (error) {
      if (token !== indexToken) return;
      // 已有索引時重新整理失敗，繼續沿用 Rust 記憶體中的舊索引。
      indexError = messageOf(error);
    } finally {
      if (token === indexToken) indexLoading = false;
    }
    if (index !== null && search.kind === 'waiting') {
      void runOnline(search.query, search.chartType);
    }
  }

  const localMatches = $derived.by<ChartRecord[]>(() => {
    const words = submitted.toLowerCase().split(/\s+/).filter((word) => word.length > 0);
    if (words.length === 0) return [];
    return records.items.filter((record) => {
      if (typeFilter !== 'all' && record.wiki?.chartType !== typeFilter) return false;
      const text = recordSearchText(record);
      return words.every((word) => text.includes(word));
    });
  });

  const onlineItems = $derived(search.kind === 'done' ? search.items : []);
  const shownItems = $derived(onlineItems.slice(0, MAX_SHOWN));
  const selected = $derived(onlineItems.find((song) => songKey(song) === selectedKey) ?? null);

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
    if (index === null) {
      // 索引未就緒時不打後端（必定 reject）；載入中就等它完成，失敗則保留錯誤提示。
      searchToken++;
      search = { kind: 'waiting', query: text, chartType: typeFilter };
      if (!indexLoading) void loadIndex(false);
      void keepFocus(() => input);
      return;
    }
    await runOnline(text, typeFilter);
    void keepFocus(() => input);
  }

  async function runOnline(text: string, chartType: TypeFilter) {
    const token = ++searchToken;
    search = { kind: 'searching', query: text, chartType };
    try {
      const items = await searchWikiSongs(text, chartType);
      if (token !== searchToken) return;
      search = { kind: 'done', query: text, chartType, items };
      if (!items.some((song) => songKey(song) === selectedKey)) {
        selectedKey = items.length > 0 ? songKey(items[0]) : null;
      }
    } catch (error) {
      if (token !== searchToken) return;
      search = { kind: 'error', query: text, chartType, message: messageOf(error) };
    }
  }

  /** 種類篩選是一次明確的點擊；已經搜尋過就用新種類重搜一次。 */
  function setTypeFilter(next: TypeFilter) {
    if (typeFilter === next) return;
    typeFilter = next;
    if (submitted.length > 0 && session.desktop && index !== null && !busy) {
      void runOnline(submitted, next);
    } else if (search.kind === 'waiting') {
      search = { ...search, chartType: next };
    }
  }

  /** Page URL 用系統瀏覽器開啟；桌面版要擋掉 WebView 內導覽。 */
  async function openPageUrl(event: MouseEvent, url: string) {
    if (!session.desktop) return;
    event.preventDefault();
    try {
      await openExternalUrl(url);
    } catch (error) {
      toasts.show({ id: 'wiki-open-url', tone: 'error', title: '無法開啟連結', body: messageOf(error) });
    }
  }

  function originOf(song: WikiSong, difficulty: Difficulty, payload: WikiChartPayload): WikiOrigin {
    return {
      pageId: song.pageId,
      chartType: song.chartType,
      difficulty: difficulty.key,
      title: payload.title.trim() || song.title,
      level: payload.level ?? song.difficulties[difficulty.key]?.level ?? null,
      pageUrl: payload.pageUrl || song.pageUrl,
    };
  }

  async function openLocal(record: ChartRecord, reason?: string) {
    if (analyzing || importing !== null) return;
    failure = null;
    onClose();
    records.activeId = record.id;
    if (reason) {
      toasts.show({ id: 'wiki-local', tone: 'info', title: '已在本機', body: reason });
    }
    await session.load(record.source);
  }

  async function importChart(song: WikiSong, difficulty: Difficulty) {
    if (!canImport) return;
    failure = null;
    const known = records.findByWiki(song.pageId, song.chartType, difficulty.key);
    if (known) {
      await openLocal(known, `「${recordTitle(known)}」已經匯入過，直接開啟本機紀錄。`);
      return;
    }
    const key = `${songKey(song)}:${difficulty.key}`;
    let payload = downloaded.get(key);
    if (payload === undefined) {
      importing = { key: songKey(song), difficulty, step: 'download' };
      try {
        payload = await fetchWikiChart(song.pageId, song.chartType, difficulty.key);
        downloaded.set(key, payload);
      } catch (error) {
        importing = null;
        failure = { kind: 'download', song, difficulty, message: messageOf(error) };
        focusFeedback();
        return;
      }
    }
    // 原文 hash 為主：逐字相同就視為同一張，補上 Wiki 來源後直接開啟。
    const same = records.findByHash(await hashText(payload.chartText));
    if (same) {
      importing = null;
      records.attachWiki(same.id, originOf(song, difficulty, payload));
      await openLocal(same, `本機已有內容完全相同的譜面「${recordTitle(same)}」，直接開啟。`);
      return;
    }
    importing = { key: songKey(song), difficulty, step: 'analyze' };
    const response = await session.analyze(payload.chartText, usable);
    importing = null;
    if (!response) {
      failure = {
        kind: 'analysis',
        song,
        difficulty,
        payload,
        message: session.errorMessage ?? '分析沒有完成，盤面維持原狀。',
      };
      focusFeedback();
      return;
    }
    if (!usable(response)) {
      failure = { kind: 'rejected', song, difficulty, response, payload };
      focusFeedback();
      return;
    }
    await records.add(payload.chartText, { wiki: originOf(song, difficulty, payload) });
    failure = null;
    onClose();
  }
</script>

{#snippet levelList(song: WikiSong)}
  <ul class="levels" aria-label="難度">
    {#each difficultiesOf(song) as item (item.key)}
      <li
        class="badge badge--quiet mono"
        class:is-unavailable={!item.info.availabilityHint}
        title={`${item.label}${item.info.availabilityHint ? '' : '（unavailable）'}`}
      >
        {DIFFICULTY_SHORT[item.key]}
        {item.info.level ?? '?'}
      </li>
    {/each}
  </ul>
{/snippet}

<div bind:this={root} class="panel" aria-busy={busy || indexLoading}>
  <p class="source-note xsmall muted">
    <span class="badge badge--quiet source">Source: simai Wiki</span>
    Wiki 使用者轉錄的文字譜，note 位置與 timing 不保證與官方譜面完全一致。
  </p>

  <div class="index-bar" role="status">
    {#if !session.desktop}
      <span class="xsmall muted row"><Icon name="info" size={14} />瀏覽器預覽不載入 simai Wiki 索引</span>
    {:else if indexLoading}
      <span class="xsmall row"><Icon name="loader" size={14} spin />Loading simai Wiki index...</span>
    {:else if index && index.stale}
      <span class="xsmall row">
        <Icon name="triangle-alert" size={14} />Offline / Wiki unavailable, using cached index
        <span class="muted mono">{index.count} songs・{formatFetchedAt(index.fetchedAt)}</span>
      </span>
    {:else if index}
      <span class="xsmall row">
        <Icon name="circle-check" size={14} />simai Wiki index ready
        <span class="muted mono">
          {index.count} songs・{formatFetchedAt(index.fetchedAt)}{index.fromCache ? '・cached' : ''}
        </span>
      </span>
    {:else if indexError}
      <span class="xsmall row"><Icon name="circle-alert" size={14} />索引載入失敗，沒有可用的快取</span>
    {/if}
    <span class="spacer"></span>
    {#if session.desktop}
      <button
        class="btn filter"
        onclick={() => loadIndex(index !== null)}
        disabled={indexLoading || importing !== null}
        title={index === null ? '重新嘗試載入索引' : '忽略快取，重新從 simai Wiki 抓取索引'}
      >
        <Icon name="refresh-cw" size={14} />{index === null && indexError ? '重試' : '重新整理索引'}
      </button>
    {/if}
  </div>
  {#if session.desktop && indexError && !indexLoading}
    <p class="notice small field-error index-error">
      {index ? `重新整理失敗，繼續使用目前的索引：${indexError}` : indexError}
    </p>
  {/if}

  <form class="search-bar" role="search" onsubmit={runSearch}>
    <label class="sr-only" for="wiki-query">搜尋 Wiki 歌曲名稱或紀錄名稱</label>
    <input
      id="wiki-query"
      class="input"
      type="search"
      autocomplete="off"
      spellcheck="false"
      maxlength={MAX_QUERY}
      placeholder="歌曲名稱或紀錄名稱"
      bind:this={input}
      bind:value={query}
    />
    <button class="btn btn--primary" type="submit" disabled={!canSearch}>
      <Icon name={searching ? 'loader' : 'search'} spin={searching} />
      {searching ? '搜尋中' : '搜尋'}
    </button>
  </form>

  <div class="filter-bar" role="group" aria-labelledby="wiki-type-label">
    <span id="wiki-type-label" class="xsmall muted">種類</span>
    {#each TYPE_FILTERS as item (item.id)}
      <button
        class="btn filter"
        class:is-active={typeFilter === item.id}
        aria-pressed={typeFilter === item.id}
        disabled={busy}
        onclick={() => setTypeFilter(item.id)}>{item.label}</button
      >
    {/each}
  </div>

  {#if !session.desktop}
    <p class="notice field-error">瀏覽器預覽沒有 Rust 核心，只能搜尋本機紀錄，無法搜尋 simai Wiki 或匯入。</p>
  {:else if session.configIssues.length > 0}
    <p class="notice field-error">「參數」分頁有超出範圍的數值，修正後才能匯入。</p>
  {/if}

  {#if failure}
    <section class="feedback" tabindex="-1" bind:this={feedback} aria-labelledby="wiki-failure">
      <div class="row row-wrap">
        {#if failure.kind === 'rejected' && rejectedStatus}
          <span class="badge badge--danger">{STATUS_LABEL[rejectedStatus] ?? rejectedStatus}</span>
        {:else}
          <span class="badge badge--danger">
            {failure.kind === 'download' ? '取得失敗' : '分析未完成'}
          </span>
        {/if}
        <span id="wiki-failure" class="small feedback-title">
          「{failure.song.title}」{typeLabel(failure.song.chartType)}
          {failure.difficulty.label} 沒有匯入
        </span>
        <span class="spacer"></span>
        <button class="btn btn--icon" onclick={() => (failure = null)} aria-label="關閉匯入錯誤">
          <Icon name="x" size={14} />
        </button>
      </div>
      {#if failure.kind === 'rejected'}
        <p class="xsmall muted">
          {rejectedStatus && STATUS_HINT[rejectedStatus] ? `${STATUS_HINT[rejectedStatus]} ` : ''}盤面與譜面紀錄維持原狀，可以改選別的難度。
        </p>
        {#if failure.response.diagnostics.length > 0}
          <div class="feedback-list scroll">
            <DiagnosticList diagnostics={failure.response.diagnostics} />
          </div>
        {/if}
      {:else}
        <p class="small">{failure.message}</p>
        <p class="xsmall muted">盤面與譜面紀錄維持原狀，可以重試或改選別的難度。</p>
      {/if}
      {#if failure.kind !== 'download'}
        {@const current = failure}
        <div class="row row-wrap">
          <CopyDebugButton
            input={() => ({
              context: 'simai Wiki 匯入',
              source: current.payload.chartText,
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
                'Wiki page id': String(current.song.pageId),
                'Wiki 標題': current.payload.title,
                種類: typeLabel(current.song.chartType),
                難度: current.difficulty.label,
                Level: current.payload.level ?? '—',
                'Page URL': current.payload.pageUrl,
              },
            })}
          />
        </div>
      {/if}
    </section>
  {/if}

  <div class="body">
    <div class="results scroll" aria-live="polite">
      {#if submitted.length === 0}
        <p class="state small muted">
          輸入關鍵字後按 Enter，會同時搜尋本機譜面紀錄與 simai Wiki 索引。選一首歌後在右側選難度載入：文字譜交給核心分析成功後才加入譜面紀錄。Standard 與 DX 是不同的譜面，分開列出。
        </p>
      {:else}
        <section class="group" aria-labelledby="wiki-local-title">
          <h3 id="wiki-local-title" class="group-title">
            <span>本機紀錄</span>
            <span class="mono">{localMatches.length}</span>
          </h3>
          {#if localMatches.length === 0}
            <p class="group-empty small muted">
              沒有符合「{submitted}」{typeFilter !== 'all' ? `的 ${typeLabel(typeFilter)} Wiki` : '的本機'}紀錄。
            </p>
          {:else}
            <ul class="list">
              {#each localMatches as record (record.id)}
                <li class="item" class:is-current={records.activeId === record.id}>
                  <div class="info">
                    <span class="title">{recordTitle(record)}</span>
                    <dl class="meta xsmall">
                      {#if record.wiki}
                        <dt>來源</dt>
                        <dd>simai Wiki・Level {record.wiki.level ?? '—'}</dd>
                      {/if}
                      <dt>新增</dt>
                      <dd class="mono">{recordDate(record)}</dd>
                    </dl>
                  </div>
                  <button
                    class="btn"
                    onclick={() => openLocal(record)}
                    disabled={analyzing || importing !== null || !session.desktop}
                    aria-label={`開啟本機紀錄 ${recordTitle(record)}`}
                  >
                    <Icon name="external-link" />開啟
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        {#if session.desktop}
          <section class="group" aria-labelledby="wiki-online-title">
            <h3 id="wiki-online-title" class="group-title">
              <span>simai Wiki</span>
              {#if search.kind === 'done'}
                <span class="mono">{search.items.length}</span>
              {/if}
            </h3>
            {#if search.kind === 'waiting'}
              <p class="group-empty small muted row">
                {#if indexLoading}
                  <Icon name="loader" spin />索引載入完成後會搜尋「{search.query}」
                {:else}
                  <Icon name="circle-alert" />索引沒有載入，無法搜尋。請按上方的重試。
                {/if}
              </p>
            {:else if search.kind === 'searching'}
              <p class="group-empty small muted row">
                <Icon name="loader" spin />正在搜尋「{search.query}」
              </p>
            {:else if search.kind === 'error'}
              <div class="group-empty stack-sm">
                <p class="small row"><Icon name="circle-alert" />搜尋「{search.query}」失敗</p>
                <p class="small muted">{search.message}</p>
                <div>
                  <button
                    class="btn"
                    onclick={() => runOnline(submitted, typeFilter)}
                    disabled={busy || index === null}
                  >
                    <Icon name="refresh-cw" />重試
                  </button>
                </div>
              </div>
            {:else if search.kind === 'done' && search.items.length === 0}
              <p class="group-empty small muted">
                simai Wiki 索引裡找不到符合「{search.query}」{search.chartType !== 'all'
                  ? `的 ${typeLabel(search.chartType)}`
                  : '的'}歌曲，換個關鍵字試試。
              </p>
            {:else if search.kind === 'done'}
              <ul class="list">
                {#each shownItems as song (songKey(song))}
                  {@const key = songKey(song)}
                  <li>
                    <button
                      class="item song"
                      class:is-current={selectedKey === key}
                      aria-pressed={selectedKey === key}
                      onclick={() => (selectedKey = key)}
                    >
                      <span class="info">
                        <span class="title">
                          {song.title}
                          <span class="badge badge--quiet type-badge">{typeLabel(song.chartType)}</span>
                          {#if importing?.key === key}
                            <Icon name="loader" size={13} spin />
                          {/if}
                        </span>
                        {#if song.section}
                          <span class="xsmall muted">{song.section}</span>
                        {/if}
                        {@render levelList(song)}
                      </span>
                    </button>
                  </li>
                {/each}
              </ul>
              {#if search.items.length > MAX_SHOWN}
                <p class="group-empty xsmall muted">
                  只列出前 {MAX_SHOWN} 筆（共 {search.items.length} 筆），請用更完整的關鍵字縮小範圍。
                </p>
              {/if}
            {/if}
          </section>
        {/if}
      {/if}
    </div>

    {#if session.desktop}
      <section class="preview scroll" aria-labelledby="wiki-preview-title">
        <h3 id="wiki-preview-title" class="group-title"><span>預覽</span></h3>
        {#if selected}
          {@const song = selected}
          {@const key = songKey(song)}
          <dl class="meta preview-meta small">
            <dt>Song</dt>
            <dd class="preview-song">{song.title}</dd>
            <dt>Type</dt>
            <dd>{typeLabel(song.chartType)}</dd>
            {#if song.section}
              <dt>Section</dt>
              <dd>{song.section}</dd>
            {/if}
            <dt>Page URL</dt>
            <dd class="mono xsmall url">
              <a
                class="url-link"
                href={song.pageUrl}
                target="_blank"
                rel="noopener noreferrer"
                title="在瀏覽器開啟 Wiki 歌曲頁"
                onclick={(event) => void openPageUrl(event, song.pageUrl)}
                >{song.pageUrl}</a
              >
            </dd>
          </dl>
          <table class="difficulties small">
            <thead>
              <tr>
                <th scope="col">Difficulty</th>
                <th scope="col">Level</th>
                <th scope="col"><span class="sr-only">操作</span></th>
              </tr>
            </thead>
            <tbody>
              {#each difficultiesOf(song) as item (item.key)}
                {@const current = importing?.key === key && importing.difficulty.key === item.key}
                {@const local = records.findByWiki(song.pageId, song.chartType, item.key)}
                {@const failed =
                  failure !== null &&
                  songKey(failure.song) === key &&
                  failure.difficulty.key === item.key}
                <tr class:is-current={current || failed}>
                  <th scope="row" class="mono">{item.label}</th>
                  <td class="mono">{item.info.level ?? '—'}</td>
                  <td class="action">
                    {#if local}
                      <button
                        class="btn"
                        onclick={() => importChart(song, item)}
                        disabled={!canImport}
                        aria-label={`開啟已匯入的 ${song.title} ${typeLabel(song.chartType)} ${item.label}`}
                      >
                        <Icon name="external-link" />開啟
                      </button>
                    {:else if !item.info.availabilityHint}
                      <button
                        class="btn"
                        disabled
                        title="Wiki 索引沒有這個難度的譜面連結"
                        aria-label={`${song.title} ${item.label} unavailable`}
                      >
                        unavailable
                      </button>
                    {:else}
                      <button
                        class="btn"
                        onclick={() => importChart(song, item)}
                        disabled={!canImport}
                        aria-label={`載入 ${song.title} ${typeLabel(song.chartType)} ${item.label}`}
                      >
                        {#if current}
                          <Icon name="loader" spin />{importing?.step === 'download'
                            ? `Loading ${item.label}...`
                            : '分析中'}
                        {:else}
                          <Icon name="download" />載入
                        {/if}
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          <p class="xsmall muted preview-note">
            unavailable 表示 Wiki 索引沒有該難度的連結；實際能否取得以 Rust 回傳為準。
          </p>
        {:else}
          <p class="group-empty small muted">
            {search.kind === 'done' && search.items.length > 0 ? '在左側選一首歌。' : '搜尋後在左側選一首歌，這裡會列出難度。'}
          </p>
        {/if}
      </section>
    {/if}
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
  }

  .source-note {
    padding: var(--space-3) var(--space-4) 0;
    line-height: var(--lh-normal);
  }

  .source {
    margin-right: var(--space-1);
    font-weight: 400;
  }

  .index-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .index-bar .row {
    gap: var(--space-2);
  }

  .index-error {
    padding-top: var(--space-2);
    overflow-wrap: anywhere;
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

  /* 左邊搜尋結果、右邊預覽；兩欄各自捲動，分隔用線不另加框。 */
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(260px, 320px);
    flex: 1 1 auto;
    min-height: 0;
    border-top: 1px solid var(--c-border);
  }

  .results,
  .preview {
    min-height: 0;
  }

  .preview {
    border-left: 1px solid var(--c-border);
  }

  @media (max-width: 700px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
    }

    .preview {
      border-left: none;
      border-top: 1px solid var(--c-border);
    }
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

  .type-badge {
    margin-left: var(--space-1);
    font-size: var(--fs-xs);
    font-weight: 400;
    vertical-align: middle;
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

  .list > li:last-child > .item,
  .item:last-child {
    border-bottom: none;
  }

  .item.is-current {
    background: var(--c-control);
  }

  /* 歌曲列整列可點，外觀與本機紀錄列一致。 */
  .song {
    width: 100%;
    color: inherit;
    font: inherit;
    text-align: left;
    background: none;
    border-top: none;
    border-left: none;
    border-right: none;
    cursor: pointer;
  }

  .song:hover {
    background: var(--c-control);
  }

  .song.is-current {
    background: var(--c-control-hover);
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

  .levels .badge.is-unavailable {
    color: var(--c-text-dim);
    text-decoration: line-through;
  }

  .preview-meta {
    gap: var(--space-1) var(--space-3);
    padding: var(--space-2) var(--space-4) var(--space-3);
  }

  .preview-song {
    font-weight: 600;
    color: var(--c-text-strong);
  }

  .url {
    user-select: text;
  }

  .url-link {
    color: var(--c-right);
    text-decoration: underline;
    overflow-wrap: anywhere;
  }

  .difficulties {
    width: calc(100% - 2 * var(--space-2));
    margin: 0 var(--space-2);
    border-collapse: collapse;
  }

  .difficulties th,
  .difficulties td {
    padding: var(--space-1) var(--space-2);
    text-align: left;
    border-bottom: 1px solid var(--c-border);
  }

  .difficulties thead th {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--c-text-dim);
  }

  .difficulties tbody th {
    font-weight: 600;
  }

  .difficulties tbody tr:last-child th,
  .difficulties tbody tr:last-child td {
    border-bottom: none;
  }

  .difficulties tr.is-current {
    background: var(--c-control);
  }

  .difficulties .action {
    text-align: right;
    white-space: nowrap;
  }

  .preview-note {
    padding: var(--space-3) var(--space-4) var(--space-4);
  }
</style>
