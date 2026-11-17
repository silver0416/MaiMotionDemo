<script lang="ts">
  import { SITE, sectionById, type Page } from '$lib/nav';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';
  import PageToc from '$lib/shell/PageToc.svelte';
  import RoamingLogo from '$lib/shell/RoamingLogo.svelte';
  import type { PageProps } from './$types';

  /** 內容頁：章節名、標題、一句話說明、（窄時）本頁內容、內文、上一頁／下一頁；≥ 1280px 右側是本頁內容。 */
  let { data }: PageProps = $props();

  const Content = $derived(data.content);
  const headings = $derived(data.page.headings);
  const title = $derived(`${data.page.title}｜${SITE.name}`);
  const url = $derived(`${SITE.origin}${data.page.url}`);

  /** 上一頁／下一頁的標題：跨到別的章節時在前面加章名（例如「基礎操作演示：下載」），不然看不出換章了。 */
  function pagerTitle(target: Page): string {
    if (target.section === data.page.section) return target.title;
    const section = sectionById(target.section);
    return section ? `${section.title}：${target.title}` : target.title;
  }
</script>

<svelte:head>
  <title>{title}</title>
  {#if data.page.description}<meta name="description" content={data.page.description} />{/if}
  <link rel="canonical" href={url} />
  <meta property="og:type" content="article" />
  <meta property="og:site_name" content={SITE.name} />
  <meta property="og:title" content={data.page.title} />
  {#if data.page.description}<meta property="og:description" content={data.page.description} />{/if}
  <meta property="og:url" content={url} />
  <meta property="og:locale" content="zh_TW" />
</svelte:head>

<div class="doc">
  <article class="doc-main">
    <header class="doc-head">
      <p class="doc-section">{data.section.title}</p>
      {#if data.page.mascot}
        <div class="doc-title">
          <h1>{data.page.title}</h1>
          <RoamingLogo size={48} />
        </div>
      {:else}
        <h1>{data.page.title}</h1>
      {/if}
      {#if data.page.description}<p class="doc-lede">{data.page.description}</p>{/if}
    </header>

    <div class="toc-inline">
      <PageToc {headings} variant="inline" />
    </div>

    <div class="prose">
      <Content />
    </div>

    {#if data.prev || data.next}
      <nav class="pager" aria-label="上一頁與下一頁">
        {#if data.prev}
          <a class="pager-link" href={data.prev.url} rel="prev">
            <span class="pager-label"><ExtraIcon name="arrow-left" />上一頁</span>
            <span class="pager-title">{pagerTitle(data.prev)}</span>
          </a>
        {:else}
          <span></span>
        {/if}
        {#if data.next}
          <a class="pager-link is-next" href={data.next.url} rel="next">
            <span class="pager-label">下一頁<ExtraIcon name="arrow-right" /></span>
            <span class="pager-title">{pagerTitle(data.next)}</span>
          </a>
        {/if}
      </nav>
    {/if}
  </article>

  <aside class="toc-aside">
    <PageToc {headings} variant="aside" />
  </aside>
</div>

<style>
  .doc {
    display: grid;
    grid-template-columns: minmax(0, var(--prose-max));
    gap: var(--space-6);
    /* 寬螢幕時內文（連同右側目錄）置中在側欄右邊的空間裡，不要全擠在左邊、右邊空一大塊。 */
    justify-content: center;
  }

  @media (width >= 1280px) {
    .doc {
      grid-template-columns: minmax(0, var(--prose-max)) var(--toc-w);
      justify-content: center;
    }
  }

  .doc-main {
    min-width: 0;
  }

  .doc-head {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-bottom: var(--space-5);
  }

  .doc-section {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  /* 標題旁的吉祥物：和標題同一列、上下置中。 */
  .doc-title {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }

  /* 頁面標題：內文 16px 的 1.75 倍（28px）。 */
  h1 {
    color: var(--c-text-strong);
    font-size: calc(var(--fs-prose) * 1.75);
    font-weight: 700;
    line-height: var(--lh-tight);
  }

  .doc-lede {
    color: var(--c-text-dim);
    font-size: var(--fs-prose);
    line-height: var(--lh-prose);
  }

  .toc-inline {
    margin-bottom: var(--space-5);
  }

  .toc-aside {
    display: none;
  }

  @media (width >= 1280px) {
    .toc-inline {
      display: none;
    }

    .toc-aside {
      display: block;
      position: sticky;
      top: calc(var(--topbar-h) + var(--space-6));
      align-self: start;
      max-height: calc(100dvh - var(--topbar-h) - var(--space-6) * 2);
      overflow-y: auto;
      scrollbar-width: thin;
      scrollbar-color: var(--c-border-strong) transparent;
    }
  }

  /* 上一頁／下一頁：兩個並排的列（單層、有框），手機上下排。 */
  .pager {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
    margin-top: var(--space-6);
    padding-top: var(--space-5);
    border-top: 1px solid var(--c-border);
  }

  .pager-link {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-item);
    color: var(--c-text);
    text-decoration: none;
    transition:
      background-color 120ms ease,
      border-color 120ms ease,
      transform 90ms ease-out;
  }

  .pager-link.is-next {
    grid-column: 2;
    align-items: flex-end;
    text-align: right;
  }

  @media (hover: hover) {
    .pager-link:hover {
      background: var(--c-control);
      border-color: var(--c-border-strong);
    }
  }

  .pager-link:active {
    transform: scale(0.99);
  }

  .pager-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  .pager-title {
    color: var(--c-text-strong);
    font-size: var(--fs-lg);
    font-weight: 600;
    line-height: var(--lh-tight);
  }

  @media (width < 560px) {
    .pager {
      grid-template-columns: 1fr;
    }

    .pager > span:empty {
      display: none;
    }

    .pager-link.is-next {
      grid-column: auto;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .pager-link {
      transition-duration: 1ms;
    }
  }
</style>
