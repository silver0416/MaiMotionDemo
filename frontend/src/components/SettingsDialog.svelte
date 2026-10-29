<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';
  import { appVersion, clearWikiCache, wikiCacheInfo, type WikiCacheInfo } from '../lib/api';
  import { BUILD_ID, BUILD_TIME, FRONTEND_VERSION } from '../lib/build';
  import {
    SCHEMA_VERSION,
    SCORING_LABEL,
    SCORING_V3,
    SCORING_VERSION,
    TEMOTO_LABEL,
    TEMOTO_MODELS,
  } from '../lib/contract';
  import { STORE_ANALYSES, STORE_RECORDS, STORE_SETTINGS, dbClear, dbCount } from '../lib/db';
  import { copyText } from '../lib/debug';
  import { reducedMotion } from '../lib/press';
  import { FAILURE_KIND_LABEL, errorLog } from '../state/errorLog.svelte';
  import { DISTRIBUTION_LABEL, updateState } from '../state/update.svelte';
  import { cacheEvents } from '../state/cacheEvents.svelte';
  import { records } from '../state/records.svelte';
  import { exportPrefs } from '../state/exportPrefs.svelte';
  import { SOLVER_REVISION, session } from '../state/session.svelte';
  import { toasts } from '../state/toasts.svelte';
  import { VideoLibrary } from '../state/videoLibrary.svelte';
  import { formatDuration } from '../lib/video';
  import { segmented } from '../lib/segmented';
  import { APPEARANCES, DARK_PALETTES, LIGHT_PALETTE, theme } from '../lib/theme.svelte';

  interface Props {
    open: boolean;
  }

  let { open = $bindable() }: Props = $props();

  type Section = 'about' | 'appearance' | 'errors' | 'video' | 'export' | 'data';
  const SECTIONS: { id: Section; label: string; icon: IconName; desktop?: boolean }[] = [
    { id: 'about', label: '版本與更新', icon: 'info' },
    { id: 'appearance', label: '外觀', icon: 'palette' },
    { id: 'video', label: '影片同步', icon: 'video', desktop: true },
    { id: 'export', label: '匯出', icon: 'download', desktop: true },
    { id: 'data', label: '資料管理', icon: 'database' },
    { id: 'errors', label: '錯誤紀錄', icon: 'circle-alert' },
  ];

  let dialog: HTMLDialogElement | null = $state(null);
  /** 左側分類；關閉再開啟時停在上次看的分類。 */
  let section = $state<Section>('about');
  const sections = $derived(SECTIONS.filter((item) => !item.desktop || session.desktop));

  function pickSection(id: Section) {
    section = id;
    // 換分類時取消還沒按第二次的危險操作。
    confirming = null;
  }
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

  /** 目前參數頁選的評分方式；null 為純 V3。畫面上只顯示 TemotoAI-1 與顏色，複製的版本資訊才帶模型 id。 */
  const temoto = $derived(TEMOTO_MODELS.find((model) => model.id === session.config.mlModel) ?? null);
  /** 沒選 Temoto 時選的基座（v4、v3、v3.1、v3.2）直接當版本號；選了 Temoto 時一律為 null。 */
  const scoringVersion = $derived((!temoto && session.config.v3Variant) || SCORING_VERSION);

  function versionText(): string {
    const scoring = temoto
      ? `評分方式：${TEMOTO_LABEL}（${temoto.id}，基於 ${SCORING_LABEL} ${SCORING_VERSION}／${SCORING_V3}，schema ${SCHEMA_VERSION}）`
      : `評分方式：${SCORING_LABEL} ${scoringVersion}（${SCORING_V3}，schema ${SCHEMA_VERSION}）`;
    return [
      `App 版本：${session.desktop ? version : `${FRONTEND_VERSION}（瀏覽器預覽）`}`,
      `應用程式種類：${DISTRIBUTION_LABEL[updateState.distribution]}`,
      `Build：${BUILD_ID}（${BUILD_TIME}）`,
      `判定規則修訂：${SOLVER_REVISION}`,
      scoring,
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
    <button bind:this={closeButton} class="btn btn--round" onclick={() => (open = false)} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div class="dialog-main">
  <nav class="side" aria-label="設定分類">
    {#each sections as item (item.id)}
      <button
        class="side-item"
        class:is-active={section === item.id}
        aria-current={section === item.id ? 'page' : undefined}
        onclick={() => pickSection(item.id)}
      >
        <Icon name={item.icon} size={14} />
        <span class="side-label">{item.label}</span>
        {#if item.id === 'errors' && errorLog.entries.length > 0}
          <span class="side-count mono">{errorLog.entries.length}</span>
        {/if}
      </button>
    {/each}
  </nav>

  <div class="dialog-body scroll">
    {#if section === 'about'}
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
        <dt>評分方式</dt>
        <dd>
          {#if temoto}
            <span class={`scoring-model scoring-model--${temoto.color}`}><span class="scoring-dot" aria-hidden="true"></span>{TEMOTO_LABEL}</span>
            <span class="muted">基於 {SCORING_LABEL} <span class="mono">{SCORING_VERSION}</span></span>
          {:else}
            {SCORING_LABEL} <span class="mono">{scoringVersion}</span>
          {/if}
          <span class="muted mono">{SCORING_V3}・schema {SCHEMA_VERSION}</span>
        </dd>
      </dl>

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
    {/if}

    {#if section === 'appearance'}
      <section class="block" aria-labelledby="settings-scheme">
        <div class="block-head">
          <h3 id="settings-scheme">深淺模式</h3>
        </div>
        <div class="seg scheme-seg" role="radiogroup" aria-labelledby="settings-scheme" use:segmented>
          {#each APPEARANCES as item (item.id)}
            <button
              class="seg-btn"
              role="radio"
              aria-checked={theme.appearance === item.id}
              onclick={() => theme.setAppearance(item.id)}
            >
              {item.label}
            </button>
          {/each}
        </div>
        <p class="field-hint">
          目前是{theme.scheme === 'light' ? '淺色' : '深色'}。淺色固定使用「{LIGHT_PALETTE.label}」：{LIGHT_PALETTE.hint}
          主視窗與影片同步視窗一起切換。
        </p>
      </section>

      <section class="block" aria-labelledby="settings-palette">
        <div class="block-head">
          <h3 id="settings-palette">深色配色</h3>
        </div>
        <div class="palettes" role="radiogroup" aria-labelledby="settings-palette">
          {#each DARK_PALETTES as item (item.id)}
            <button
              class="palette"
              role="radio"
              aria-checked={theme.dark === item.id}
              onclick={() => theme.setDark(item.id)}
            >
              <span class="palette-swatches" aria-hidden="true">
                {#each item.swatches as color, index (index)}
                  <span class="palette-swatch" style={`background:${color}`}></span>
                {/each}
              </span>
              <span class="palette-name">
                {item.label}
                {#if theme.dark === item.id}<Icon name="check" size={14} />{/if}
              </span>
              <span class="palette-hint">{item.hint}</span>
            </button>
          {/each}
        </div>
        {#if theme.scheme === 'light'}
          <p class="field-hint">目前是淺色模式；切回深色時才會用這裡選的配色。</p>
        {/if}
      </section>
    {/if}

    {#if section === 'errors'}
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
    {/if}

    {#if section === 'video' && session.desktop}
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

    {#if section === 'export' && session.desktop}
      <section class="block" aria-labelledby="settings-export">
        <div class="block-head">
          <h3 id="settings-export">匯出</h3>
        </div>

        <div class="data-row">
          <div class="data-text">
            <div class="data-title">預設匯出資料夾</div>
            <p class="xsmall muted mono path">{exportPrefs.dir || '未指定（另存新檔從系統預設位置開始）'}</p>
            <p class="field-hint">匯出標記檔時，另存新檔會先開在這個資料夾。</p>
          </div>
          <div class="data-actions">
            <button class="btn" onclick={() => void exportPrefs.pickDir()}>
              <Icon name="folder-open" />選擇
            </button>
            {#if exportPrefs.dir}
              <button class="btn" onclick={() => exportPrefs.setDir('')}>
                <Icon name="x" />清除
              </button>
            {/if}
          </div>
        </div>

        <label class="check" class:is-disabled={!exportPrefs.dir}>
          <input
            type="checkbox"
            checked={exportPrefs.skipAsk}
            disabled={!exportPrefs.dir}
            onchange={(event) => exportPrefs.setSkipAsk(event.currentTarget.checked)}
          />
          <span>
            不再詢問位置，直接存到預設資料夾
            <span class="field-hint">同名檔案已存在時自動加上編號，不會覆寫。需要先指定預設資料夾。</span>
          </span>
        </label>
      </section>
    {/if}

    {#if section === 'data'}
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
              <span>譜面網站快取 {wikiCache.files} 個檔案・{formatBytes(wikiCache.bytes)}</span>
            {/if}
          </p>
          <p class="field-hint">
            刪除已保存的分析結果{session.desktop ? '，以及 simai Wiki、adxdls 的索引與譜面快取' : ''}。
            譜面紀錄、參數與顯示設定都會保留；下次開啟譜面時重新分析{session.desktop ? '，搜尋譜面網站時重新下載索引' : ''}。
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
    {/if}
  </div>
  </div>
</dialog>

<style>
  /* 固定高度：換分類時對話框不會跟著內容忽大忽小。 */
  .dialog {
    width: min(820px, calc(100vw - 32px));
    height: min(640px, calc(100vh - 32px));
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

  .dialog-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .dialog-main {
    display: grid;
    grid-template-columns: 168px minmax(0, 1fr);
    flex: 1 1 auto;
    min-height: 0;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-2);
    border-right: 1px solid var(--c-border);
  }

  .side-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 34px;
    padding: 0 14px;
    color: var(--c-text-dim);
    font: inherit;
    font-size: var(--fs-sm);
    text-align: left;
    background: none;
    border: none;
    border-radius: var(--radius-pill);
    cursor: pointer;
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .side-item:hover {
    color: var(--c-text);
    background: var(--c-control);
  }

  .side-item.is-active {
    color: var(--c-text-strong);
    background: var(--c-control-hover);
  }

  .side-item:focus-visible {
    outline: 2px solid var(--c-focus);
    outline-offset: -2px;
  }

  .side-label {
    flex: 1;
    min-width: 0;
  }

  .side-count {
    padding: 0 6px;
    font-size: var(--fs-xs);
    color: var(--c-on-danger);
    background: var(--c-danger);
    border-radius: var(--radius-pill);
  }

  .scheme-seg {
    max-width: 360px;
  }

  .palettes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: var(--space-2);
  }

  /* 配色卡：選中時框線用主色。色票是該配色本身的顏色，不跟著目前配色變。 */
  .palette {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding: 12px 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    background: var(--c-control);
    color: var(--c-text);
    text-align: left;
    cursor: pointer;
    transition:
      background-color 120ms ease,
      border-color 160ms ease,
      transform 90ms ease-out;
  }

  .palette:hover {
    background: var(--c-control-hover);
  }

  .palette:active {
    transform: scale(0.98);
  }

  .palette[aria-checked='true'] {
    border-color: var(--c-primary);
    background: var(--c-control-hover);
  }

  .palette-swatches {
    display: flex;
    gap: 4px;
  }

  .palette-swatch {
    flex: 1 1 0;
    height: 22px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border-strong);
  }

  .palette-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-md);
    font-weight: 600;
    color: var(--c-text-strong);
  }

  .palette-name :global(.icon) {
    color: var(--c-primary);
  }

  .palette-hint {
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--c-text-dim);
  }

  .dialog-body {
    min-height: 0;
  }

  @media (max-width: 560px) {
    .dialog-main {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
    }

    .side {
      flex-direction: row;
      overflow-x: auto;
      border-right: none;
      border-bottom: 1px solid var(--c-border);
    }

    .side-item {
      width: auto;
      white-space: nowrap;
    }
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

  .path {
    overflow-wrap: anywhere;
  }

  .check.is-disabled {
    color: var(--c-text-dim);
    cursor: not-allowed;
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

  .empty {
    padding: var(--space-2) 0;
  }

  /* 目前的評分方式：TemotoAI-1 以模型原色的圓點區分三個變體，不顯示變體名稱。 */
  .scoring-model {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 700;
    color: var(--c-text-strong);
  }

  .scoring-dot {
    width: 9px;
    height: 9px;
    border-radius: var(--radius-pill);
    background: var(--dot);
  }

  .scoring-model--rurikon {
    --dot: var(--c-rurikon-deep);
  }

  .scoring-model--ayame {
    --dot: var(--c-ayame-deep);
  }

  .scoring-model--ruri {
    --dot: var(--c-ruri-deep);
  }

  .scoring-model--tokiwa {
    --dot: var(--c-tokiwa-deep);
  }

  .scoring-model--akane {
    --dot: var(--c-akane-deep);
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
    height: 6px;
    background: var(--c-control);
    border-radius: var(--radius-pill);
    overflow: hidden;
  }

  .update-progress-fill {
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--c-primary);
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

  /* Claude Design 定案的對話框：標題列左側多留空間，標題用顯示字型。 */
  .dialog-head {
    padding: 16px 16px 16px 24px;
  }
</style>
