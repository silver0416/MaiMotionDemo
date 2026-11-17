<script lang="ts">
  import Icon from '$app/components/Icon.svelte';
  import Demo from '$lib/demos/Demo.svelte';
  import { pagesIn, SECTIONS, SITE } from '$lib/nav';
  import InstallEntry from '$lib/pwa/InstallEntry.svelte';
  import LatestVersion from '$lib/release/LatestVersion.svelte';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';

  /**
   * 首頁：一句話說明、盤面演示、認識 MaiMotionDemo（全頁唯一的主要動作）、閱讀手冊與下載、各個章節。
   * 手機（iOS／Android）上「下載」換成「安裝手冊到手機」（把這份手冊裝成 App）：桌面程式在手機上不能用，
   * 真正的下載入口留在選單裡。已經裝成 App 時兩顆都不顯示。
   */
  const chapters = SECTIONS.map((section) => ({ section, pages: pagesIn(section.id) }));
  const description = '把 simai 譜面變成可播放的左右手動畫：看兩隻手怎麼移動、什麼時候換手。';
</script>

<svelte:head>
  <title>{SITE.name}</title>
  <meta name="description" content={description} />
  <link rel="canonical" href={`${SITE.origin}/`} />
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content={SITE.name} />
  <meta property="og:title" content={SITE.name} />
  <meta property="og:description" content={description} />
  <meta property="og:url" content={`${SITE.origin}/`} />
  <meta property="og:locale" content="zh_TW" />
</svelte:head>

<div class="home">
  <section class="hero" aria-labelledby="home-title">
    <div class="hero-text">
      <h1 id="home-title" class="wordmark">MaiMotionDemo</h1>
      <p class="lede">把 simai 譜面變成可播放的左右手動畫，看兩隻手怎麼移動、什麼時候換手。</p>
      <p class="note">打法是依規則推算的建議，不是官方判定，也不是唯一正確的打法。</p>
      <div class="actions">
        <a class="btn btn--primary cta" href="/intro/about">認識 MaiMotionDemo<span class="cta-arrow"><ExtraIcon name="arrow-right" /></span></a>
        <a class="btn" href="/start/install"><ExtraIcon name="book-open" />閱讀手冊</a>
        <a class="btn home-download" href={SITE.download}>
          <Icon name="download" />下載
        </a>
        <InstallEntry label="安裝手冊到手機" />
      </div>
      <LatestVersion />
    </div>
    <div class="hero-demo">
      <Demo id="tap-alternate" caption="八個 Tap 左右交替：以預設參數為例，右手打 1–4、左手打 8–5。" />
    </div>
  </section>

  <section class="chapters" aria-labelledby="chapters-title">
    <h2 id="chapters-title" class="chapters-title">章節</h2>
    <ol class="chapter-list">
      {#each chapters as { section, pages } (section.id)}
        <li class="chapter">
          <div class="chapter-head">
            <h3>{section.title}</h3>
            <p class="chapter-blurb">{section.blurb}</p>
          </div>
          {#if pages.length > 0}
            <ul class="chapter-pages">
              {#each pages as item (item.url)}
                <li><a class="chapter-link" href={item.url}>{item.title}</a></li>
              {/each}
            </ul>
          {:else}
            <p class="chapter-empty">內容整理中</p>
          {/if}
        </li>
      {/each}
    </ol>
  </section>
</div>

<style>
  .home {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    width: 100%;
    max-width: 68rem;
    /* 寬螢幕時置中。 */
    margin-inline: auto;
  }

  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-5);
    align-items: center;
  }

  @media (width >= 1100px) {
    .hero {
      grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr);
      gap: var(--space-6);
    }
  }

  .hero-text {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  /* 首頁標題：內文 16px 的 2 倍。 */
  h1 {
    color: var(--c-text-strong);
    font-size: calc(var(--fs-prose) * 2);
    font-weight: 700;
    line-height: var(--lh-tight);
  }

  .lede {
    color: var(--c-text);
    font-size: var(--fs-xl);
    line-height: var(--lh-normal);
  }

  .note {
    color: var(--c-text-dim);
    font-size: var(--fs-md);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  /* 手機：桌面程式的「下載」不顯示（<html data-os> 由 app.html 在第一次繪製前設好，不會先閃一下）。 */
  :global(:root[data-os]) .home-download {
    display: none;
  }

  .actions :global(.btn) {
    min-height: 40px;
    padding: 0 var(--space-5);
    font-size: var(--fs-md);
  }

  .hero-demo :global(.demo) {
    margin: 0;
  }

  .chapters-title {
    margin-bottom: var(--space-2);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  /* 章節清單：一章一列，列與列之間 1px 分隔線（不是卡片）。 */
  .chapter-list {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--c-border);
  }

  .chapter {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-2) var(--space-5);
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--c-border);
  }

  @media (width >= 900px) {
    .chapter {
      grid-template-columns: minmax(0, 18rem) minmax(0, 1fr);
    }
  }

  .chapter-head h3 {
    color: var(--c-text-strong);
    font-size: var(--fs-xl);
  }

  .chapter-blurb {
    margin-top: var(--space-1);
    color: var(--c-text-dim);
    font-size: var(--fs-md);
  }

  .chapter-pages {
    display: flex;
    flex-wrap: wrap;
    align-content: flex-start;
    gap: var(--space-1) var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chapter-link {
    display: inline-flex;
    align-items: center;
    min-height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-pill);
    color: var(--c-text);
    font-size: var(--fs-md);
    text-decoration: none;
    transition:
      background-color 120ms ease,
      border-color 120ms ease,
      transform 90ms ease-out;
  }

  @media (hover: hover) {
    .chapter-link:hover {
      background: var(--c-control);
      border-color: var(--c-border-strong);
      color: var(--c-text-strong);
    }
  }

  .chapter-link:active {
    transform: scale(0.96);
  }

  .chapter-empty {
    color: var(--c-text-dim);
    font-size: var(--fs-md);
  }

  @media (pointer: coarse) {
    .chapter-link {
      min-height: 44px;
      padding: 0 var(--space-4);
    }

    .actions :global(.btn) {
      min-height: 44px;
    }
  }

  /* 主要動作的箭頭：滑鼠移上去（或鍵盤聚焦）時往右滑一點，暗示「往下一步」；按下時再多一點。 */
  .cta-arrow {
    display: inline-flex;
    transition: transform 220ms var(--ease-spring);
  }

  @media (hover: hover) {
    .cta:hover .cta-arrow {
      transform: translateX(4px);
    }
  }

  .cta:focus-visible .cta-arrow {
    transform: translateX(4px);
  }

  .cta:active .cta-arrow {
    transform: translateX(6px);
  }

  @media (prefers-reduced-motion: reduce) {
    .chapter-link,
    .cta-arrow {
      transition-duration: 1ms;
    }

    .cta:hover .cta-arrow,
    .cta:focus-visible .cta-arrow,
    .cta:active .cta-arrow {
      transform: none;
    }
  }
</style>
