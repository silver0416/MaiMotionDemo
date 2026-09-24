<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import DownloadError from './DownloadError.svelte';
  import {
    formatBytes,
    formatDuration,
    parseYoutubeId,
    thumbnailUrl,
    videoApi,
    type SearchItem,
    type VideoEntry,
    type VideoTool,
  } from '../lib/video';
  import type { VideoLibrary } from '../state/videoLibrary.svelte';

  interface Props {
    library: VideoLibrary;
    /** 預設搜尋字串（曲名＋難度） */
    query: string;
    /** 目前使用中的 YouTube 影片 */
    currentId?: string;
    onUse: (entry: VideoEntry) => void;
    onLocal: () => void;
  }

  let { library, query, currentId, onUse, onLocal }: Props = $props();

  let text = $state('');
  let edited = $state(false);
  let results = $state<SearchItem[] | null>(null);
  let searching = $state(false);
  let searchError = $state<string | null>(null);

  // 換譜面時搜尋字串跟著換，使用者改過就不動。
  $effect(() => {
    const next = query;
    if (!edited) text = next;
  });

  const ytReady = $derived(!!library.status?.ytDlp);
  const pastedId = $derived(parseYoutubeId(text));

  const TOOL_LABEL: Record<VideoTool, string> = { 'yt-dlp': 'yt-dlp', deno: 'Deno' };

  async function search() {
    if (pastedId) {
      await get({ id: pastedId });
      return;
    }
    if (!text.trim() || searching) return;
    searching = true;
    searchError = null;
    try {
      results = await videoApi.search(text.trim(), 12);
    } catch (error) {
      searchError = typeof error === 'string' ? error : String(error);
      results = null;
    } finally {
      searching = false;
    }
  }

  /** 下載（已下載就直接使用）。 */
  async function get(item: Pick<SearchItem, 'id'> & Partial<SearchItem>) {
    const existing = library.entry(item.id);
    if (existing) {
      onUse(existing);
      return;
    }
    const entry = await library.download(item);
    if (entry) {
      onUse(entry);
      if (results) results = results.map((row) => (row.id === entry.id ? { ...row, downloaded: true } : row));
    }
  }

  function percent(id: string): number | null {
    const progress = library.downloads[id];
    if (!progress?.total || progress.downloaded === null) return null;
    return Math.min(100, Math.round((100 * progress.downloaded) / progress.total));
  }

  function toolPercent(tool: VideoTool): number | null {
    const progress = library.installing[tool];
    if (!progress?.total) return null;
    return Math.min(100, Math.round((100 * progress.downloaded) / progress.total));
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      void search();
    }
  }
</script>

<div class="finder">
  {#if !library.desktop}
    <section class="section stack-sm">
      <p class="small">瀏覽器預覽不能搜尋或下載 YouTube 影片，只能開啟本機影片。</p>
      <button class="btn" onclick={onLocal}><Icon name="folder-open" size={14} />開啟本機影片</button>
    </section>
  {:else}
    <section class="section stack-sm" aria-labelledby="finder-tools">
      <div class="section-title" id="finder-tools">
        下載工具
        <button
          class="btn btn--ghost btn--icon"
          onclick={() => void library.refreshStatus(true)}
          disabled={library.checking}
          aria-label="重新偵測"
          title="重新偵測"
        >
          <Icon name={library.checking ? 'loader' : 'refresh-cw'} spin={library.checking} size={14} />
        </button>
      </div>
      {#if !library.status}
        <p class="small muted">偵測中…</p>
      {:else}
        <div class="tool">
          <div class="tool-text">
            <span class="small">yt-dlp</span>
            <span class="xsmall muted mono">
              {library.status.ytDlp ? `${library.status.ytDlp.version}${library.status.ytDlp.managed ? '' : '（系統）'}` : '尚未下載'}
            </span>
          </div>
          {#if !library.status.ytDlp || !library.status.ytDlp.managed}
            <button
              class="btn"
              class:btn--primary={!library.status.ytDlp}
              onclick={() => void library.install('yt-dlp')}
              disabled={!!library.installing['yt-dlp'] || !library.status.installable}
              title={library.status.ytDlp ? '系統上的 yt-dlp 可能太舊；下載最新版後改用它' : ''}
            >
              <Icon name={library.installing['yt-dlp'] ? 'loader' : 'download'} spin={!!library.installing['yt-dlp']} size={14} />
              {library.status.ytDlp ? '下載最新版' : '下載（約 18 MB）'}
            </button>
          {/if}
        </div>
        <div class="tool">
          <div class="tool-text">
            <span class="small">JS 執行環境</span>
            <span class="xsmall muted mono">
              {library.status.runtime
                ? `${library.status.runtime.kind === 'deno' ? 'Deno' : 'Node.js'} ${library.status.runtime.version}${library.status.runtime.managed ? '' : '（系統）'}`
                : '未偵測到'}
            </span>
          </div>
          {#if !library.status.runtime}
            <button class="btn" onclick={() => void library.install('deno')} disabled={!!library.installing.deno || !library.status.installable}>
              <Icon name={library.installing.deno ? 'loader' : 'download'} spin={!!library.installing.deno} size={14} />下載 Deno（約 45 MB）
            </button>
          {/if}
        </div>
        {#if !library.status.runtime}
          <p class="xsmall muted">YouTube 要求用 JS 執行環境解題，沒有的話很多影片會下載失敗。</p>
        {/if}
        {#each Object.keys(library.installing) as tool (tool)}
          {@const value = toolPercent(tool as VideoTool)}
          <div class="progress" role="progressbar" aria-label={`下載 ${TOOL_LABEL[tool as VideoTool]}`} aria-valuenow={value ?? undefined} aria-valuemin={0} aria-valuemax={100}>
            <div class="progress-fill" style={`width:${value ?? 0}%`}></div>
          </div>
          <p class="xsmall muted">正在下載 {TOOL_LABEL[tool as VideoTool]}{value !== null ? ` ${value}%` : '…'}</p>
        {/each}
        {#if library.toolError}
          <div class="alert alert--error" role="alert">{library.toolError}</div>
        {/if}
      {/if}
    </section>

    <section class="section stack-sm" aria-labelledby="finder-search">
      <div class="section-title" id="finder-search">搜尋 YouTube</div>
      <div class="row">
        <input
          class="input"
          type="search"
          placeholder="曲名、難度，或貼上影片網址"
          bind:value={text}
          oninput={() => (edited = true)}
          onkeydown={onKey}
          aria-label="搜尋文字"
        />
        <button class="btn btn--primary" onclick={() => void search()} disabled={!ytReady || searching || !text.trim()}>
          <Icon name={searching ? 'loader' : pastedId ? 'download' : 'search'} spin={searching} size={14} />{pastedId ? '下載' : '搜尋'}
        </button>
      </div>
      {#if edited && text !== query && query}
        <button class="linkish xsmall" onclick={() => ((text = query), (edited = false))}>改回預設：{query}</button>
      {/if}
      {#if !ytReady}
        <p class="xsmall muted">先下載 yt-dlp 才能搜尋。</p>
      {/if}
      {#if searchError}
        <div class="alert alert--error" role="alert">{searchError}</div>
      {/if}
      {#if pastedId && library.downloads[pastedId]}
        {@const value = percent(pastedId)}
        <div class="progress" role="progressbar" aria-label="下載進度" aria-valuenow={value ?? undefined} aria-valuemin={0} aria-valuemax={100}>
          <div class="progress-fill" style={`width:${value ?? 0}%`}></div>
        </div>
      {/if}
      {#if pastedId}
        <DownloadError {library} id={pastedId} />
      {/if}
    </section>

    {#if results}
      <section class="results" aria-label="搜尋結果">
        {#if results.length === 0}
          <p class="small muted empty">找不到影片，換個關鍵字試試（例如加上「直撮り」）。</p>
        {/if}
        {#each results as item (item.id)}
          {@const busy = !!library.downloads[item.id]}
          {@const value = percent(item.id)}
          {@const have = !!library.entry(item.id)}
          <article class="result" class:is-current={item.id === currentId}>
            <img class="thumb" src={thumbnailUrl(item.id)} alt="" loading="lazy" width="120" height="68" />
            <div class="result-text">
              <div class="result-title small" title={item.title}>{item.title}</div>
              <div class="xsmall muted">
                {item.channel}{item.duration ? `・${formatDuration(item.duration)}` : ''}
              </div>
              {#if busy}
                <div class="progress" role="progressbar" aria-label="下載進度" aria-valuenow={value ?? undefined} aria-valuemin={0} aria-valuemax={100}>
                  <div class="progress-fill" style={`width:${value ?? 0}%`}></div>
                </div>
              {/if}
              <DownloadError {library} id={item.id} />
            </div>
            <div class="result-actions">
              {#if item.id === currentId}
                <span class="badge">使用中</span>
              {:else if busy}
                <span class="xsmall mono muted">{value !== null ? `${value}%` : '準備中'}</span>
                <button class="btn btn--icon" onclick={() => void library.cancel(item.id)} aria-label="取消下載" title="取消下載">
                  <Icon name="x" size={14} />
                </button>
              {:else}
                <button class="btn" onclick={() => void get(item)}>
                  {#if have}使用{:else}<Icon name="download" size={14} />下載{/if}
                </button>
              {/if}
            </div>
          </article>
        {/each}
      </section>
    {/if}

    <section class="section stack-sm" aria-labelledby="finder-library">
      <div class="section-title" id="finder-library">
        已下載
        {#if library.cache}<span class="xsmall mono">{formatBytes(library.cache.bytes)}</span>{/if}
      </div>
      {#if library.entries.length === 0}
        <p class="small muted">還沒有下載的影片。可以在設定裡清除已下載的影片。</p>
      {:else}
        <ul class="library">
          {#each library.entries as entry (entry.id)}
            <li class="library-item">
              <div class="result-text">
                <div class="result-title small" title={entry.title}>{entry.title}</div>
                <div class="xsmall muted mono">
                  {formatDuration(entry.duration)}{entry.height ? `・${entry.height}p` : ''}{entry.fps ? `${Math.round(entry.fps)}` : ''}・{formatBytes(entry.bytes)}
                </div>
              </div>
              {#if entry.id === currentId}
                <span class="badge">使用中</span>
              {:else}
                <button class="btn" onclick={() => onUse(entry)}>使用</button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="section">
      <button class="btn" onclick={onLocal}><Icon name="folder-open" size={14} />開啟本機影片</button>
    </section>
  {/if}
</div>

<style>
  .finder {
    display: flex;
    flex-direction: column;
  }

  .tool {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .tool-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: var(--lh-tight);
  }

  .progress {
    height: 4px;
    background: var(--c-control);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--c-right);
  }

  .results {
    border-bottom: 1px solid var(--c-border);
  }

  .empty {
    padding: var(--space-4);
  }

  .result {
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .result:last-child {
    border-bottom: none;
  }

  .result.is-current {
    background: var(--c-control);
  }

  .thumb {
    width: 96px;
    height: 54px;
    object-fit: cover;
    border-radius: var(--radius-sm);
    background: var(--c-control);
  }

  .result-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    line-height: var(--lh-tight);
  }

  .result-title {
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .result-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .library {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: 0;
    list-style: none;
  }

  .library-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .linkish {
    align-self: flex-start;
    padding: 0;
    background: none;
    border: none;
    color: var(--c-right);
    text-align: left;
    cursor: pointer;
  }
</style>
