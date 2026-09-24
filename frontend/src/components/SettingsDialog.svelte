<script lang="ts">
  import Icon from './Icon.svelte';
  import { appVersion, clearWikiCache, wikiCacheInfo, type WikiCacheInfo } from '../lib/api';
  import { BUILD_ID, BUILD_TIME, FRONTEND_VERSION } from '../lib/build';
  import { SCHEMA_VERSION, SCORING_LABEL, SCORING_MODELS } from '../lib/contract';
  import { STORE_ANALYSES, STORE_RECORDS, STORE_SETTINGS, dbClear, dbCount } from '../lib/db';
  import { copyText } from '../lib/debug';
  import { reducedMotion } from '../lib/press';
  import { FAILURE_KIND_LABEL, errorLog } from '../state/errorLog.svelte';
  import { DISTRIBUTION_LABEL, updateState } from '../state/update.svelte';
  import { cacheEvents } from '../state/cacheEvents.svelte';
  import { records } from '../state/records.svelte';
  import { SOLVER_REVISION, session } from '../state/session.svelte';
  import { toasts } from '../state/toasts.svelte';
  import { VideoLibrary } from '../state/videoLibrary.svelte';
  import { formatDuration } from '../lib/video';

  interface Props {
    open: boolean;
  }

  let { open = $bindable() }: Props = $props();

  let dialog: HTMLDialogElement | null = $state(null);
  let closeButton: HTMLButtonElement | null = $state(null);
  let version = $state('');
  /** 分析快取筆數；null = 讀不到資料庫。 */
  let cacheCount = $state<number | null>(null);
  /** 桌面核心保存的 simai Wiki 快取；瀏覽器預覽或讀取失敗時為 null。 */
  let wikiCache = $state<WikiCacheInfo | null>(null);
  /** 危險操作要按兩次：第一次只切成確認狀態。 */
  let confirming = $state<'cache' | 'all' | 'videos' | null>(null);
  /** 影片同步用的工具與已下載影片（只有桌面版）。 */
  const videos = new VideoLibrary();
  let working = $state(false);

  let closing = $state(false);
  let pressedOutside = false;
  const CLOSE_MS = 140;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!dialog) return;
    if (open) {
      clearTimeout(closeTimer);
      closing = false;
      if (!dialog.open) {
        confirming = null;
        void refresh();
        dialog.showModal();
        closeButton?.focus();
      }
    } else if (dialog.open && !closing) {
      if (reducedMotion()) {
        dialog.close();
        return;
      }
      closing = true;
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

  async function refresh() {
    version = await appVersion();
    await updateState.init();
    cacheCount = await dbCount(STORE_ANALYSES);
    wikiCache = session.desktop ? await wikiCacheInfo().catch(() => null) : null;
    if (session.desktop) {
      await videos.init();
      await Promise.all([videos.refreshList(), videos.refreshStatus(false)]);
    }
  }

  function videoResult(error: string | null, done: string) {
    toasts.show({
      id: 'settings-video',
      tone: error ? 'error' : 'ok',
      title: error ? '無法刪除影片' : done,
      ...(error ? { body: error } : {}),
    });
  }

  async function removeVideo(id: string) {
    videoResult(await videos.remove(id), '已刪除影片');
  }

  async function clearVideos() {
    if (confirming !== 'videos') {
      confirming = 'videos';
      return;
    }
    working = true;
    const error = await videos.clear();
    working = false;
    confirming = null;
    videoResult(error, '已清除所有下載的影片');
  }

  async function updateTool(tool: 'yt-dlp' | 'deno') {
    const ok = await videos.install(tool);
    toasts.show({
      id: 'settings-video',
      tone: ok ? 'ok' : 'error',
      title: ok ? `${tool === 'deno' ? 'Deno' : 'yt-dlp'} 已是最新版` : '下載失敗',
      ...(ok ? {} : { body: videos.toolError ?? '' }),
    });
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  const hasCache = $derived((cacheCount ?? 0) > 0 || (wikiCache?.files ?? 0) > 0);

  /** 清除 Wiki 磁碟快取；瀏覽器預覽沒有核心，直接視為成功。回傳錯誤訊息或 null。 */
  async function clearWiki(): Promise<string | null> {
    if (!session.desktop) return null;
    try {
      await clearWikiCache();
      cacheEvents.wikiGeneration += 1;
      return null;
    } catch (error) {
      return typeof error === 'string' ? error : String(error);
    }
  }

  function formatTime(epoch: number): string {
    const date = new Date(epoch);
    const pad = (value: number) => String(value).padStart(2, '0');
    return (
      `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ` +
      `${pad(date.getHours())}:${pad(date.getMinutes())}`
    );
  }

  const buildTime = $derived.by(() => {
    const time = Date.parse(BUILD_TIME);
    return Number.isNaN(time) ? BUILD_TIME : formatTime(time);
  });

  function versionText(): string {
    return [
      `App 版本：${session.desktop ? version : `${FRONTEND_VERSION}（瀏覽器預覽）`}`,
      `應用程式種類：${DISTRIBUTION_LABEL[updateState.distribution]}`,
      `Build：${BUILD_ID}（${BUILD_TIME}）`,
      `判定規則修訂：${SOLVER_REVISION}`,
      `評分方式：${SCORING_MODELS.map((model) => `${SCORING_LABEL[model.id]} ${model.version}（${model.id}，schema ${SCHEMA_VERSION[model.id]}）`).join('、')}`,
    ].join('\n');
  }

  async function copyVersion() {
    const ok = await copyText(versionText());
    toasts.show({
      id: 'settings-copy',
      tone: ok ? 'ok' : 'error',
      title: ok ? '已複製版本資訊' : '無法寫入剪貼簿',
    });
  }

  /** 設定頁手動檢查更新；結果直接顯示在更新區塊，不用 toast。 */
  async function checkUpdateManually() {
    await updateState.init();
    await updateState.check();
  }

  async function openUpdateDownload() {
    await updateState.openDownload();
  }

  async function copyAndClear() {
    const count = errorLog.entries.length;
    if (count === 0) return;
    const ok = await copyText(errorLog.buildReport());
    if (!ok) {
      // 沒複製成功就不清，避免紀錄憑空消失。
      toasts.show({
        id: 'settings-copy',
        tone: 'error',
        title: '無法寫入剪貼簿',
        body: '錯誤紀錄沒有清除，請再試一次。',
      });
      return;
    }
    await errorLog.markReported();
    toasts.show({
      id: 'settings-copy',
      tone: 'ok',
      title: `已複製 ${count} 筆錯誤紀錄`,
      body: '紀錄已清除；同一份建置再發生相同錯誤不會重複記錄。',
    });
  }

  async function clearCache() {
    if (confirming !== 'cache') {
      confirming = 'cache';
      return;
    }
    working = true;
    const ok = await dbClear(STORE_ANALYSES);
    const wikiError = await clearWiki();
    working = false;
    confirming = null;
    await refresh();
    const problems = [ok ? null : '分析快取：無法存取本機資料庫。', wikiError && `Wiki 快取：${wikiError}`].filter(
      (item): item is string => Boolean(item),
    );
    toasts.show({
      id: 'settings-clear',
      tone: problems.length === 0 ? 'ok' : 'error',
      title: problems.length === 0 ? '已清除暫存資料' : '部分暫存沒有清除',
      body:
        problems.length === 0
          ? '下次開啟譜面會重新分析，simai Wiki 索引會重新下載。'
          : problems.join(' '),
    });
  }

  async function clearAll() {
    if (confirming !== 'all') {
      confirming = 'all';
      return;
    }
    working = true;
    const ok = await dbClear(STORE_RECORDS, STORE_SETTINGS, STORE_ANALYSES);
    // Wiki 快取不是使用者資料，清不掉也不擋住重設；下次清除暫存可再試。
    await clearWiki();
    try {
      const keys: string[] = [];
      for (let index = 0; index < localStorage.length; index += 1) {
        const key = localStorage.key(index);
        if (key?.startsWith('maimotion.')) keys.push(key);
      }
      for (const key of keys) localStorage.removeItem(key);
    } catch {
      // localStorage 只存欄寬與搜尋分頁，清不掉不影響資料。
    }
    if (!ok) {
      working = false;
      confirming = null;
      toasts.show({ id: 'settings-clear', tone: 'error', title: '清除失敗', body: '無法存取本機資料庫。' });
      return;
    }
    records.reset();
    errorLog.reset();
    session.clearResult();
    // 重新載入讓所有狀態回到初次啟動，不會被記憶體中的設定寫回資料庫。
    window.location.reload();
  }
</script>

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="settings-title"
  onclose={() => (open = false)}
  oncancel={(event) => {
    event.preventDefault();
    open = false;
  }}
  onkeydown={(event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (confirming) confirming = null;
      else open = false;
    }
  }}
  onpointerdown={(event) => (pressedOutside = isOutside(event))}
  onclick={(event) => {
    if (pressedOutside && isOutside(event) && !working) open = false;
    pressedOutside = false;
  }}
>
  <header class="dialog-head">
    <h2 id="settings-title">設定</h2>
    <span class="spacer"></span>
    <button bind:this={closeButton} class="btn btn--icon" onclick={() => (open = false)} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div class="dialog-body scroll">
    <section class="block" aria-labelledby="settings-version">
      <div class="block-head">
        <h3 id="settings-version">版本資訊</h3>
        <button class="btn" onclick={copyVersion}><Icon name="copy" />複製</button>
      </div>
      <dl class="kv info">
        <dt>App 版本</dt>
        <dd>
          {#if session.desktop}
            {version || '讀取中…'}
          {:else}
            {FRONTEND_VERSION} <span class="badge badge--sample">瀏覽器預覽</span>
          {/if}
        </dd>
        <dt>Build 號</dt>
        <dd>{BUILD_ID}</dd>
        <dt>應用程式種類</dt>
        <dd>{DISTRIBUTION_LABEL[updateState.distribution]}</dd>
        <dt>建置時間</dt>
        <dd>{buildTime}</dd>
        <dt>判定規則修訂</dt>
        <dd>{SOLVER_REVISION}</dd>
      </dl>

      <table class="table models">
        <caption class="sr-only">評分方式版本</caption>
        <thead>
          <tr>
            <th scope="col">評分方式</th>
            <th scope="col">版本</th>
            <th scope="col">識別碼</th>
            <th scope="col">Schema</th>
          </tr>
        </thead>
        <tbody>
          {#each SCORING_MODELS as model (model.id)}
            <tr>
              <td>
                {SCORING_LABEL[model.id]}
                {#if session.config.scoringModel === model.id}
                  <span class="badge badge--quiet">使用中</span>
                {/if}
              </td>
              <td class="mono">{model.version}</td>
              <td class="mono">{model.id}</td>
              <td class="mono">{SCHEMA_VERSION[model.id]}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>

    <section class="block" aria-labelledby="settings-update">
      <div class="block-head">
        <h3 id="settings-update">軟體更新</h3>
        {#if session.desktop}
          <button class="btn" onclick={checkUpdateManually} disabled={updateState.checking}>
            <Icon name={updateState.checking ? 'loader' : 'refresh-cw'} spin={updateState.checking} />
            {updateState.checking ? '檢查中…' : '檢查更新'}
          </button>
        {/if}
      </div>
      {#if !session.desktop}
        <p class="field-hint">瀏覽器預覽沒有版本檢查，請用桌面版查看更新。</p>
      {:else if updateState.checking && !updateState.result}
        <p class="field-hint">正在向 GitHub 查詢最新版本…</p>
      {:else if updateState.result?.hasUpdate}
        <p class="update-available">
          有新版本 <span class="mono">v{updateState.result.latest}</span>
         （目前 <span class="mono">v{updateState.result.current}</span>）
        </p>
        {#if updateState.canSelfUpdate}
          {#if updateState.downloaded}
            <p class="field-hint">
              已下載到 {updateState.downloaded.fallback ? '「下載」資料夾' : '目前程式所在的資料夾'}（<span class="mono">{updateState.downloaded.name}</span>）。
              重新啟動就會換成新版，舊版可以在新版開啟後刪除。
            </p>
          {:else if updateState.downloading}
            <div class="update-progress" role="progressbar" aria-label="下載更新" aria-valuenow={updateState.percent} aria-valuemin={0} aria-valuemax={100}>
              <div class="update-progress-fill" style={`width:${updateState.percent}%`}></div>
            </div>
            <p class="field-hint mono">{updateState.percent}%</p>
          {:else}
            <p class="field-hint">會下載到目前程式所在的資料夾，完成後重新啟動就換成新版。</p>
          {/if}
          {#if updateState.downloadError}
            <p class="field-error">{updateState.downloadError}</p>
          {/if}
          <div class="data-actions">
            {#if updateState.downloaded}
              <button class="btn btn--primary" onclick={() => void updateState.restart()} disabled={updateState.restarting}>
                <Icon name="refresh-cw" />重新啟動並更新
              </button>
            {:else}
              <button class="btn btn--primary" onclick={() => void updateState.download()} disabled={updateState.downloading}>
                <Icon name={updateState.downloading ? 'loader' : 'download'} spin={updateState.downloading} />
                {updateState.downloading ? '下載中…' : `下載並更新 v${updateState.result.latest}`}
              </button>
            {/if}
            <button class="btn" onclick={openUpdateDownload}>
              <Icon name="external-link" />在 GitHub 查看
            </button>
          </div>
        {:else}
          {#if updateState.distribution === 'installed'}
            <p class="field-hint">安裝版未來會支援自動更新；目前請先到 GitHub 下載新版安裝。</p>
          {:else}
            <p class="field-hint">請到 GitHub 下載新版 exe 取代舊檔即可。</p>
          {/if}
          <div class="data-actions">
            <button class="btn btn--primary" onclick={openUpdateDownload}>
              <Icon name="download" />前往下載 v{updateState.result.latest}
            </button>
          </div>
        {/if}
      {:else if updateState.result}
        <p class="field-hint">
          已是最新版本（<span class="mono">v{updateState.result.current}</span>）。
          {#if updateState.lastChecked}
            <span class="muted">上次檢查：{formatTime(updateState.lastChecked)}</span>
          {/if}
        </p>
      {:else if updateState.error}
        <p class="field-error">{updateState.error}</p>
        <p class="field-hint">離線或 GitHub 忙碌時會檢查失敗，不影響使用，稍後再試即可。</p>
      {:else}
        <p class="field-hint">
          尚未檢查。每次開啟都會自動檢查一次，也可隨時按「檢查更新」。
        </p>
      {/if}
    </section>

    <section class="block" aria-labelledby="settings-errors">
      <div class="block-head">
        <h3 id="settings-errors">
          錯誤紀錄
          <span class="muted mono">{errorLog.entries.length}</span>
        </h3>
        <button class="btn btn--primary" onclick={copyAndClear} disabled={errorLog.entries.length === 0}>
          <Icon name="copy" />複製並清除
        </button>
      </div>
      <p class="field-hint">
        譜面匯入失敗（下載失敗、呼叫核心失敗、語法錯誤或未支援）會自動記在這裡，相同的錯誤只記一筆並累計次數。
        複製後貼給開發者，紀錄會自動清除，避免下次重複回報。
      </p>

      {#if errorLog.entries.length === 0}
        <p class="empty small muted">目前沒有錯誤紀錄。</p>
      {:else}
        <ul class="errors scroll">
          {#each errorLog.entries as entry (entry.fingerprint)}
            <li class="error">
              <div class="error-head">
                <span class="badge badge--quiet">{FAILURE_KIND_LABEL[entry.kind]}</span>
                <span class="error-title">{entry.origin}・{entry.title}</span>
              </div>
              <p class="error-summary small">{entry.summary}</p>
              <p class="xsmall muted mono">
                {formatTime(entry.lastAt)}{entry.count > 1 ? `・共 ${entry.count} 次` : ''}
              </p>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if session.desktop}
      <section class="block" aria-labelledby="settings-video">
        <div class="block-head">
          <h3 id="settings-video">影片同步</h3>
        </div>

        <div class="data-row">
          <div class="data-text">
            <div class="data-title">yt-dlp</div>
            <p class="xsmall muted mono">
              {#if videos.status?.ytDlp}
                {videos.status.ytDlp.version}{videos.status.ytDlp.managed ? '' : '（系統安裝，不由本程式更新）'}
              {:else if videos.status}
                尚未下載
              {:else}
                偵測中…
              {/if}
            </p>
            <p class="field-hint">搜尋與下載 YouTube 影片用。YouTube 改版後舊版常會失效，下載失敗時先更新。</p>
          </div>
          <div class="data-actions">
            <button class="btn" onclick={() => void updateTool('yt-dlp')} disabled={!!videos.installing['yt-dlp'] || !videos.status?.installable}>
              <Icon name={videos.installing['yt-dlp'] ? 'loader' : 'download'} spin={!!videos.installing['yt-dlp']} />
              {videos.status?.ytDlp?.managed ? '更新' : '下載'}
            </button>
          </div>
        </div>

        <div class="data-row">
          <div class="data-text">
            <div class="data-title">JS 執行環境</div>
            <p class="xsmall muted mono">
              {#if videos.status?.runtime}
                {videos.status.runtime.kind === 'deno' ? 'Deno' : 'Node.js'} {videos.status.runtime.version}{videos.status.runtime.managed ? '' : '（系統安裝）'}
              {:else if videos.status}
                未偵測到
              {:else}
                偵測中…
              {/if}
            </p>
            <p class="field-hint">yt-dlp 需要它才能通過 YouTube 的檢查。已安裝 Node.js 或 Deno 就不用另外下載。</p>
          </div>
          <div class="data-actions">
            {#if !videos.status?.runtime || videos.status.runtime.managed}
              <button class="btn" onclick={() => void updateTool('deno')} disabled={!!videos.installing.deno || !videos.status?.installable}>
                <Icon name={videos.installing.deno ? 'loader' : 'download'} spin={!!videos.installing.deno} />
                {videos.status?.runtime?.managed ? '更新 Deno' : '下載 Deno'}
              </button>
            {/if}
          </div>
        </div>

        <div class="data-row" class:is-confirming={confirming === 'videos'}>
          <div class="data-text">
            <div class="data-title">已下載的影片</div>
            <p class="xsmall muted mono usage">
              <span>{videos.entries.length} 部・{formatBytes(videos.cache?.bytes ?? 0)}</span>
            </p>
            <p class="field-hint">刪除後標註裡的影片連結與對齊都會保留，需要時再下載一次即可。</p>
            {#if confirming === 'videos'}
              <p class="field-error">再按一次「確定清除」才會刪除全部影片。</p>
            {/if}
          </div>
          <div class="data-actions">
            {#if confirming === 'videos'}
              <button class="btn" onclick={() => (confirming = null)} disabled={working}>取消</button>
            {/if}
            <button
              class="btn"
              class:btn--danger={confirming === 'videos'}
              onclick={clearVideos}
              disabled={working || (confirming !== 'videos' && (videos.cache?.files ?? 0) === 0)}
            >
              <Icon name="trash" />{confirming === 'videos' ? '確定清除' : '全部清除'}
            </button>
          </div>
        </div>

        {#if videos.entries.length > 0}
          <ul class="video-list">
            {#each videos.entries as entry (entry.id)}
              <li class="video-item">
                <div class="data-text">
                  <span class="small video-title" title={entry.title}>{entry.title}</span>
                  <span class="xsmall muted mono">
                    {formatDuration(entry.duration)}{entry.height ? `・${entry.height}p` : ''}・{formatBytes(entry.bytes)}
                  </span>
                </div>
                <button class="btn btn--icon" onclick={() => void removeVideo(entry.id)} aria-label={`刪除 ${entry.title}`} title="刪除這部影片">
                  <Icon name="trash" />
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <section class="block" aria-labelledby="settings-data">
      <div class="block-head">
        <h3 id="settings-data">資料管理</h3>
      </div>

      <div class="data-row">
        <div class="data-text">
          <div class="data-title">
            清除暫存資料
          </div>
          <p class="xsmall muted mono usage">
            {#if cacheCount !== null}<span>分析快取 {cacheCount} 筆</span>{/if}
            {#if wikiCache}
              <span>simai Wiki 快取 {wikiCache.files} 個檔案・{formatBytes(wikiCache.bytes)}</span>
            {/if}
          </p>
          <p class="field-hint">
            刪除已保存的分析結果{session.desktop ? '，以及 simai Wiki 的索引與歌曲頁快取' : ''}。
            譜面紀錄、參數與顯示設定都會保留；下次開啟譜面時重新分析{session.desktop ? '，搜尋 Wiki 時重新下載索引' : ''}。
          </p>
        </div>
        <div class="data-actions">
          {#if confirming === 'cache'}
            <button class="btn" onclick={() => (confirming = null)} disabled={working}>取消</button>
          {/if}
          <button
            class="btn"
            class:btn--danger={confirming === 'cache'}
            onclick={clearCache}
            disabled={working || (confirming !== 'cache' && !hasCache)}
          >
            <Icon name="database" />{confirming === 'cache' ? '確定清除' : '清除暫存'}
          </button>
        </div>
      </div>

      <div class="data-row" class:is-confirming={confirming === 'all'}>
        <div class="data-text">
          <div class="data-title">清除所有使用者資料</div>
          <p class="field-hint">
            刪除全部譜面紀錄（{records.items.length} 筆）、參數與顯示設定、錯誤紀錄、分析與 Wiki 快取、欄寬等介面偏好，
            回到第一次啟動的狀態。無法復原。
          </p>
          {#if confirming === 'all'}
            <p class="field-error">再按一次「確定全部清除」才會執行，應用程式會重新載入。</p>
          {/if}
        </div>
        <div class="data-actions">
          {#if confirming === 'all'}
            <button class="btn" onclick={() => (confirming = null)} disabled={working}>取消</button>
          {/if}
          <button class="btn btn--danger" onclick={clearAll} disabled={working}>
            <Icon name={working && confirming === 'all' ? 'loader' : 'trash'} spin={working && confirming === 'all'} />
            {confirming === 'all' ? '確定全部清除' : '清除全部'}
          </button>
        </div>
      </div>
    </section>
  </div>
</dialog>

<style>
  .dialog {
    width: min(680px, calc(100vw - 32px));
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

  .dialog::backdrop {
    background: none;
  }

  .dialog-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .dialog-body {
    min-height: 0;
  }

  /* 區塊之間只用分隔線，不再加一層外框。 */
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .block:last-child {
    border-bottom: none;
  }

  .block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 30px;
  }

  .block-head h3 {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .info dd {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .models td {
    vertical-align: middle;
  }

  .empty {
    padding: var(--space-2) 0;
  }

  .update-available {
    font-weight: 600;
  }

  .errors {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 240px;
    border-top: 1px solid var(--c-border);
  }

  .error {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--c-border);
  }

  .error:last-child {
    border-bottom: none;
  }

  .error-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .error-title {
    min-width: 0;
    font-size: var(--fs-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .error-summary {
    overflow-wrap: anywhere;
  }

  .data-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
  }

  .data-row + .data-row {
    padding-top: var(--space-3);
    border-top: 1px solid var(--c-border);
  }

  .data-text {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    flex: 1 1 auto;
    min-width: 0;
  }

  .data-title {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-weight: 600;
  }

  .usage {
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--space-3);
  }

  .data-actions {
    display: flex;
    gap: var(--space-2);
    flex: none;
  }

  .update-progress {
    height: 4px;
    background: var(--c-control);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .update-progress-fill {
    height: 100%;
    background: var(--c-right);
    transition: width 240ms ease-out;
  }

  .video-list {
    display: flex;
    flex-direction: column;
    max-height: 220px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
    border-top: 1px solid var(--c-border);
  }

  .video-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--c-border);
  }

  .video-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (max-width: 559px) {
    .data-row {
      flex-direction: column;
      gap: var(--space-2);
    }
  }
</style>
