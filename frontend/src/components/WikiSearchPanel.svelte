<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import DiagnosticList from './DiagnosticList.svelte';
  import CopyDebugButton from './CopyDebugButton.svelte';
  import { STATUS_HINT, STATUS_LABEL } from '../lib/contract';
  import {
    fetchAdxChart,
    fetchWikiChart,
    openExternalUrl,
    refreshAdxIndex,
    refreshWikiIndex,
    searchCharts,
  } from '../lib/api';
  import { hashText } from '../lib/db';
  import { segmented } from '../lib/segmented';
  import {
    WIKI_DIFFICULTIES,
    WIKI_TYPE_LABEL,
    recordDate,
    recordSearchText,
    recordTitle,
    records,
    type AdxOrigin,
    type ChartRecord,
    type WikiDifficultyKey,
    type WikiOrigin,
  } from '../state/records.svelte';
  import { session } from '../state/session.svelte';
  import { errorLog, type ImportFailure } from '../state/errorLog.svelte';
  import { cacheEvents } from '../state/cacheEvents.svelte';
  import type { DebugInput } from '../lib/debug';
  import { toasts } from '../state/toasts.svelte';
  import type {
    AdxChartPayload,
    AnalyzeResponse,
    AnalyzeStatus,
    ChartSearchHit,
    WikiChartPayload,
    WikiChartType,
  } from '../lib/types';

  /**
   * 譜面網站分頁：simai Wiki 與 adxdls 合併搜尋。兩邊的索引各自載入、各自快取，
   * 搜尋在 Rust 端合併（同種類、同歌名的合成一筆）；預覽時選要從哪邊載入。
   */
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
  type Source = 'wiki' | 'adx';
  type Payload = WikiChartPayload | AdxChartPayload;

  const TYPE_FILTERS: { id: TypeFilter; label: string }[] = [
    { id: 'all', label: '全部' },
    { id: 'standard', label: 'Standard' },
    { id: 'deluxe', label: 'DX' },
  ];
  /** 預覽的來源切換依這個順序排。 */
  const SOURCES: { id: Source; label: string }[] = [
    { id: 'adx', label: 'adxdls' },
    { id: 'wiki', label: 'simai Wiki' },
  ];
  const SOURCE_LABEL: Record<Source, string> = { wiki: 'simai Wiki', adx: 'adxdls' };
  const SOURCE_KEY = 'maimotion.chartSource';
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
  type SourceIndex = { info: IndexInfo | null; loading: boolean; error: string | null };
  type Ready = Record<Source, boolean>;

  type SearchState =
    | { kind: 'idle' }
    /** 送出時索引還在載入，載入完成後自動搜尋一次。 */
    | { kind: 'waiting'; query: string; chartType: TypeFilter }
    | { kind: 'searching'; query: string; chartType: TypeFilter }
    | { kind: 'error'; query: string; chartType: TypeFilter; message: string }
    | { kind: 'done'; query: string; chartType: TypeFilter; items: ChartSearchHit[]; ready: Ready };

  type Failure =
    | { kind: 'download'; hit: ChartSearchHit; source: Source; difficulty: Difficulty; message: string }
    | {
        kind: 'analysis';
        hit: ChartSearchHit;
        source: Source;
        difficulty: Difficulty;
        message: string;
        payload: Payload;
      }
    | {
        kind: 'rejected';
        hit: ChartSearchHit;
        source: Source;
        difficulty: Difficulty;
        response: AnalyzeResponse;
        payload: Payload;
      };

  /** 預覽表格的一列：該來源有列出的難度。 */
  type Row = Difficulty & { level: string | null; designer: string | null; available: boolean };

  let root: HTMLElement | null = $state(null);
  let input: HTMLInputElement | null = $state(null);
  let feedback: HTMLElement | null = $state(null);

  let typeFilter = $state<TypeFilter>('all');
  let search = $state<SearchState>({ kind: 'idle' });
  /** 最後一次送出的關鍵字；本機紀錄即時依這個字篩選。 */
  let submitted = $state('');

  const indexes = $state<Record<Source, SourceIndex>>({
    wiki: { info: null, loading: false, error: null },
    adx: { info: null, loading: false, error: null },
  });
  const indexTokens: Record<Source, number> = { wiki: 0, adx: 0 };
  const anyIndex = $derived(indexes.wiki.info !== null || indexes.adx.info !== null);
  const anyLoading = $derived(indexes.wiki.loading || indexes.adx.loading);

  /** 兩邊都有時預設從哪邊載入；記在本機，下次沿用。 */
  let preferred = $state<Source>(initialSource());

  /** 右側預覽的歌曲。 */
  let selectedKey = $state<string | null>(null);
  let importing = $state<{
    key: string;
    source: Source;
    difficulty: Difficulty;
    step: 'download' | 'analyze';
  } | null>(null);
  let failure = $state<Failure | null>(null);

  /** 已取得的譜面；分析失敗後重試不必再抓一次。對話框完全關閉時清空。 */
  const downloaded = new Map<string, Payload>();
  let searchToken = 0;

  const searching = $derived(search.kind === 'searching');
  const busy = $derived(searching || importing !== null);
  const canSearch = $derived(!busy && query.trim().length > 0);
  const canImport = $derived(session.desktop && !busy && session.configIssues.length === 0);
  const rejectedStatus = $derived<AnalyzeStatus | null>(
    failure?.kind === 'rejected' ? (failure.response.status as AnalyzeStatus) : null,
  );

  $effect(() => {
    locked = importing !== null;
  });

  function initialSource(): Source {
    try {
      return localStorage.getItem(SOURCE_KEY) === 'wiki' ? 'wiki' : 'adx';
    } catch {
      return 'adx';
    }
  }

  function choose(source: Source) {
    preferred = source;
    try {
      localStorage.setItem(SOURCE_KEY, source);
    } catch {
      // 無法保存就只在這次生效。
    }
  }

  /**
   * 對話框打開或切到這個分頁時呼叫：聚焦輸入框、索引還沒載入才自動載入；
   * 關鍵字在別的分頁改過就用新字重搜（索引未就緒時 runSearch 會先排隊）。
   */
  export function activate() {
    input?.focus();
    if (session.desktop) {
      for (const source of ['wiki', 'adx'] as const) {
        const index = indexes[source];
        if (index.info === null && !index.loading) void loadIndex(source, false);
      }
    }
    const text = query.trim();
    if (text.length > 0 && text !== submitted && canSearch) void runSearch();
  }

  /** 對話框完全關閉時呼叫；下次打開一律重新抓取。 */
  export function reset() {
    if (importing === null) downloaded.clear();
  }

  function hitKey(hit: ChartSearchHit): string {
    return `${hit.chartType}:${hit.wiki?.pageId ?? ''}:${hit.adx?.slug ?? ''}`;
  }

  /** 這首歌實際使用的來源：偏好的那邊沒有就用另一邊。 */
  function sourceOf(hit: ChartSearchHit): Source {
    if (hit[preferred]) return preferred;
    return hit.wiki ? 'wiki' : 'adx';
  }

  function typeLabel(chartType: string): string {
    return WIKI_TYPE_LABEL[chartType as WikiChartType] ?? chartType;
  }

  function pageUrlOf(hit: ChartSearchHit, source: Source): string {
    return (source === 'wiki' ? hit.wiki?.pageUrl : hit.adx?.pageUrl) ?? '';
  }

  /** 依固定順序列出這首歌在該來源的難度（Wiki DX 沒有 easy；adxdls 沒有 easy）。 */
  function rowsOf(hit: ChartSearchHit, source: Source): Row[] {
    return WIKI_DIFFICULTIES.flatMap((item): Row[] => {
      if (source === 'wiki') {
        const info = hit.wiki?.difficulties[item.key];
        return info ? [{ ...item, level: info.level, designer: null, available: info.availabilityHint }] : [];
      }
      const info = hit.adx?.difficulties[item.key];
      return info ? [{ ...item, level: info.level, designer: info.designer, available: true }] : [];
    });
  }

  function localOf(hit: ChartSearchHit, source: Source, key: string): ChartRecord | null {
    if (source === 'wiki') return hit.wiki ? records.findByWiki(hit.wiki.pageId, hit.chartType, key) : null;
    return hit.adx ? records.findByAdx(hit.adx.slug, key) : null;
  }

  function failureTitle(current: Failure): string {
    return `${current.hit.title}（${typeLabel(current.hit.chartType)} ${current.difficulty.label}）`;
  }

  function debugInputOf(current: Failure): Omit<DebugInput, 'appVersion'> {
    const payload = current.kind === 'download' ? null : current.payload;
    const extra: Record<string, string> =
      current.source === 'wiki'
        ? { 'Wiki page id': String(current.hit.wiki?.pageId ?? '—'), 'Wiki 標題': payload?.title ?? current.hit.title }
        : { 'adxdls slug': current.hit.adx?.slug ?? '—', 'adxdls 標題': payload?.title ?? current.hit.title };
    return {
      context: `${SOURCE_LABEL[current.source]} 匯入`,
      source: payload?.chartText ?? '',
      config:
        current.kind === 'analysis'
          ? (session.lastFailure?.request.solverConfig ?? session.requestConfig)
          : session.requestConfig,
      firstSeconds: session.firstSeconds,
      requestId:
        current.kind === 'analysis' ? (session.lastFailure?.request.requestId ?? null) : null,
      response: current.kind === 'rejected' ? current.response : null,
      error: current.kind === 'rejected' ? null : current.message,
      extra: {
        ...extra,
        種類: typeLabel(current.hit.chartType),
        難度: current.difficulty.label,
        Level: payload?.level ?? '—',
        'Page URL': payload?.pageUrl ?? pageUrlOf(current.hit, current.source),
      },
    };
  }

  /** 匯入失敗一律寫進設定頁的錯誤紀錄；相同錯誤由 errorLog 去重。 */
  function logFailure(current: Failure) {
    const identity =
      current.source === 'wiki'
        ? `wiki:${current.hit.wiki?.pageId}:${current.hit.chartType}:${current.difficulty.key}`
        : `adx:${current.hit.adx?.slug}:${current.difficulty.key}`;
    const entry: ImportFailure = {
      kind: current.kind,
      origin: SOURCE_LABEL[current.source],
      title: failureTitle(current),
      identity,
      summary:
        current.kind === 'rejected'
          ? `${STATUS_LABEL[current.response.status as AnalyzeStatus] ?? current.response.status}：${current.response.diagnostics[0]?.message ?? '沒有診斷'}`
          : current.message,
      debug: $state.snapshot(debugInputOf(current)) as Omit<DebugInput, 'appVersion'>,
    };
    void errorLog.record(entry);
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

  function formatFetchedAt(seconds: number): string {
    const date = new Date(seconds * 1000);
    if (Number.isNaN(date.getTime())) return '';
    const pad = (value: number) => String(value).padStart(2, '0');
    return (
      `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ` +
      `${pad(date.getHours())}:${pad(date.getMinutes())}`
    );
  }

  // 設定頁清除了快取：Rust 端兩邊的索引都已不在，這裡的索引資訊與已下載的譜面一併作廢。
  let seenWikiGeneration = cacheEvents.wikiGeneration;
  $effect(() => {
    const generation = cacheEvents.wikiGeneration;
    if (generation === seenWikiGeneration) return;
    seenWikiGeneration = generation;
    for (const source of ['wiki', 'adx'] as const) {
      indexTokens[source] += 1;
      indexes[source] = { info: null, loading: false, error: null };
    }
    if (importing === null) downloaded.clear();
  });

  async function loadIndex(source: Source, force: boolean) {
    if (!session.desktop) return;
    const token = ++indexTokens[source];
    indexes[source].loading = true;
    indexes[source].error = null;
    try {
      const info =
        source === 'wiki'
          ? await refreshWikiIndex(force).then((payload) => ({ ...payload, count: payload.songs.length }))
          : await refreshAdxIndex(force);
      if (token !== indexTokens[source]) return;
      indexes[source].info = {
        count: info.count,
        fetchedAt: info.fetchedAt,
        fromCache: info.fromCache,
        stale: info.stale,
      };
    } catch (error) {
      if (token !== indexTokens[source]) return;
      // 已有索引時重新整理失敗，繼續沿用 Rust 記憶體中的舊索引。
      indexes[source].error = messageOf(error);
    } finally {
      if (token === indexTokens[source]) indexes[source].loading = false;
    }
    if (token !== indexTokens[source] || importing !== null) return;
    // 排隊中的搜尋在第一份索引就緒時送出；已經有結果但少了這個來源，就補搜一次。
    if (search.kind === 'waiting' && anyIndex) {
      void runOnline(search.query, search.chartType);
    } else if (search.kind === 'done' && !search.ready[source] && indexes[source].info !== null) {
      void runOnline(search.query, search.chartType);
    }
  }

  function refreshAll() {
    for (const source of ['wiki', 'adx'] as const) {
      if (!indexes[source].loading) void loadIndex(source, indexes[source].info !== null);
    }
  }

  const localMatches = $derived.by<ChartRecord[]>(() => {
    const words = submitted.toLowerCase().split(/\s+/).filter((word) => word.length > 0);
    if (words.length === 0) return [];
    return records.items.filter((record) => {
      if (typeFilter !== 'all' && (record.wiki ?? record.adx)?.chartType !== typeFilter) return false;
      const text = recordSearchText(record);
      return words.every((word) => text.includes(word));
    });
  });

  const onlineItems = $derived(search.kind === 'done' ? search.items : []);
  const shownItems = $derived(onlineItems.slice(0, MAX_SHOWN));
  const selected = $derived(onlineItems.find((hit) => hitKey(hit) === selectedKey) ?? null);

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
    if (!anyIndex) {
      // 兩邊索引都未就緒時不打後端（必定 reject）；載入中就等，失敗則保留錯誤提示。
      searchToken++;
      search = { kind: 'waiting', query: text, chartType: typeFilter };
      if (!anyLoading) refreshAll();
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
      const result = await searchCharts(text, chartType);
      if (token !== searchToken) return;
      search = {
        kind: 'done',
        query: text,
        chartType,
        items: result.items,
        ready: { wiki: result.wikiReady, adx: result.adxReady },
      };
      if (!result.items.some((hit) => hitKey(hit) === selectedKey)) {
        selectedKey = result.items.length > 0 ? hitKey(result.items[0]) : null;
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
    if (submitted.length > 0 && session.desktop && anyIndex && !busy) {
      void runOnline(submitted, next);
    } else if (search.kind === 'waiting') {
      search = { ...search, chartType: next };
    }
  }

  /** 外部連結用系統瀏覽器開啟；桌面版要擋掉 WebView 內導覽。 */
  async function openPageUrl(event: MouseEvent, url: string) {
    if (!session.desktop) return;
    event.preventDefault();
    try {
      await openExternalUrl(url);
    } catch (error) {
      toasts.show({ id: 'wiki-open-url', tone: 'error', title: '無法開啟連結', body: messageOf(error) });
    }
  }

  function wikiOriginOf(hit: ChartSearchHit, difficulty: Difficulty, payload: Payload): WikiOrigin | null {
    const song = hit.wiki;
    if (!song) return null;
    return {
      pageId: song.pageId,
      chartType: song.chartType,
      difficulty: difficulty.key,
      title: payload.title.trim() || song.title,
      level: payload.level ?? song.difficulties[difficulty.key]?.level ?? null,
      pageUrl: payload.pageUrl || song.pageUrl,
    };
  }

  function adxOriginOf(hit: ChartSearchHit, difficulty: Difficulty, payload: Payload): AdxOrigin | null {
    const song = hit.adx;
    if (!song) return null;
    const info = song.difficulties[difficulty.key];
    return {
      slug: song.slug,
      chartType: song.chartType,
      difficulty: difficulty.key,
      title: payload.title.trim() || song.title,
      artist: song.artist,
      level: payload.level ?? info?.level ?? null,
      designer: info?.designer ?? null,
      pageUrl: payload.pageUrl || song.pageUrl,
    };
  }

  async function openLocal(record: ChartRecord, reason?: string) {
    if (importing !== null) return;
    failure = null;
    onClose();
    records.activeId = record.id;
    if (reason) {
      toasts.show({ id: 'wiki-local', tone: 'info', title: '已在本機', body: reason });
    }
    await session.load(record.source);
  }

  async function fetchPayload(hit: ChartSearchHit, source: Source, difficulty: Difficulty): Promise<Payload> {
    if (source === 'wiki') {
      if (!hit.wiki) throw new Error('simai Wiki 沒有這首歌');
      return await fetchWikiChart(hit.wiki.pageId, hit.chartType, difficulty.key);
    }
    if (!hit.adx) throw new Error('adxdls 沒有這首歌');
    return await fetchAdxChart(hit.adx.slug, difficulty.key);
  }

  async function importChart(hit: ChartSearchHit, source: Source, difficulty: Difficulty) {
    if (!canImport) return;
    failure = null;
    const known = localOf(hit, source, difficulty.key);
    if (known) {
      await openLocal(known, `「${recordTitle(known)}」已經匯入過，直接開啟本機紀錄。`);
      return;
    }
    const key = hitKey(hit);
    const cacheKey = `${source}:${key}:${difficulty.key}`;
    let payload = downloaded.get(cacheKey);
    if (payload === undefined) {
      importing = { key, source, difficulty, step: 'download' };
      try {
        payload = await fetchPayload(hit, source, difficulty);
        downloaded.set(cacheKey, payload);
      } catch (error) {
        importing = null;
        failure = { kind: 'download', hit, source, difficulty, message: messageOf(error) };
        logFailure(failure);
        focusFeedback();
        return;
      }
    }
    const wiki = source === 'wiki' ? wikiOriginOf(hit, difficulty, payload) : null;
    const adx = source === 'adx' ? adxOriginOf(hit, difficulty, payload) : null;
    // 原文 hash 為主：逐字相同就視為同一張，補上來源後直接開啟。
    const same = records.findByHash(await hashText(payload.chartText));
    if (same) {
      importing = null;
      if (wiki) records.attachWiki(same.id, wiki);
      if (adx) records.attachAdx(same.id, adx);
      await openLocal(same, `本機已有內容完全相同的譜面「${recordTitle(same)}」，直接開啟。`);
      return;
    }
    importing = { key, source, difficulty, step: 'analyze' };
    const response = await session.analyze(payload.chartText, usable);
    importing = null;
    if (!response) {
      failure = {
        kind: 'analysis',
        hit,
        source,
        difficulty,
        payload,
        message: session.errorMessage ?? '分析沒有完成，盤面維持原狀。',
      };
      logFailure(failure);
      focusFeedback();
      return;
    }
    if (!usable(response)) {
      failure = { kind: 'rejected', hit, source, difficulty, response, payload };
      logFailure(failure);
      focusFeedback();
      return;
    }
    await records.add(payload.chartText, { ...(wiki ? { wiki } : {}), ...(adx ? { adx } : {}) });
    failure = null;
    onClose();
  }
</script>

{#snippet levelList(hit: ChartSearchHit)}
  {@const source = sourceOf(hit)}
  <ul class="levels" aria-label={`難度（${SOURCE_LABEL[source]}）`}>
    {#each rowsOf(hit, source) as item (item.key)}
      <li
        class="badge badge--quiet mono"
        class:is-unavailable={!item.available}
        title={`${item.label}${item.available ? '' : '（unavailable）'}`}
      >
        {DIFFICULTY_SHORT[item.key]}
        {item.level ?? '?'}
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet indexState(source: Source)}
  {@const index = indexes[source]}
  {@const label = SOURCE_LABEL[source]}
  {#if index.loading}
    <span class="xsmall row"><Icon name="loader" size={14} spin />{label} 索引載入中</span>
  {:else if index.info && index.info.stale}
    <span class="xsmall row">
      <Icon name="triangle-alert" size={14} />{label} 連不上，使用舊索引
      <span class="muted mono">{index.info.count} 首・{formatFetchedAt(index.info.fetchedAt)}</span>
    </span>
  {:else if index.info}
    <span class="xsmall row">
      <Icon name="circle-check" size={14} />{label}
      <span class="muted mono">
        {index.info.count} 首・{formatFetchedAt(index.info.fetchedAt)}{index.info.fromCache ? '・快取' : ''}
      </span>
    </span>
  {:else if index.error}
    <span class="xsmall row"><Icon name="circle-alert" size={14} />{label} 索引載入失敗</span>
  {/if}
{/snippet}

<div bind:this={root} class="panel" aria-busy={busy || anyLoading}>
  <p class="source-note xsmall muted">
    同時搜尋 simai Wiki（使用者轉錄的文字譜）與
    <a
      class="url-link"
      href="https://adxdls.saop.cc"
      target="_blank"
      rel="noopener noreferrer"
      onclick={(event) => void openPageUrl(event, 'https://adxdls.saop.cc')}>adxdls</a
    >（AstroDX 譜面目錄，目錄資料 CC BY 4.0），同一首歌合成一筆。譜面版權屬原權利人，note 位置與 timing 不保證與官方完全一致。
  </p>

  <div class="index-bar" role="status">
    {#if !session.desktop}
      <span class="xsmall muted row"><Icon name="info" size={14} />瀏覽器預覽不載入譜面網站索引</span>
    {:else}
      {@render indexState('wiki')}
      {@render indexState('adx')}
    {/if}
    <span class="spacer"></span>
    {#if session.desktop}
      <button
        class="btn filter"
        onclick={refreshAll}
        disabled={anyLoading || importing !== null}
        title="忽略快取，重新從 simai Wiki 與 adxdls 抓取索引"
      >
        <Icon name="refresh-cw" size={14} />{!anyIndex && (indexes.wiki.error || indexes.adx.error)
          ? '重試'
          : '重新整理索引'}
      </button>
    {/if}
  </div>
  {#if session.desktop}
    {#each ['wiki', 'adx'] as const as source (source)}
      {@const index = indexes[source]}
      {#if index.error && !index.loading}
        <p class="notice small field-error index-error">
          {SOURCE_LABEL[source]}：{index.info ? `重新整理失敗，繼續使用目前的索引。${index.error}` : index.error}
        </p>
      {/if}
    {/each}
  {/if}

  <form class="search-bar" role="search" onsubmit={runSearch}>
    <label class="sr-only" for="wiki-query">搜尋歌曲名稱、曲師、別名或紀錄名稱</label>
    <input
      id="wiki-query"
      class="input"
      type="search"
      autocomplete="off"
      spellcheck="false"
      maxlength={MAX_QUERY}
      placeholder="歌曲名稱、曲師、別名或紀錄名稱"
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
    <p class="notice field-error">瀏覽器預覽沒有 Rust 核心，只能搜尋本機紀錄，無法搜尋譜面網站或匯入。</p>
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
          {SOURCE_LABEL[failure.source]}「{failure.hit.title}」{typeLabel(failure.hit.chartType)}
          {failure.difficulty.label} 沒有匯入
        </span>
        <span class="spacer"></span>
        <button class="btn btn--icon" onclick={() => (failure = null)} aria-label="關閉匯入錯誤">
          <Icon name="x" size={14} />
        </button>
      </div>
      {#if failure.kind === 'rejected'}
        <p class="xsmall muted">
          {rejectedStatus && STATUS_HINT[rejectedStatus] ? `${STATUS_HINT[rejectedStatus]} ` : ''}盤面與譜面紀錄維持原狀，可以改選別的難度或另一個來源。
        </p>
        {#if failure.response.diagnostics.length > 0}
          <div class="feedback-list scroll">
            <DiagnosticList diagnostics={failure.response.diagnostics} />
          </div>
        {/if}
      {:else}
        <p class="small">{failure.message}</p>
        <p class="xsmall muted">盤面與譜面紀錄維持原狀，可以重試、改選別的難度或另一個來源。</p>
      {/if}
      {#if failure.kind !== 'download'}
        {@const current = failure}
        <div class="row row-wrap">
          <CopyDebugButton input={() => debugInputOf(current)} />
        </div>
      {/if}
    </section>
  {/if}

  <div class="body">
    <div class="results scroll" aria-live="polite">
      {#if submitted.length === 0}
        <p class="state small muted">
          輸入關鍵字後按 Enter，會同時搜尋本機譜面紀錄、simai Wiki 與 adxdls。兩邊都有的歌合成一筆，選一首後在右側選來源與難度載入：譜面交給核心分析成功後才加入譜面紀錄。Standard 與 DX 是不同的譜面，分開列出。
        </p>
      {:else}
        <section class="group" aria-labelledby="wiki-local-title">
          <h3 id="wiki-local-title" class="group-title">
            <span>本機紀錄</span>
            <span class="mono">{localMatches.length}</span>
          </h3>
          {#if localMatches.length === 0}
            <p class="group-empty small muted">
              沒有符合「{submitted}」{typeFilter !== 'all' ? `的 ${typeLabel(typeFilter)} 網站匯入` : '的本機'}紀錄。
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
                        <dd>
                          <span class="badge source-badge source-badge--wiki">simai Wiki</span>
                          Level {record.wiki.level ?? '—'}
                        </dd>
                      {:else if record.adx}
                        <dt>來源</dt>
                        <dd>
                          <span class="badge source-badge source-badge--adx">adxdls</span>
                          定數 {record.adx.level ?? '—'}
                        </dd>
                      {/if}
                      <dt>新增</dt>
                      <dd class="mono">{recordDate(record)}</dd>
                    </dl>
                  </div>
                  <button
                    class="btn"
                    onclick={() => openLocal(record)}
                    disabled={importing !== null || !session.desktop}
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
              <span>譜面網站</span>
              {#if search.kind === 'done'}
                <span class="mono">{search.items.length}</span>
              {/if}
            </h3>
            {#if search.kind === 'waiting'}
              <p class="group-empty small muted row">
                {#if anyLoading}
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
                    disabled={busy || !anyIndex}
                  >
                    <Icon name="refresh-cw" />重試
                  </button>
                </div>
              </div>
            {:else if search.kind === 'done'}
              {#if !search.ready.wiki || !search.ready.adx}
                <p class="group-empty xsmall muted row partial">
                  <Icon name="info" size={14} />{SOURCE_LABEL[search.ready.wiki ? 'adx' : 'wiki']} 的索引還沒有載入，這次只搜尋了
                  {SOURCE_LABEL[search.ready.wiki ? 'wiki' : 'adx']}。
                </p>
              {/if}
              {#if search.items.length === 0}
                <p class="group-empty small muted">
                  譜面網站的索引裡找不到符合「{search.query}」{search.chartType !== 'all'
                    ? `的 ${typeLabel(search.chartType)}`
                    : '的'}歌曲，換個關鍵字試試。
                </p>
              {:else}
                <ul class="list">
                  {#each shownItems as hit (hitKey(hit))}
                    {@const key = hitKey(hit)}
                    <li>
                      <button
                        class="item song"
                        class:is-current={selectedKey === key}
                        aria-pressed={selectedKey === key}
                        onclick={() => (selectedKey = key)}
                      >
                        <span class="info">
                          <span class="title">
                            {hit.title}
                            <span class="badge badge--quiet type-badge">{typeLabel(hit.chartType)}</span>
                            {#if importing?.key === key}
                              <Icon name="loader" size={13} spin />
                            {/if}
                          </span>
                          {#if hit.adx?.artist || hit.wiki?.section}
                            <span class="xsmall muted">{hit.adx?.artist || hit.wiki?.section}</span>
                          {/if}
                          <span class="sources">
                            {#each SOURCES as source (source.id)}
                              {#if hit[source.id]}
                                <span
                                  class={`badge source-badge source-badge--${source.id}`}
                                  class:is-used={sourceOf(hit) === source.id}
                                  title={sourceOf(hit) === source.id ? '目前從這裡載入' : undefined}
                                >
                                  {source.label}
                                </span>
                              {/if}
                            {/each}
                          </span>
                          {@render levelList(hit)}
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
            {/if}
          </section>
        {/if}
      {/if}
    </div>

    {#if session.desktop}
      <section class="preview scroll" aria-labelledby="wiki-preview-title">
        <h3 id="wiki-preview-title" class="group-title"><span>預覽</span></h3>
        {#if selected}
          {@const hit = selected}
          {@const key = hitKey(hit)}
          {@const source = sourceOf(hit)}
          {@const pageUrl = pageUrlOf(hit, source)}
          <dl class="meta preview-meta small">
            <dt>Song</dt>
            <dd class="preview-song">{hit.title}</dd>
            {#if hit.adx?.artist}
              <dt>Artist</dt>
              <dd>{hit.adx.artist}</dd>
            {/if}
            <dt>Type</dt>
            <dd>{typeLabel(hit.chartType)}</dd>
            {#if hit.adx?.genre || hit.wiki?.section}
              <dt>Genre</dt>
              <dd>{hit.adx?.genre || hit.wiki?.section}</dd>
            {/if}
          </dl>
          <div class="source-pick">
            <span id="wiki-source-label" class="xsmall muted">載入來源</span>
            <div class="seg" role="radiogroup" aria-labelledby="wiki-source-label" use:segmented>
              {#each SOURCES as item (item.id)}
                <button
                  class="seg-btn"
                  role="radio"
                  aria-checked={source === item.id}
                  disabled={!hit[item.id] || importing !== null}
                  title={hit[item.id] ? undefined : `${item.label} 沒有這首歌`}
                  onclick={() => choose(item.id)}
                >
                  <span class={`source-dot source-dot--${item.id}`} aria-hidden="true"></span>
                  {item.label}
                </button>
              {/each}
            </div>
          </div>
          <dl class="meta preview-meta small">
            <dt>Page URL</dt>
            <dd class="mono xsmall url">
              <a
                class="url-link"
                href={pageUrl}
                target="_blank"
                rel="noopener noreferrer"
                title={`在瀏覽器開啟 ${SOURCE_LABEL[source]} 歌曲頁`}
                onclick={(event) => void openPageUrl(event, pageUrl)}>{pageUrl}</a
              >
            </dd>
          </dl>
          <table class="difficulties small">
            <thead>
              <tr>
                <th scope="col">Difficulty</th>
                <th scope="col">{source === 'adx' ? '定數' : 'Level'}</th>
                <th scope="col"><span class="sr-only">操作</span></th>
              </tr>
            </thead>
            <tbody>
              {#each rowsOf(hit, source) as item (item.key)}
                {@const current =
                  importing?.key === key && importing.source === source && importing.difficulty.key === item.key}
                {@const local = localOf(hit, source, item.key)}
                {@const failed =
                  failure !== null &&
                  hitKey(failure.hit) === key &&
                  failure.source === source &&
                  failure.difficulty.key === item.key}
                <tr class:is-current={current || failed}>
                  <th scope="row" class="mono">
                    {item.label}
                    {#if item.designer}<span class="designer xsmall muted">{item.designer}</span>{/if}
                  </th>
                  <td class="mono">{item.level ?? '—'}</td>
                  <td class="action">
                    {#if local}
                      <button
                        class="btn"
                        onclick={() => importChart(hit, source, item)}
                        disabled={!canImport}
                        aria-label={`開啟已匯入的 ${hit.title} ${typeLabel(hit.chartType)} ${item.label}`}
                      >
                        <Icon name="external-link" />開啟
                      </button>
                    {:else if !item.available}
                      <button
                        class="btn"
                        disabled
                        title="Wiki 索引沒有這個難度的譜面連結"
                        aria-label={`${hit.title} ${item.label} unavailable`}
                      >
                        unavailable
                      </button>
                    {:else}
                      <button
                        class="btn"
                        onclick={() => importChart(hit, source, item)}
                        disabled={!canImport}
                        aria-label={`從 ${SOURCE_LABEL[source]} 載入 ${hit.title} ${typeLabel(hit.chartType)} ${item.label}`}
                      >
                        {#if current}
                          <Icon name="loader" spin />{importing?.step === 'download' ? '下載中' : '分析中'}
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
            {#if source === 'wiki'}
              unavailable 表示 Wiki 索引沒有該難度的連結；實際能否取得以 Rust 回傳為準。
            {:else}
              定數與譜師取自 adxdls 目錄。每個難度各自成為一筆譜面紀錄。
            {/if}
          </p>
        {:else}
          <p class="group-empty small muted">
            {search.kind === 'done' && search.items.length > 0 ? '在左側選一首歌。' : '搜尋後在左側選一首歌，這裡會列出來源與難度。'}
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

  /* 篩選：未選中是透明膠囊，選中反白（與 .btn 的 aria-pressed 樣式相同）。 */
  .filter {
    min-height: 28px;
    padding: 0 11px;
    font-size: var(--fs-xs);
  }

  .filter:not([aria-pressed='true']) {
    background: transparent;
    color: var(--c-text-dim);
  }

  .filter:not([aria-pressed='true']):hover:not(:disabled) {
    background: var(--c-control);
    color: var(--c-text);
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
    padding: 10px 12px 10px 14px;
    border-bottom: 1px solid transparent;
    border-radius: var(--radius-item);
    transition: background-color 120ms ease;
  }

  /* 搜尋結果列（Claude Design 定案）：圓角 16，不畫分隔線，滑過淡底。 */
  .item:hover {
    background: var(--c-control);
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

  /*
   * 譜源用底色區分：adxdls 混琥珀、simai Wiki 混主色（不新增色相，比例同 .alert）。
   * 目前會從哪邊載入另用字重與框線標示，不只靠顏色。
   */
  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }

  .source-badge {
    min-height: 18px;
    padding: 0 7px;
    border: 1px solid transparent;
    color: var(--c-text);
    font-size: var(--fs-xs);
    font-weight: 400;
  }

  .source-badge--adx {
    background: color-mix(in srgb, var(--c-accent) 14%, var(--c-panel));
  }

  .source-badge--wiki {
    background: color-mix(in srgb, var(--c-primary) 14%, var(--c-panel));
  }

  .source-badge.is-used {
    font-weight: 600;
    color: var(--c-text-strong);
  }

  .source-badge--adx.is-used {
    border-color: color-mix(in srgb, var(--c-accent) 50%, var(--c-panel));
  }

  .source-badge--wiki.is-used {
    border-color: color-mix(in srgb, var(--c-primary) 50%, var(--c-panel));
  }

  .meta .source-badge {
    margin-right: var(--space-1);
  }

  .source-dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .source-dot--adx {
    background: var(--c-accent);
  }

  .source-dot--wiki {
    background: var(--c-primary);
  }

  .partial {
    padding-bottom: 0;
  }

  .source-pick {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-4) var(--space-2);
  }

  .source-pick .seg {
    flex: 1 1 auto;
  }

  .designer {
    display: block;
    font-family: var(--font-sans);
    font-weight: 400;
  }

  .preview-note {
    padding: var(--space-3) var(--space-4) var(--space-4);
  }
</style>
