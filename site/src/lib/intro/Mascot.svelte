<script lang="ts">
  /**
   * 實驗：吉祥物對話框。站上的標誌（Logo：圓環加粉、藍兩點）當吉祥物，旁邊一個漫畫式對話框，放一兩句補充。
   *
   * 可以整個拿掉：只在 src/content/intro/*.md 以 <Mascot>…</Mascot> 使用，其他元件都不依賴它；
   * 刪掉那些標籤與這個檔案即可（markdown.js 依 src/lib/intro/ 實際有的檔案自動 import，不必另外改）。
   * 對話框只放補充或重述，正文不能只存在這裡。
   *
   * 生命感只有一個：第一次捲進畫面時兩個點眨一下眼（一次、不循環），滑鼠移上去也會眨；減少動態時完全靜止。
   * 像聊天軟體：一段話一個泡泡，可以連續好幾個（最多四個）；標誌（頭像）在最後一個泡泡旁邊、和它上下置中，
   * 只有那個泡泡有尖角（左邊正中間，指向頭像）。
   * at="top"：頭像改放在第一個泡泡旁邊，尖角在第一個泡泡上。
   * look="right"：兩個點往右偏，像是看向右邊。第一次捲進畫面時先眨眼，再從正中間飄到右邊，之後停在右邊（一次、不循環）；
   *   減少動態或沒有 JS 時一開始就在右邊。
   * href＋action：泡泡右邊放一顆主要按鈕（連結），窄螢幕換到泡泡下面；有按鈕時只放一段話。
   *   <Mascot look="right" href="/quick/download" action="進入基礎操作演示">
   * 對話框下面還有很多內容時用，讓視線從頭像往下讀；對話框是一段的結尾時用預設（底部）。
   * 用法（標籤與每段話之間都要空一行，mdsvex 才會把每段轉成 <p>）：
   *   <Mascot>
   *
   *   第一句。
   *
   *   第二句。
   *
   *   </Mascot>
   */
  import { onMount, type Snippet } from 'svelte';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';
  import Logo from '$lib/shell/Logo.svelte';

  interface Props {
    at?: 'top' | 'bottom';
    /** 眼睛看的方向 */
    look?: 'right';
    /** 泡泡右邊那顆按鈕要去的地方；和 action 一起給 */
    href?: string;
    /** 按鈕上的字 */
    action?: string;
    children?: Snippet;
  }

  let { at = 'bottom', look, href, action, children }: Props = $props();
  const hasAction = $derived(Boolean(href && action));

  let root: HTMLElement;
  let blinking = $state(false);
  /** look="right"：true 時兩點先待在正中間，等捲進畫面、眨完眼才飄到右邊。 */
  let armed = $state(false);
  /** 已經看過去了：這時才加轉場，啟動時把兩點移回正中間那一下不會被看到在倒退。 */
  let looked = $state(false);

  onMount(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches || !('IntersectionObserver' in window)) return;
    let timer = 0;
    let glance = 0;
    if (look === 'right') armed = true;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        timer = window.setTimeout(() => (blinking = true), 600);
        // 眨眼是 200ms × 2，眨完再看過去。
        if (look !== 'right') return;
        glance = window.setTimeout(() => {
          looked = true;
          armed = false;
        }, 1100);
      },
      { threshold: 0.6 },
    );
    observer.observe(root);
    return () => {
      observer.disconnect();
      window.clearTimeout(timer);
      window.clearTimeout(glance);
    };
  });
</script>

<div
  class={['mascot', at === 'top' && 'is-top', look === 'right' && 'looks-right', armed && 'is-armed', looked && 'is-looked', hasAction && 'has-action']}
  role="note"
  bind:this={root}
>
  <span class={['mascot-face', blinking && 'is-blinking']} aria-hidden="true"><Logo size={44} /></span>
  {@render children?.()}
  {#if hasAction}
    <a class="btn btn--primary mascot-action" {href}>{action}<ExtraIcon name="arrow-right" /></a>
  {/if}
</div>

<style>
  /* 標誌在第一欄，每段話（<p>）是第二欄的一列。標誌和它旁邊那個泡泡同一列、上下置中。 */
  .mascot {
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr);
    column-gap: var(--space-3);
    row-gap: var(--space-1);
  }

  .mascot-face {
    display: flex;
    grid-row: 1;
    grid-column: 1;
    align-self: center;
    color: var(--c-text-strong);
  }

  /* 預設（頭像在下）：標誌放到最後一個泡泡那一列。列數不固定，所以依泡泡的數量逐一指定（最多四個）。 */
  .mascot:not(.is-top):has(> :global(p:nth-of-type(2))) .mascot-face {
    grid-row: 2;
  }

  .mascot:not(.is-top):has(> :global(p:nth-of-type(3))) .mascot-face {
    grid-row: 3;
  }

  .mascot:not(.is-top):has(> :global(p:nth-of-type(4))) .mascot-face {
    grid-row: 4;
  }

  /* 泡泡：不透明的控制項底色＋明顯的框；只跟文字一樣寬，文字太長才換行。 */
  .mascot > :global(p) {
    position: relative;
    grid-column: 2;
    justify-self: start;
    margin: 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-pop);
    background: var(--c-control);
    color: var(--c-text);
  }

  /* 尖角：頭像旁邊那個泡泡的左邊正中間，一個轉 45° 的小方塊，只畫外側兩條邊，同色蓋住泡泡的框線（半邊 6px）。 */
  .mascot:not(.is-top) > :global(p:last-of-type::before),
  .mascot.is-top > :global(p:first-of-type::before) {
    content: '';
    position: absolute;
    top: calc(50% - 6px);
    left: -7px;
    width: 12px;
    height: 12px;
    border-bottom: 1px solid var(--c-border-strong);
    border-left: 1px solid var(--c-border-strong);
    background: var(--c-control);
    transform: rotate(45deg);
  }

  /* 泡泡右邊的按鈕：泡泡那一欄只佔文字的寬度，按鈕緊跟在後面、和泡泡上下置中。 */
  .mascot.has-action {
    grid-template-columns: 44px auto minmax(0, 1fr);
  }

  .mascot-action {
    grid-row: 1;
    grid-column: 3;
    align-self: center;
    justify-self: start;
  }

  @media (width < 560px) {
    /* 窄螢幕：按鈕換到泡泡下面，和泡泡左緣對齊。 */
    .mascot.has-action {
      grid-template-columns: 44px minmax(0, 1fr);
      row-gap: var(--space-2);
    }

    .mascot-action {
      grid-row: 2;
      grid-column: 2;
    }
  }

  /* 看向右邊：兩個點一起往右偏（viewBox 的 6 單位，右邊的點仍在環內）。用 translate 而不是 transform，眨眼的 transform 才不會把它蓋掉。 */
  .looks-right :global(.logo-left),
  .looks-right :global(.logo-right) {
    translate: 6px 0;
  }

  .looks-right.is-looked :global(.logo-left),
  .looks-right.is-looked :global(.logo-right) {
    transition: translate 420ms var(--ease-spring);
  }

  /* 還沒看過去：先待在正中間（只有 JS 啟動、而且不是減少動態時才會有這個狀態）。 */
  .looks-right.is-armed :global(.logo-left),
  .looks-right.is-armed :global(.logo-right) {
    translate: 0 0;
  }

  /* 眨眼：兩個點（左手粉、右手藍，Logo 本來的顏色）上下壓扁一下，連眨兩次後停住。 */
  .is-blinking :global(.logo-left),
  .is-blinking :global(.logo-right) {
    transform-box: fill-box;
    transform-origin: center;
    animation: mascot-blink 200ms ease-in-out 2;
  }

  @media (hover: hover) {
    .mascot:hover :global(.logo-left),
    .mascot:hover :global(.logo-right) {
      transform-box: fill-box;
      transform-origin: center;
      animation: mascot-blink 200ms ease-in-out 2;
    }
  }

  @keyframes mascot-blink {
    50% {
      transform: scaleY(0.15);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .looks-right.is-looked :global(.logo-left),
    .looks-right.is-looked :global(.logo-right) {
      transition: none;
    }

    .is-blinking :global(.logo-left),
    .is-blinking :global(.logo-right),
    .mascot:hover :global(.logo-left),
    .mascot:hover :global(.logo-right) {
      animation: none;
    }
  }
</style>
