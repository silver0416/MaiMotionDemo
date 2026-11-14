<script lang="ts">
  import Icon from '$app/components/Icon.svelte';
  import type { Heading } from '$lib/nav';

  /**
   * 「本頁內容」：這一頁的 ## ／ ### 標題。
   * - variant="aside"：≥ 1280px 時在右側 sticky，捲動時標出目前所在的段落。
   * - variant="inline"：較窄時放在內容上方，可收合（預設收合）。
   */
  interface Props {
    headings: Heading[];
    variant: 'aside' | 'inline';
  }

  let { headings, variant }: Props = $props();

  let active = $state<string | null>(null);
  let open = $state(false);

  /** 標題頂端捲過這條線（視窗上方約三成處）就算進入那一段。 */
  const line = () => Math.max(120, window.innerHeight * 0.3);

  // 換頁時（同一個路由元件）headings 會換，所以用 $effect 而不是 onMount。
  $effect(() => {
    if (variant !== 'aside') return;
    const list = headings;
    let frame = 0;
    const update = () => {
      frame = 0;
      let current: string | null = null;
      const limit = line();
      for (const heading of list) {
        const element = document.getElementById(heading.id);
        if (!element) continue;
        if (element.getBoundingClientRect().top <= limit) current = heading.id;
        else break;
      }
      // 捲到底時最後一段可能永遠到不了那條線：直接算最後一段。
      if (window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2 && list.length) {
        current = list[list.length - 1].id;
      }
      active = current;
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
    };
  });
</script>

{#if headings.length > 0}
  {#if variant === 'aside'}
    <nav class="toc toc--aside" aria-labelledby="toc-title">
      <p class="toc-title" id="toc-title">本頁內容</p>
      <ul class="toc-list">
        {#each headings as heading (heading.id)}
          <li>
            <a
              class={['toc-link', heading.depth === 3 && 'is-sub', active === heading.id && 'is-active']}
              href={`#${heading.id}`}
              aria-current={active === heading.id ? 'location' : undefined}
            >
              {heading.text}
            </a>
          </li>
        {/each}
      </ul>
    </nav>
  {:else}
    <nav class="toc toc--inline" aria-label="本頁內容">
      <button
        type="button"
        class="toc-toggle"
        aria-expanded={open}
        aria-controls="toc-inline-list"
        onclick={() => (open = !open)}
      >
        <span>本頁內容</span>
        <span class="toc-count mono">{headings.filter((h) => h.depth === 2).length || headings.length}</span>
        <span class="spacer"></span>
        <span class="toc-chevron" class:is-open={open}><Icon name="chevron-down" /></span>
      </button>
      <ul class="toc-list" id="toc-inline-list" hidden={!open}>
        {#each headings as heading (heading.id)}
          <li>
            <a class={['toc-link', heading.depth === 3 && 'is-sub']} href={`#${heading.id}`} onclick={() => (open = false)}>
              {heading.text}
            </a>
          </li>
        {/each}
      </ul>
    </nav>
  {/if}
{/if}

<style>
  .toc-title {
    padding: 0 var(--space-3) var(--space-2);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .toc-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .toc-list[hidden] {
    display: none;
  }

  .toc-link {
    display: block;
    padding: 5px var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    line-height: var(--lh-tight);
    text-decoration: none;
    transition:
      color 120ms ease,
      background-color 120ms ease;
  }

  .toc-link.is-sub {
    padding-left: calc(var(--space-3) + var(--space-3));
    font-size: var(--fs-sm);
  }

  @media (hover: hover) {
    .toc-link:hover {
      color: var(--c-text-strong);
      background: var(--c-control);
    }
  }

  /* 目前所在的段落：選中列樣式（底色＋框），不只靠顏色。 */
  .toc-link.is-active {
    background: var(--c-control-hover);
    border-color: var(--c-border-strong);
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .toc-link:focus-visible {
    outline-offset: -2px;
  }

  /* 內容上方的可收合版本：一塊有框的列，不包第二層卡片。 */
  .toc--inline {
    border: 1px solid var(--c-border);
    border-radius: var(--radius-item);
    background: var(--c-panel);
  }

  .toc-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 44px;
    padding: 0 var(--space-4);
    border: none;
    border-radius: var(--radius-item);
    background: transparent;
    color: var(--c-text-strong);
    font-size: var(--fs-md);
    font-weight: 600;
    cursor: pointer;
  }

  .toc-count {
    display: inline-flex;
    align-items: center;
    min-height: 20px;
    padding: 0 8px;
    border-radius: var(--radius-pill);
    background: var(--c-control-hover);
    color: var(--c-text-dim);
    font-size: var(--fs-xs);
  }

  .toc-chevron {
    display: inline-flex;
    color: var(--c-text-dim);
    transition: transform 220ms var(--ease-out);
  }

  .toc-chevron.is-open {
    transform: rotate(180deg);
  }

  .toc--inline .toc-list {
    padding: 0 var(--space-2) var(--space-2);
  }

  .toc--inline .toc-link {
    padding-top: 8px;
    padding-bottom: 8px;
    font-size: var(--fs-lg);
  }

  .toc--inline .toc-link.is-sub {
    font-size: var(--fs-md);
  }

  @media (prefers-reduced-motion: reduce) {
    .toc-link,
    .toc-chevron {
      transition-duration: 1ms;
    }
  }
</style>
