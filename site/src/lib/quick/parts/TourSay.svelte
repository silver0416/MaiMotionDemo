<script lang="ts">
  import { untrack } from 'svelte';
  import Logo from '$lib/shell/Logo.svelte';

  /**
   * 實際畫面導覽（QuickTour）的旁白泡泡：左邊泡泡、右邊吉祥物（站上的標誌），像吉祥物在講解。
   * 只是畫面上的樣子：整塊 aria-hidden，旁白給輔助技術念的是 Stage 裡視覺隱藏的 aria-live 那一行。
   *
   * - 寬的時候疊在截圖右下角；這一步要按（或框起來）的地方和泡泡重疊時改放右上角（吉祥物仍在右邊），
   *   兩個角都會碰到就放重疊比較少的那一角。位置在新句子出現的那一刻決定、同一句話中途不動（鏡頭中途拉遠也不跳）；
   *   舞台寬度改變時重算。量尺寸在繪製前（effect 裡），不會先在右下閃一下再跳。
   * - 窄的時候（容器查詢）不蓋在截圖上，改成截圖正下方的一列，保留兩行高，每一步不上下跳。
   * - 換句：舊句子整句往上淡出（OUT_MS），淡出完才換成新句子（泡泡大小直接換），再一個字一個字淡入；
   *   整句從第一個字到最後一個字開始淡入的時間差不超過 SPREAD_MS，每個字淡入 100ms（CSS），約 0.25 秒看得完整。
   *   連續快速換句時永遠以最新的句子為準。換到下一步但句子一樣（連續兩步都是「按 D 標右手」）時也播一次，看得出換了一步。
   * - 減少動態：直接換字，沒有動畫。伺服器端算繪／沒有 JS：直接顯示第一句；載入時（hydration）不播，只有句子真的變了才播。
   */
  interface Box {
    left: number;
    top: number;
    width: number;
    height: number;
  }

  interface Props {
    /** 目前的旁白 */
    text: string;
    /** 目前第幾步：換步時就算句子一樣也重播一次（同一步裡的小動作不重播） */
    step: number;
    reduced: boolean;
    /** 這一步要點的地方（舞台像素，鏡頭停好之後）；沒有就是 null */
    avoid: Box | null;
    /** 截圖視窗的寬高（舞台像素） */
    width: number;
    height: number;
  }

  let { text, step, reduced, avoid, width, height }: Props = $props();

  /** 舊句子往上淡出的時間（和 CSS 一致）。 */
  const OUT_MS = 130;
  /** 第一個字到最後一個字開始淡入的最大時間差、每個字之間的最大間隔。 */
  const SPREAD_MS = 160;
  const STEP_MAX = 14;
  /** 泡泡離截圖邊緣（var(--space-3)）、要點四周多留的空間。 */
  const INSET = 12;
  const CLEAR = 12;

  let shown = $state(untrack(() => text));
  /** still：靜止（第一句、減少動態）；in：一個字一個字淡入；out：整句往上淡出。 */
  let phase = $state<'still' | 'in' | 'out'>('still');
  /** 每換一句加一：重建字的 span，動畫從頭播；也觸發重新決定位置。 */
  let gen = $state(0);
  let corner = $state<'bottom' | 'top'>('bottom');
  let root = $state<HTMLElement | null>(null);
  /** 淡出中又換句時，淡出完要顯示的那一句（永遠是最新的）。 */
  let pending = '';
  /** 上一次處理到第幾步。 */
  let lastStep = untrack(() => step);
  let outTimer: ReturnType<typeof setTimeout> | undefined;

  const chars = $derived(Array.from(shown));
  const stepMs = $derived(chars.length > 0 ? Math.min(STEP_MAX, SPREAD_MS / chars.length) : 0);

  function show(next: string, animate: boolean) {
    shown = next;
    phase = animate ? 'in' : 'still';
    gen += 1;
  }

  function change(next: string) {
    pending = next;
    if (reduced) {
      clearTimeout(outTimer);
      outTimer = undefined;
      show(next, false);
      return;
    }
    // 已經在淡出：等它淡出完，直接換成最新的這一句。
    if (phase === 'out') return;
    phase = 'out';
    outTimer = setTimeout(() => {
      outTimer = undefined;
      show(pending, true);
    }, OUT_MS);
  }

  $effect(() => {
    const next = text;
    const at = step;
    untrack(() => {
      const moved = at !== lastStep;
      lastStep = at;
      if (moved || next !== (phase === 'out' ? pending : shown)) change(next);
    });
  });

  $effect(() => () => clearTimeout(outTimer));

  const area = (a: Box, b: Box) =>
    Math.max(0, Math.min(a.left + a.width, b.left + b.width) - Math.max(a.left, b.left)) *
    Math.max(0, Math.min(a.top + a.height, b.top + b.height) - Math.max(a.top, b.top));

  /** 新句子出現（gen 變了）或舞台寬度改變時，決定放右下還是右上。DOM 已經是新句子，還沒繪製。 */
  $effect(() => {
    void gen;
    void width;
    void height;
    untrack(() => {
      if (!root || !avoid || getComputedStyle(root).position !== 'absolute') {
        corner = 'bottom';
        return;
      }
      const w = root.offsetWidth;
      const h = root.offsetHeight;
      const keep = {
        left: avoid.left - CLEAR,
        top: avoid.top - CLEAR,
        width: avoid.width + CLEAR * 2,
        height: avoid.height + CLEAR * 2,
      };
      const left = width - INSET - w;
      const bottom = area({ left, top: height - INSET - h, width: w, height: h }, keep);
      const top = bottom > 0 ? area({ left, top: INSET, width: w, height: h }, keep) : Infinity;
      corner = top < bottom ? 'top' : 'bottom';
    });
  });
</script>

<div class={['tour-say', corner === 'top' && 'is-top']} bind:this={root} aria-hidden="true">
  <p class="say-bubble">
    {#key gen}
      <span class={['say-text', phase === 'out' && 'is-leaving']}>
        {#if phase === 'still'}
          {shown}
        {:else}
          {#each chars as ch, i}<span class="ch" style:animation-delay={`${Math.round(i * stepMs)}ms`}>{ch}</span>{/each}
        {/if}
      </span>
    {/key}
  </p>
  <span class="say-face"><Logo size={36} /></span>
</div>

<style>
  /* 寬的時候：疊在截圖視窗右下角（右上角見 .is-top）。z-index 在要點的框（2）之上、游標（5）之下。 */
  .tour-say {
    position: absolute;
    right: var(--space-3);
    bottom: var(--space-3);
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-3);
    /* 泡泡最寬 min(60%, 22rem)，加上吉祥物（44px）與間距。 */
    max-width: calc(min(60%, 22rem) + 44px + var(--space-3));
    pointer-events: none;
  }

  .tour-say.is-top {
    top: var(--space-3);
    bottom: auto;
  }

  /* 泡泡：同 intro 的 Mascot（不透明的控制項底色＋明顯的框），尖角在右邊正中間、指向吉祥物。 */
  .say-bubble {
    position: relative;
    flex: 0 1 auto;
    min-width: 0;
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-pop);
    background: var(--c-control);
    color: var(--c-text);
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
  }

  .say-bubble::after {
    content: '';
    position: absolute;
    top: calc(50% - 6px);
    right: -7px;
    width: 12px;
    height: 12px;
    border-top: 1px solid var(--c-border-strong);
    border-right: 1px solid var(--c-border-strong);
    background: var(--c-control);
    transform: rotate(45deg);
  }

  /* 吉祥物：蓋在截圖（深色的 App 畫面）上，底下墊一塊不透明的圓，淺色配色也看得清楚。 */
  .say-face {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border: 1px solid var(--c-border-strong);
    border-radius: 50%;
    background: var(--c-control);
    color: var(--c-text-strong);
  }

  .say-text {
    display: block;
  }

  /* 一個字一個字淡入：只動 opacity；字是 inline（不是 inline-block），中文標點的換行規則不受影響。 */
  .ch {
    animation: say-in 100ms ease-out both;
  }

  @keyframes say-in {
    from {
      opacity: 0;
    }
  }

  /* 舊句子整句往上淡出（130ms，和 OUT_MS 一致），停在看不見的狀態等新句子。 */
  .say-text.is-leaving {
    animation: say-out 130ms ease-in forwards;
  }

  @keyframes say-out {
    to {
      opacity: 0;
      transform: translateY(-6px);
    }
  }

  /* 窄的時候：截圖本身已經很小，泡泡不蓋在上面，改成截圖正下方的一列（靠右、吉祥物在右）；保留兩行高，每一步不上下跳。 */
  @container demo (width < 560px) {
    .tour-say {
      position: static;
      max-width: none;
      min-height: calc(2em * 1.6 + 2 * var(--space-2) + 2px + 2 * var(--space-2));
      padding: var(--space-2) var(--space-3);
      border-top: 1px solid var(--c-border);
      background: var(--c-panel);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .ch,
    .say-text.is-leaving {
      animation: none;
    }
  }
</style>
