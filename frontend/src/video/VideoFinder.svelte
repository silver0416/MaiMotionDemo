<script lang="ts">
  import ContextMenu, { type MenuItem } from '../components/ContextMenu.svelte';
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
    /** 主視窗開著的譜面紀錄；下載或選用的影片歸到這份譜面。 */
    recordId: string | null;
    onUse: (entry: VideoEntry) => void;
    onLocal: () => void;
  }

  let { library, query, currentId, recordId, onUse, onLocal }: Props = $props();

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
      library.assign(existing.id, recordId);
      onUse(existing);
      return;
    }
    const entry = await library.download(item, recordId);
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

  // ---- 已下載：預設只列這份譜面的影片，打開開關才列全部。
  const SHOW_ALL_KEY = 'maimotion.video-show-all.v1';

  function loadShowAll(): boolean {
    try {
      return localStorage.getItem(SHOW_ALL_KEY) === '1';
    } catch {
      return false;
    }
  }

  let showAll = $state(loadShowAll());

  function setShowAll(value: boolean) {
    showAll = value;
    try {
      localStorage.setItem(SHOW_ALL_KEY, value ? '1' : '0');
    } catch {
      // 存不了只影響下次開啟。
    }
  }

  /** 屬於這份譜面（或正在使用）的影片。 */
  const mine = $derived(
    library.entries.filter((entry) => entry.id === currentId || library.ownedBy(entry.id, recordId)),
  );
  const listed = $derived(showAll ? library.entries : mine);

  let menu = $state<{ entry: VideoEntry; x: number; y: number } | null>(null);
  let deleting = $state<string | null>(null);
  let deleteError = $state<string | null>(null);

  const menuItems = $derived.by<MenuItem[]>(() => {
    const entry = menu?.entry;
    if (!entry) return [];
    const current = entry.id === currentId;
    const owned = library.ownedBy(entry.id, recordId);
    const items: MenuItem[] = [];
    if (!current) items.push({ id: 'use', label: '使用這部影片', icon: 'play' });
    if (recordId) {
      items.push(
        owned
          ? { id: 'unassign', label: '不再歸在這份譜面', icon: 'x' }
          : { id: 'assign', label: '歸到這份譜面', icon: 'bookmark-plus' },
      );
    }
    items.push({
      id: 'delete',
      label: current ? '刪除影片（使用中，先換別部）' : '刪除影片',
      icon: 'trash',
      danger: true,
      disabled: current || deleting !== null,
      divider: true,
    });
    return items;
  });

  function openMenu(entry: VideoEntry, event: MouseEvent) {
    event.preventDefault();
    menu = { entry, x: event.clientX, y: event.clientY };
  }

  function openMenuFrom(entry: VideoEntry, event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { entry, x: rect.right, y: rect.bottom };
  }

  function useFromLibrary(entry: VideoEntry) {
    library.assign(entry.id, recordId);
    onUse(entry);
  }

  async function selectMenu(id: string) {
    const entry = menu?.entry;
    menu = null;
    if (!entry) return;
    if (id === 'use') {
      useFromLibrary(entry);
    } else if (id === 'assign') {
      library.assign(entry.id, recordId);
    } else if (id === 'unassign') {
      library.unassign(entry.id, recordId);
    } else if (id === 'delete') {
      deleting = entry.id;
      deleteError = null;
      const error = await library.remove(entry.id);
      deleting = null;
      if (error) deleteError = `無法刪除「${entry.title}」：${error}`;
    }
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
      <label class="check small">
        <input type="checkbox" checked={showAll} onchange={(event) => setShowAll(event.currentTarget.checked)} />
        <span>顯示所有影片<span class="muted mono">（{library.entries.length}）</span></span>
      </label>
      {#if deleteError}
        <div class="alert alert--error" role="alert">{deleteError}</div>
      {/if}
      {#if listed.length === 0}
        <p class="small muted">
          {library.entries.length === 0
            ? '還沒有下載的影片。'
            : '這份譜面還沒有影片。在這份譜面開著時下載或選用的影片會自動歸到這裡；打開「顯示所有影片」可以看其他譜面的影片。'}
        </p>
      {:else}
        <ul class="library" aria-label="已下載的影片">
          {#each listed as entry (entry.id)}
            {@const owned = entry.id === currentId || library.ownedBy(entry.id, recordId)}
            <li
              class="library-item"
              class:is-current={entry.id === currentId}
              class:is-deleting={deleting === entry.id}
              oncontextmenu={(event) => openMenu(entry, event)}
            >
              <div class="result-text">
                <div class="result-title small" title={entry.title}>{entry.title}</div>
                <div class="xsmall muted mono">
                  {formatDuration(entry.duration)}{entry.height ? `・${entry.height}p` : ''}{entry.fps ? `${Math.round(entry.fps)}` : ''}・{formatBytes(entry.bytes)}
                  {#if showAll && owned && entry.id !== currentId}<span class="badge badge--quiet">這份譜面</span>{/if}
                </div>
              </div>
              {#if entry.id === currentId}
                <span class="badge">使用中</span>
              {:else if deleting === entry.id}
                <Icon name="loader" spin size={14} />
              {:else}
                <button class="btn" onclick={() => useFromLibrary(entry)}>使用</button>
              {/if}
              <button
                class="btn btn--ghost btn--icon"
                onclick={(event) => openMenuFrom(entry, event)}
                aria-label={`${entry.title} 的更多動作`}
                title="更多動作（也可以按右鍵）"
              >
                <Icon name="more-horizontal" size={14} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if menu}
        <ContextMenu
          items={menuItems}
          x={menu.x}
          y={menu.y}
          align="end"
          label={`「${menu.entry.title}」的動作`}
          onSelect={(id) => void selectMenu(id)}
          onClose={() => (menu = null)}
        />
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
    gap: var(--space-2);
    margin: 0 calc(-1 * var(--space-2));
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  .library-item > .result-text {
    flex: 1;
  }

  .library-item:hover,
  .library-item.is-current {
    background: var(--c-control);
  }

  .library-item.is-deleting {
    opacity: 0.5;
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
