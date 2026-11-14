<script lang="ts">
  import type { Component } from 'svelte';
  import { dev } from '$app/env';
  import { demoEntry } from './registry';

  /**
   * 演示外框：<Demo id="swap" caption="…" />（.md 裡不用 import，建置時自動注入）。
   * - 依 registry.ts 找到元件，進入可視範圍（提早 200px）才載入；載入前用固定長寬比預留高度，版面不跳動。
   * - 元件檔還不存在時畫「演示製作中」的佔位框。
   * - 徽章標清楚資料來源：盤面演示「核心實際結果」、示意動畫「示意」。
   * 播放／暫停、重播、進出可視範圍時自動播放與暫停、prefers-reduced-motion，由各演示元件自己負責。
   */
  interface Props {
    id: string;
    caption?: string;
  }

  let { id, caption }: Props = $props();

  const entry = $derived(demoEntry(id));

  let frame = $state<HTMLElement | null>(null);
  let visible = $state(false);
  let Loaded = $state<Component<Record<string, unknown>> | null>(null);
  let failed = $state(false);
  /** 元件檔不存在（registry 有登記、檔案還沒做）。 */
  let missing = $state(false);

  $effect(() => {
    if (!frame || visible) return;
    if (typeof IntersectionObserver === 'undefined') {
      visible = true;
      return;
    }
    const observer = new IntersectionObserver(
      (records) => {
        if (records.some((record) => record.isIntersecting)) {
          visible = true;
          observer.disconnect();
        }
      },
      { rootMargin: '200px 0px' },
    );
    observer.observe(frame);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (!visible || !entry) return;
    const path = entry.path;
    let cancelled = false;
    // 元件清單放在 loaders.ts、用 import() 載入（原因見該檔），進入可視範圍才抓。
    import('./loaders')
      .then(({ loaders }) => {
        const load = loaders[path];
        if (!load) {
          if (!cancelled) missing = true;
          return;
        }
        return load().then((module) => {
          if (!cancelled) Loaded = module.default;
        });
      })
      .catch(() => {
        if (!cancelled) failed = true;
      });
    return () => {
      cancelled = true;
    };
  });

  const reserve = $derived(entry?.reserve);

  /**
   * 分段預留（registry.ts 的 steps）：每段一條 @container 規則，依 from 由小到大排，後面的蓋過前面的；
   * 觸控的放在 @media (pointer: coarse) 裡、排在最後。容器是外層的 figure（container-type: inline-size）。
   * 載入後（.is-loaded）不再套用。CSP 的 style-src 本來就允許行內樣式。
   */
  const stepsCss = $derived.by(() => {
    const steps = reserve?.steps;
    if (!steps) return '';
    const sel = `.demo .demo-stage.has-reserve[data-reserve="${id}"]:not(.is-loaded)`;
    const rule = ([from, k, b]: [number, number, number]) => {
      const body = `${sel}{height:calc(var(--w) * ${k} + ${b}px)}`;
      return from > 0 ? `@container (min-width: ${from}px){${body}}` : body;
    };
    return `<style>${steps.fine.map(rule).join('')}@media (pointer: coarse){${steps.coarse.map(rule).join('')}}</style>`;
  });
  const badge = $derived(entry?.kind === 'disc' ? '核心實際結果' : '示意');
</script>

<svelte:head>
  <!-- 內容只來自 registry.ts 的數字與 id（stepsCss）。 -->
  {@html stepsCss}
</svelte:head>

<figure class={['demo', entry?.path.startsWith('./disc/') && 'demo--disc']} bind:this={frame}>
  <div
    class="demo-stage"
    data-reserve={reserve?.steps ? id : undefined}
    class:is-loaded={Loaded !== null}
    class:has-reserve={reserve !== undefined}
    style:--demo-ratio={entry?.ratio ?? '16 / 10'}
    style:--rk-fine={reserve?.fine[0]}
    style:--rb-fine={reserve ? `${reserve.fine[1]}px` : undefined}
    style:--rk-coarse={reserve?.coarse[0]}
    style:--rb-coarse={reserve ? `${reserve.coarse[1]}px` : undefined}
    style:--r-chars={reserve?.chars}
  >
    {#if Loaded && entry}
      <Loaded {...entry.props} />
    {:else if !entry}
      <div class="demo-placeholder">
        <p>找不到這個演示</p>
        {#if dev}<p class="demo-id mono">id：{id}（請到 src/lib/demos/registry.ts 登記）</p>{/if}
      </div>
    {:else if missing || failed}
      <div class="demo-placeholder">
        <p>{failed ? '演示載入失敗' : '演示製作中'}</p>
        {#if dev}<p class="demo-id mono">{entry.path.replace('./', 'src/lib/demos/')}</p>{/if}
      </div>
    {:else}
      <div class="demo-placeholder is-waiting" aria-hidden="true"></div>
    {/if}
  </div>
  {#if entry || caption}
    <figcaption class="demo-caption">
      {#if entry}
        <span class={['badge', entry.kind === 'disc' ? 'badge--live' : 'badge--sample']}>{badge}</span>
      {/if}
      {#if caption}<span class="demo-text">{caption}</span>{/if}
    </figcaption>
  {/if}
</figure>

<style>
  .demo {
    margin: 1.5em 0;
    container-type: inline-size;
  }

  /* 演示的舞台：載入前依登記的長寬比預留高度；盤面演示最寬 480px 置中。 */
  .demo-stage {
    width: 100%;
    margin: 0 auto;
    aspect-ratio: var(--demo-ratio);
  }

  .demo--disc .demo-stage,
  .demo--disc .demo-caption {
    max-width: 480px;
  }

  /*
   * 有 reserve 的演示：高 = k × 舞台寬 + b（registry.ts），滑鼠與觸控各一組。
   * 盤面演示的原文比可用寬度（舞台寬 − 62px，原文等寬字約 7.62px）長時，滑鼠操作下多一條 8.5px 的捲軸。
   */
  .demo-stage.has-reserve {
    --w: 100cqi;
    --r-bar: clamp(0px, (var(--r-chars, 0) * 7.62px - var(--w) + 62px) * 100, 8.5px);
    aspect-ratio: auto;
    height: calc(var(--w) * var(--rk-fine) + var(--rb-fine) + var(--r-bar));
  }

  /* 盤面類（./disc/）的舞台最寬 480px；其他示意動畫撐滿內容寬。 */
  .demo--disc .demo-stage.has-reserve {
    --w: min(100cqi, 480px);
  }

  @media (pointer: coarse) {
    .demo-stage.has-reserve {
      height: calc(var(--w) * var(--rk-coarse) + var(--rb-coarse));
    }
  }

  .demo-stage.is-loaded,
  .demo-stage.has-reserve.is-loaded {
    aspect-ratio: auto;
    height: auto;
    min-height: 0;
  }

  .demo-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    width: 100%;
    height: 100%;
    padding: var(--space-4);
    border: 1px dashed var(--c-border-strong);
    border-radius: var(--radius-md);
    background: var(--c-panel);
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    text-align: center;
  }

  .demo-placeholder.is-waiting {
    border-style: solid;
    border-color: var(--c-border);
  }

  .demo-id {
    font-size: var(--fs-xs);
    overflow-wrap: anywhere;
  }

  .demo-caption {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    margin: var(--space-3) auto 0;
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
    text-align: center;
  }
</style>
