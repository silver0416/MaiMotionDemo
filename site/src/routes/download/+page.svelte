<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$app/components/Icon.svelte';
  import { SITE } from '$lib/nav';
  import { formatDate, formatSize, RELEASES_URL, type Platform, type ReleaseAsset } from '$lib/release/core';
  import { latestRelease } from '$lib/release/release.svelte';

  /**
   * 下載頁：最新版本、依訪客系統放一顆主要下載鈕、四個平台的清單（直接連 GitHub 的檔案）與第一次開啟要注意的事。
   * 版本資料先用建置時的快照（沒有 JS 也有連結），掛載後向 /api/release 更新。
   * 第一次開啟的事實以 src/content/start/install.md 為準，這裡不另外加。
   */

  const PLATFORM_LABEL: Record<Platform, { name: string; variant: string; action: string }> = {
    windows: { name: 'Windows', variant: 'x64', action: '下載 Windows 版' },
    macos: { name: 'macOS', variant: 'universal', action: '下載 macOS 版' },
    'linux-appimage': { name: 'Linux', variant: 'AppImage', action: '下載 Linux 版（AppImage）' },
    'linux-deb': { name: 'Linux', variant: 'deb', action: '下載 Linux 版（deb）' },
  };

  /** pending：還沒掛載（預先算繪與沒有 JS 時），不放主要按鈕。 */
  type Visitor = 'pending' | 'windows' | 'macos' | 'linux' | 'other';

  let visitor = $state<Visitor>('pending');

  function detectVisitor(): Visitor {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string; mobile?: boolean } };
    const ua = nav.userAgent;
    const platform = nav.userAgentData?.platform || nav.platform || '';
    if (nav.userAgentData?.mobile || /Android|iPhone|iPad|iPod|Mobile/i.test(ua) || /Android|iOS/i.test(platform)) return 'other';
    if (/CrOS|Chrome OS/i.test(ua + platform)) return 'other';
    // iPadOS 的 Safari 自稱 Macintosh：有多點觸控就當成平板。
    if (/Mac/i.test(platform) || /Macintosh/.test(ua)) return nav.maxTouchPoints > 1 ? 'other' : 'macos';
    if (/Win/i.test(platform) || /Windows/.test(ua)) return 'windows';
    if (/Linux/i.test(platform) || /Linux|X11/.test(ua)) return 'linux';
    return 'other';
  }

  onMount(() => {
    visitor = detectVisitor();
    latestRelease.refresh();
  });

  const info = $derived(latestRelease.info);
  const assets = $derived(info?.assets ?? []);
  const date = $derived(formatDate(info?.publishedAt ?? null));

  const recommended = $derived.by((): ReleaseAsset | undefined => {
    const wanted: Platform | null =
      visitor === 'windows' ? 'windows' : visitor === 'macos' ? 'macos' : visitor === 'linux' ? 'linux-appimage' : null;
    return wanted ? assets.find((asset) => asset.platform === wanted) : undefined;
  });

  // SHA-256 的複製鈕：短暫換成打勾，並由 role="status" 念出結果。
  let copied = $state<Platform | null>(null);
  let copyStatus = $state('');
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  async function copyHash(asset: ReleaseAsset, event: MouseEvent) {
    if (!asset.sha256) return;
    try {
      await navigator.clipboard.writeText(asset.sha256);
      copied = asset.platform;
      copyStatus = '已複製 SHA-256';
    } catch {
      // 剪貼簿不能用：選取那串字，讓使用者自己按 Ctrl+C。
      const code = (event.currentTarget as HTMLElement).closest('.hash-body')?.querySelector('code');
      if (code) {
        const range = document.createRange();
        range.selectNodeContents(code);
        const selection = getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
      }
      copyStatus = '沒辦法直接複製，已選取內容，請按 Ctrl+C';
    }
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copied = null;
      copyStatus = '';
    }, 1600);
  }

  onMount(() => () => clearTimeout(copyTimer));

  const title = `下載｜${SITE.name}`;
  const description = '下載 MaiMotionDemo 最新版：Windows、macOS、Linux。免安裝，下載後直接開啟。';
  const url = `${SITE.origin}/download`;
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <link rel="canonical" href={url} />
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content={SITE.name} />
  <meta property="og:title" content="下載" />
  <meta property="og:description" content={description} />
  <meta property="og:url" content={url} />
  <meta property="og:locale" content="zh_TW" />
</svelte:head>

{#snippet external(href: string, label: string)}
  <a class="btn" {href} target="_blank" rel="noopener">
    {label}<Icon name="external-link" />
    <span class="sr-only">（在新分頁開啟）</span>
  </a>
{/snippet}

<div class="download">
  <header class="head">
    <h1>下載</h1>
    {#if info}
      <p class="version">
        <span class="label">最新版本</span>
        <span class="mono version-number">v{info.version}</span>
        {#if date}<span><time class="mono" datetime={info.publishedAt}>{date}</time> 發布</span>{/if}
      </p>
    {/if}
    <p class="lede">MaiMotionDemo 是免安裝的桌面程式，下載對應平台的檔案後直接開啟。分析全部在你的電腦上完成，不需要帳號。</p>
  </header>

  {#if assets.length === 0}
    <div class="alert empty" role="status">
      <p>暫時拿不到版本資訊，請直接到 GitHub 的發布頁下載。</p>
      {@render external(RELEASES_URL, '前往 GitHub Releases')}
    </div>
  {:else}
    {#if visitor !== 'pending'}
      <div class="recommend">
        {#if recommended}
          <a class="btn btn--primary" href={recommended.url} rel="noopener">
            <Icon name="download" />{PLATFORM_LABEL[recommended.platform].action}
          </a>
          <p class="recommend-meta">
            <span class="mono file">{recommended.name}</span>
            {#if formatSize(recommended.size)}<span class="mono">{formatSize(recommended.size)}</span>{/if}
          </p>
        {:else}
          <p class="recommend-note">MaiMotionDemo 是桌面程式，請在電腦上下載。</p>
        {/if}
      </div>
    {/if}

    <section class="platforms" aria-labelledby="platforms-title">
      <h2 id="platforms-title" class="list-title">所有平台</h2>
      <ul class="list">
        {#each assets as asset (asset.platform)}
          {@const label = PLATFORM_LABEL[asset.platform]}
          <li class="entry">
            <div class="entry-head">
              <div class="entry-info">
                <h3 class="entry-name">{label.name}<span class="entry-variant">{label.variant}</span></h3>
                <p class="entry-file">
                  <span class="mono file">{asset.name}</span>
                  <span class="mono size">{formatSize(asset.size) ?? '—'}</span>
                </p>
              </div>
              <a class="btn entry-action" href={asset.url} rel="noopener">
                <Icon name="download" />下載<span class="sr-only">{label.name} {label.variant}（{asset.name}）</span>
              </a>
            </div>

            <ul class="notes">
              {#if asset.platform === 'windows'}
                <li>單一執行檔，放到想放的資料夾雙擊開啟，不需要安裝。</li>
                <li>
                  需要 WebView2 Runtime。電腦缺少它時，到
                  <a href="https://developer.microsoft.com/en-us/microsoft-edge/webview2/" target="_blank" rel="noopener"
                    >Microsoft WebView2 Runtime<span class="sr-only">（在新分頁開啟）</span></a
                  >
                  下載安裝後再開一次。
                </li>
                <li>有新版時可以在程式裡直接更新。</li>
              {:else if asset.platform === 'macos'}
                <li>解壓縮後開啟。</li>
                <li>沒有簽章：第一次開啟要在 Finder 對程式按右鍵，選「打開」，再確認一次。</li>
                <li>程式內的自動更新只支援 Windows，有新版時回到這一頁下載。</li>
              {:else if asset.platform === 'linux-appimage'}
                <li>
                  先加上執行權限再開啟：在終端機執行 <code>chmod +x MaiMotionDemo-*.AppImage</code>，或在檔案管理員的「內容／權限」裡勾選允許執行。
                </li>
                <li>程式內的自動更新只支援 Windows，有新版時回到這一頁下載。</li>
              {:else}
                <li>Debian／Ubuntu 系統可用的套件，用系統的套件工具安裝。</li>
                <li>程式內的自動更新只支援 Windows，有新版時回到這一頁下載。</li>
              {/if}
            </ul>

            {#if asset.sha256}
              <details class="hash">
                <summary>SHA-256</summary>
                <div class="hash-body">
                  <code class="mono">{asset.sha256}</code>
                  <button
                    type="button"
                    class="btn btn--round"
                    aria-label={copied === asset.platform ? '已複製' : `複製 ${label.name} ${label.variant} 的 SHA-256`}
                    title={copied === asset.platform ? '已複製' : '複製'}
                    onclick={(event) => copyHash(asset, event)}
                  >
                    <Icon name={copied === asset.platform ? 'check' : 'copy'} />
                  </button>
                </div>
              </details>
            {/if}
          </li>
        {/each}
      </ul>
      <p class="untested">macOS 與 Linux 版由 GitHub Actions 自動建置，還沒有在實機上完整測試。</p>
    </section>
  {/if}

  <footer class="more">
    {#if info && assets.length > 0}
      <div class="more-links">
        {@render external(RELEASES_URL, '所有版本')}
        {@render external(info.pageUrl, '這一版的更新內容')}
      </div>
    {/if}
    <p class="help-line">
      安裝遇到問題？看<a href="/start/install">下載與啟動</a>或<a href="/more/faq">常見問題</a>。
    </p>
  </footer>
</div>

<p class="sr-only" role="status" aria-live="polite">{copyStatus}</p>

<style>
  .download {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    max-width: var(--prose-max);
    margin-inline: auto;
  }

  /* 寬螢幕：和內容頁的內文對齊（內容頁是「內文＋右側目錄」整塊置中）。 */
  @media (width >= 1280px) {
    .download {
      margin-left: max(0px, calc((100% - (var(--prose-max) + var(--space-6) + var(--toc-w))) / 2));
    }
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* 頁面標題：和內容頁一樣是內文 16px 的 1.75 倍。 */
  h1 {
    color: var(--c-text-strong);
    font-size: calc(var(--fs-prose) * 1.75);
    font-weight: 700;
    line-height: var(--lh-tight);
  }

  .version {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-2);
    color: var(--c-text-dim);
    font-size: var(--fs-md);
  }

  .version-number {
    color: var(--c-text-strong);
    font-size: var(--fs-lg);
    font-weight: 600;
  }

  .lede {
    color: var(--c-text);
    font-size: var(--fs-prose);
    line-height: var(--lh-prose);
  }

  .recommend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-4);
  }

  .recommend .btn--primary {
    min-height: 40px;
    padding: 0 var(--space-5);
    font-size: var(--fs-md);
  }

  .recommend-meta,
  .entry-file {
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--space-3);
    min-width: 0;
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  .file {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .recommend-note {
    color: var(--c-text);
    font-size: var(--fs-lg);
  }

  .list-title {
    margin-bottom: var(--space-2);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  /* 平台清單：一個平台一列，列與列之間 1px 分隔線（不是卡片）。 */
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--c-border);
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--c-border);
  }

  .entry-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .entry-info {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  .entry-name {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    color: var(--c-text-strong);
    font-size: var(--fs-xl);
    font-weight: 600;
    line-height: var(--lh-tight);
  }

  .entry-variant {
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    font-weight: 500;
  }

  .entry-action {
    flex: none;
  }

  .notes {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding-left: 1.25em;
    list-style: disc;
    color: var(--c-text);
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
  }

  .notes li::marker {
    color: var(--c-text-dim);
  }

  .notes a,
  .help-line a {
    color: var(--c-primary);
    text-decoration: underline;
    text-underline-offset: 0.2em;
    text-decoration-thickness: 1px;
  }

  @media (hover: hover) {
    .notes a:hover,
    .help-line a:hover {
      text-decoration-thickness: 2px;
    }
  }

  .notes code {
    padding: 0.1em 0.4em;
    border-radius: var(--radius-md);
    background: var(--c-control);
    color: var(--c-text-strong);
    font-family: var(--font-mono);
    font-size: 0.875em;
    overflow-wrap: anywhere;
  }

  .hash summary {
    display: inline-flex;
    align-items: center;
    min-height: 28px;
  }

  .hash-body {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .hash-body code {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--c-text);
    font-size: var(--fs-sm);
    word-break: break-all;
  }

  .hash-body .btn--round {
    flex: none;
  }

  .untested {
    margin-top: var(--space-3);
    color: var(--c-text-dim);
    font-size: var(--fs-md);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
    font-size: var(--fs-md);
  }

  .more {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .more-links {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .help-line {
    color: var(--c-text-dim);
    font-size: var(--fs-md);
  }

  @media (width < 560px) {
    .entry-head {
      flex-direction: column;
      align-items: stretch;
    }

    .entry-action {
      align-self: flex-start;
    }
  }

  @media (pointer: coarse) {
    .recommend .btn--primary {
      min-height: 44px;
    }

    .hash summary {
      min-height: 44px;
    }
  }
</style>
