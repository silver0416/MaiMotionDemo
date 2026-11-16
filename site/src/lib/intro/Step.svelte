<script lang="ts">
  /**
   * <Steps> 裡的一步（<li>）。
   * - 一般清單：第一行是標題列（圓形圖示＋標題，編號靠右），內文在下面、和頁面左緣對齊。
   *   .md 裡第一段寫「**標題**」、空一行後寫內文（標題列取的是第一段）。
   * - flow（流程圖）：左邊一顆圓形圖示，右邊一塊面板（編號、選填的分層標籤、「**粗體小標**：內文」）。
   * - icon：IntroIcon 的名字（裝飾，aria-hidden）。
   * - key：重點步驟。內文改成淡主色的一塊、圖示改成主色；不用左側強調邊框。
   * - layer：分層標籤（例如「第一層・演算法」），只是把內文已經說的分層再標一次，不放新的資訊。
   * 內文一律寫在 .md 裡（children），才會進搜尋索引。
   */
  import type { Snippet } from 'svelte';
  import IntroIcon, { type IntroIconName } from './IntroIcon.svelte';

  interface Props {
    icon: IntroIconName;
    key?: boolean;
    layer?: string;
    children?: Snippet;
  }

  let { icon, key = false, layer, children }: Props = $props();
</script>

<li class={['step', key && 'is-key']}>
  <span class="step-tile" aria-hidden="true"><IntroIcon name={icon} size={20} /></span>
  <div class="step-body">
    <div class="step-meta">
      <span class="step-num mono" aria-hidden="true"></span>
      {#if layer}<span class="badge">{layer}</span>{/if}
    </div>
    {@render children?.()}
  </div>
</li>

<style>
  .step {
    position: relative;
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
    margin: 0;
    padding: var(--space-4) 0;
    counter-increment: intro-step;
  }

  .step-tile {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-pill);
    background: var(--c-control-hover);
    color: var(--c-text-strong);
  }

  .step-body {
    min-width: 0;
  }

  .step-body > :global(p) {
    margin: 0;
  }

  .step-body > :global(p + p) {
    margin-top: 0.5em;
  }

  .step-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-height: 20px;
    margin-bottom: var(--space-1);
  }

  .step-num {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .step-num::before {
    content: counter(intro-step, decimal-leading-zero);
  }

  /* 一般清單：列與列之間一條分隔線，第一列與最後一列不留外側空白。 */
  :global(.steps:not(.steps--flow)) > .step:not(:first-child) {
    border-top: 1px solid var(--c-border);
  }

  :global(.steps:not(.steps--flow)) > .step:first-child {
    padding-top: 0;
  }

  :global(.steps:not(.steps--flow)) > .step:last-child {
    padding-bottom: 0;
  }

  /* 一般清單的版面：標題列（圖示、標題、靠右的編號）一行，內文在下面佔滿整欄。
     .step-body 不佔版面（display: contents），裡面的第一段是標題、其餘是內文，直接排進這一步的格線。 */
  :global(.steps:not(.steps--flow)) > .step {
    grid-template-columns: 36px minmax(0, 1fr) auto;
    gap: var(--space-2) var(--space-3);
    align-items: center;
  }

  :global(.steps:not(.steps--flow)) > .step .step-tile {
    grid-row: 1;
    grid-column: 1;
    width: 36px;
    height: 36px;
  }

  :global(.steps:not(.steps--flow)) > .step .step-body {
    display: contents;
  }

  :global(.steps:not(.steps--flow)) > .step .step-meta {
    grid-row: 1;
    grid-column: 3;
    min-height: 0;
    margin: 0;
  }

  :global(.steps:not(.steps--flow)) > .step .step-body > :global(p) {
    grid-column: 1 / -1;
    margin: 0;
  }

  :global(.steps:not(.steps--flow)) > .step .step-body > :global(p:first-of-type) {
    grid-row: 1;
    grid-column: 2;
    color: var(--c-text-strong);
    font-size: var(--fs-xl);
    line-height: var(--lh-tight);
  }

  /* 重點步驟：內文是一塊淡主色（不透明，color-mix 進面板色），圖示換主色。 */
  .is-key .step-tile {
    background: var(--c-primary);
    color: var(--c-on-primary);
  }

  .is-key .step-body {
    padding: var(--space-3) var(--space-4);
    border: 1px solid color-mix(in srgb, var(--c-primary) 40%, var(--c-panel));
    border-radius: var(--radius-item);
    background: color-mix(in srgb, var(--c-primary) 10%, var(--c-panel));
  }

  /* 流程圖：每步的內文是一塊面板；圖示之間畫一條連接線。 */
  :global(.steps--flow) > .step {
    /* 圖示和右邊那塊面板上下置中。 */
    align-items: center;
    padding: 0;
  }

  :global(.steps--flow) > .step .step-body {
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-item);
    background: var(--c-panel);
  }

  :global(.steps--flow) > .step.is-key .step-body {
    border-color: color-mix(in srgb, var(--c-primary) 40%, var(--c-panel));
    background: color-mix(in srgb, var(--c-primary) 10%, var(--c-panel));
  }

  /* 連接線：線寬 2、置中在 40px 的圖示下，離圖示上下緣各留 6px。
     圖示在每一步的正中間、每一步高度不同，所以一條線拆成兩半：
     這一步的下半段（::after）畫到步驟間距的中點，下一步的上半段（::before）從那裡接上。 */
  :global(.steps--flow) > .step:not(:last-child)::after,
  :global(.steps--flow) > .step:not(:first-child)::before {
    content: '';
    position: absolute;
    left: 19px;
    width: 2px;
    background: var(--c-border-strong);
  }

  :global(.steps--flow) > .step:not(:last-child)::after {
    top: calc(50% + 26px);
    bottom: calc(var(--space-4) / -2);
  }

  :global(.steps--flow) > .step:not(:first-child)::before {
    top: calc(var(--space-4) / -2);
    bottom: calc(50% + 26px);
  }

  /* 進場：由上往下依序淡入、上移 6px（只動 opacity 與 transform）。fill-mode backwards：動畫前就是起始狀態，結束後回到正常樣式。 */
  :global(.steps--flow) > .step {
    animation: step-in 260ms var(--ease-out) backwards;
  }

  :global(.steps--flow) > .step:nth-child(2) {
    animation-delay: 90ms;
  }

  :global(.steps--flow) > .step:nth-child(3) {
    animation-delay: 180ms;
  }

  :global(.steps--flow) > .step:nth-child(4) {
    animation-delay: 270ms;
  }

  :global(.steps--flow) > .step:nth-child(n + 5) {
    animation-delay: 360ms;
  }

  @keyframes step-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.steps--flow) > .step {
      animation: none;
    }
  }
</style>
