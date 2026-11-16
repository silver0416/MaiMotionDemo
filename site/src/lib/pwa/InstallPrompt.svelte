<script lang="ts">
  /**
   * 手機專用的安裝提示（Android 的安裝按鈕、iOS 的加入主畫面步驟、App 內建瀏覽器的引導），
   * 以及 Web App 需要的 <head> 標籤（manifest）、App 模式的「有新版本」提示。外框把它掛在每一頁一次即可。
   *
   * 不會自動跳出來：使用者按了首頁的「安裝手冊到手機」或選單的「加到主畫面」（InstallEntry）才出現。
   * Android 有安裝事件時直接叫出系統視窗，不經過這裡；這裡顯示的是 Android 的手動步驟、iOS 的步驟、內建瀏覽器的引導。
   * 也處理 manifest 捷徑「搜尋」（網址帶 ?search=1 就打開搜尋）。
   */
  import { onMount } from 'svelte';
  import { afterNavigate, replaceState } from '$app/navigation';
  import Icon from '$app/components/Icon.svelte';
  import { platform } from '$lib/app/platform.svelte';
  import Logo from '$lib/shell/Logo.svelte';
  import { ui } from '$lib/shell/ui.svelte';
  import { checkInstalled, dismissPrompt, runNativePrompt } from './install';
  import Notice from './Notice.svelte';
  import PwaIcon from './PwaIcon.svelte';
  import { checkForUpdate } from './register';
  import { installKind, pwa } from './state.svelte';
  import UpdateNotice from './UpdateNotice.svelte';

  const kind = $derived(installKind());
  /** 搜尋、選單抽屜，或 App 外框的「目錄／更多」面板開著時先藏起來，不疊在它們上面。 */
  const covered = $derived(ui.anyOpen);
  const visible = $derived(pwa.promptOpen && kind !== 'none' && !covered);

  let copyStatus = $state('');

  afterNavigate(({ type }) => {
    if (type !== 'enter') checkForUpdate();
  });

  /** manifest 的捷徑「搜尋」：/?search=1 打開搜尋，然後把參數從網址拿掉。 */
  function openSearchShortcut() {
    const url = new URL(location.href);
    if (!url.searchParams.has('search')) return;
    url.searchParams.delete('search');
    try {
      replaceState(url, {});
    } catch {
      // 路由還沒準備好：參數留著也沒關係。
    }
    ui.openSearch();
  }

  onMount(() => {
    void checkInstalled();
    openSearchShortcut();
  });

  /** App 內建瀏覽器：Android 用 intent 開 Chrome（沒裝 Chrome 時退回原網址）；LINE 用它自己的參數開外部瀏覽器。 */
  function chromeIntent(): string {
    const target = `${location.host}${location.pathname}${location.search}`;
    return `intent://${target}#Intent;scheme=https;package=com.android.chrome;S.browser_fallback_url=${encodeURIComponent(location.href)};end`;
  }

  function lineExternal(): string {
    const url = new URL(location.href);
    url.searchParams.set('openExternalBrowser', '1');
    return url.href;
  }

  async function copyLink() {
    try {
      await navigator.clipboard.writeText(location.href);
      copyStatus = '已複製，到 Safari 貼上網址就能打開';
    } catch {
      copyStatus = '沒辦法直接複製，請從這個 App 的選單改用瀏覽器開啟';
    }
  }
</script>

<svelte:head>
  <link rel="manifest" href="/manifest.webmanifest" />
  <!-- 加入主畫面時預設的名稱（iOS）。不設 apple-mobile-web-app-status-bar-style：網站有深淺兩色，安裝後改不了。 -->
  <meta name="apple-mobile-web-app-title" content="MaiMotion" />
</svelte:head>

<UpdateNotice hidden={covered} />

{#if visible}
  {#if kind === 'native'}
    <Notice label="安裝成 App">
      <div class="prompt">
        <span class="prompt-mark"><Logo size={28} /></span>
        <p class="prompt-text">把手冊加到主畫面，離線也能看</p>
        <button class="btn btn--primary" onclick={runNativePrompt}>安裝</button>
        <button class="btn btn--round prompt-close" aria-label="關閉" onclick={dismissPrompt}><Icon name="x" /></button>
      </div>
    </Notice>
  {:else if kind === 'android-manual'}
    <Notice label="安裝手冊到手機">
      <div class="prompt">
        <span class="prompt-mark"><Logo size={28} /></span>
        <p class="prompt-text">從瀏覽器的選單安裝，離線也能看</p>
        <button class="btn btn--round prompt-close" aria-label="關閉" onclick={dismissPrompt}><Icon name="x" /></button>
      </div>
      <ol class="steps">
        <li class="step">
          <span class="step-icon step-icon--turn"><Icon name="more-horizontal" size={20} /></span>
          <span class="step-body">
            <span class="step-title">點瀏覽器的選單</span>
            <span class="step-hint">通常在網址列的右邊</span>
          </span>
        </li>
        <li class="step">
          <span class="step-icon"><PwaIcon name="square-plus" size={20} /></span>
          <span class="step-body">
            <span class="step-title">選「安裝應用程式」或「加到主畫面」</span>
            <span class="step-hint">各家瀏覽器的用字不太一樣</span>
          </span>
        </li>
      </ol>
    </Notice>
  {:else if kind === 'ios' || kind === 'ios-other'}
    <Notice label="加到主畫面">
      <div class="prompt">
        <span class="prompt-mark"><Logo size={28} /></span>
        <p class="prompt-text">把手冊加到主畫面，離線也能看</p>
        <button
          class="btn btn--primary"
          aria-expanded={pwa.stepsOpen}
          aria-controls="pwa-steps"
          onclick={() => (pwa.stepsOpen = !pwa.stepsOpen)}
        >
          {pwa.stepsOpen ? '收起' : '怎麼加'}
        </button>
        <button class="btn btn--round prompt-close" aria-label="關閉" onclick={dismissPrompt}><Icon name="x" /></button>
      </div>
      {#if pwa.stepsOpen}
        <ol class="steps" id="pwa-steps">
          <li class="step">
            <span class="step-icon"><PwaIcon name="share" size={20} /></span>
            <span class="step-body">
              {#if kind === 'ios'}
                <span class="step-title">點「分享」</span>
                <span class="step-hint">
                  沒看到的話，先點網址列旁的
                  <span class="inline-icon" role="img" aria-label="更多"><Icon name="more-horizontal" size={14} /></span>
                </span>
              {:else}
                <span class="step-title">點瀏覽器的「分享」</span>
                <span class="step-hint">在網址列旁邊，或瀏覽器的選單裡</span>
              {/if}
            </span>
          </li>
          <li class="step">
            <span class="step-icon"><PwaIcon name="square-plus" size={20} /></span>
            <span class="step-body">
              <span class="step-title">選「加入主畫面」</span>
              <span class="step-hint">在分享選單裡，可能要往下捲</span>
            </span>
          </li>
          <li class="step">
            <span class="step-icon"><PwaIcon name="toggle-right" size={20} /></span>
            <span class="step-body">
              <span class="step-title">確認「以網頁 App 打開」是開著的，點「加入」</span>
              <span class="step-hint">沒有這個選項就直接點「加入」</span>
            </span>
          </li>
        </ol>
      {/if}
    </Notice>
  {:else if kind === 'in-app'}
    <Notice label="用瀏覽器開啟" quiet>
      <div class="prompt prompt--quiet">
        <p class="prompt-text prompt-text--quiet">請用瀏覽器開啟才能安裝</p>
        {#if platform.inApp === 'line'}
          <a class="btn" href={lineExternal()}><Icon name="external-link" />用瀏覽器開啟</a>
        {:else if platform.os === 'android'}
          <a class="btn" href={chromeIntent()}><Icon name="external-link" />用 Chrome 開啟</a>
        {:else}
          <button class="btn" onclick={copyLink}><Icon name="copy" />複製連結</button>
        {/if}
        <button class="btn btn--round prompt-close" aria-label="關閉" onclick={dismissPrompt}><Icon name="x" /></button>
      </div>
      {#if platform.inApp !== 'line' && platform.os !== 'android'}
        <p class="prompt-status" role="status">{copyStatus || '複製後到 Safari 貼上網址'}</p>
      {/if}
    </Notice>
  {/if}
{/if}

<style>
  .prompt {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .prompt-mark {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-pill);
    background: var(--c-bg);
    color: var(--c-text-strong);
  }

  .prompt-text {
    flex: 1 1 auto;
    min-width: 0;
    margin: 0;
    font-size: var(--fs-md);
    font-weight: 600;
    line-height: var(--lh-tight);
    /* 兩行時在標點處斷（「主畫面，／離線也能看」），不要從詞中間切開；真的放不下才任意斷。 */
    word-break: keep-all;
    overflow-wrap: anywhere;
  }

  .prompt-text--quiet {
    font-weight: 500;
    color: var(--c-text);
  }

  .prompt .btn {
    flex: none;
  }

  .prompt-close {
    background: var(--c-control);
    color: var(--c-text);
  }

  .prompt-status {
    margin: var(--space-1) 0 0;
    font-size: var(--fs-xs);
    color: var(--c-text-dim);
    line-height: var(--lh-tight);
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: var(--space-3) 0 var(--space-1);
    padding: var(--space-3) var(--space-2) 0 0;
    border-top: 1px solid var(--c-border-strong);
    list-style: none;
    counter-reset: step;
    animation: steps-in 160ms ease-out;
  }

  .step {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    counter-increment: step;
  }

  .step-title::before {
    content: counter(step) '. ';
    font-family: var(--font-mono);
    color: var(--c-text-dim);
  }

  .step-icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-pill);
    background: var(--c-control);
    color: var(--c-primary);
  }

  .step-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .step-title {
    font-size: var(--fs-md);
    font-weight: 600;
    line-height: var(--lh-tight);
    word-break: keep-all;
    overflow-wrap: anywhere;
  }

  .step-hint {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--fs-xs);
    color: var(--c-text-dim);
    line-height: var(--lh-tight);
  }

  .inline-icon {
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 18px;
    border-radius: var(--radius-pill);
    background: var(--c-control);
    color: var(--c-text);
  }

  @keyframes steps-in {
    from {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .steps {
      animation: none;
    }
  }

  /* Android 瀏覽器的選單是直的三個點：把橫的「更多」圖示轉 90 度。 */
  .step-icon--turn :global(svg) {
    transform: rotate(90deg);
  }
</style>
