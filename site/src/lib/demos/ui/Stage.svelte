<!--
  css="injected"：實際畫面導覽（$lib/quick/QuickTour）在伺服器端就算繪這個外框。Stage 同時被延遲載入的演示共用，
  Vite 把它的 CSS 拆成另一個檔，SvelteKit 不會放進預先產生的 HTML 的 <head>：沒有 JS 時外框沒有樣式，
  有 JS 時等 CSS 載入才套上、版面會跳。改成由元件自己帶樣式（伺服器端算繪時直接寫進 <head>）。
-->
<svelte:options css="injected" />

<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '$app/components/Icon.svelte';
  import Cursor from './Cursor.svelte';
  import { demoKey, type DemoPlayer } from './player.svelte';

  /**
   * 示意動畫的外框：內容區＋下方一列（旁白、步數、播放／暫停、重播）。
   * - 旁白用 aria-live="polite"，腳本每一步與使用者操作的回饋都寫在這裡。
   * - 按鍵只在焦點位於演示內時處理（onkey 回傳 true 才 preventDefault），不攔截整頁的 Ctrl K、/。
   *   點演示裡的空白處也會把焦點放進來（tabindex=-1，不進 Tab 順序）。
   * - 沒有 player 時（互動地圖）只顯示旁白，不畫播放鈕。
   * - flush：內容區不留內距（實際畫面導覽：截圖貼齊外框）。
   * - steps：多一組「上一步／下一步」，步數改顯示這裡給的 at／total（腳本的一步可能拆成好幾個小動作）。
   * - narration="hidden"：旁白改由演示自己畫在內容區裡（實際畫面導覽的吉祥物泡泡）。下方那一列只剩控制鈕；
   *   旁白那一行仍在 DOM 裡（全域的 .sr-only，視覺隱藏），aria-live 照樣給輔助技術念。預設 'foot'：旁白在下方那一列左邊。
   */
  interface StepNav {
    /** 目前第幾步（從 1 起算；還沒開始是 0） */
    at: number;
    total: number;
    prev: () => void;
    next: () => void;
    canPrev: boolean;
    canNext: boolean;
  }

  interface Props {
    label: string;
    player?: DemoPlayer;
    /** 沒有 player 時的旁白 */
    say?: string;
    /** 'bg'：用視窗底色（App 主視窗的縮圖） */
    surface?: 'panel' | 'bg';
    flush?: boolean;
    steps?: StepNav;
    /** 'hidden'：旁白只給輔助技術（畫面上由演示自己顯示） */
    narration?: 'foot' | 'hidden';
    onkey?: (key: string, event: KeyboardEvent) => boolean;
    children: Snippet;
  }

  let {
    label,
    player,
    say = '',
    surface = 'panel',
    flush = false,
    steps,
    narration = 'foot',
    onkey,
    children,
  }: Props = $props();

  let body = $state<HTMLElement | null>(null);

  $effect(() => {
    if (player) player.root = body;
  });

  function onkeydown(event: KeyboardEvent) {
    const key = demoKey(event);
    if (key === null || !onkey) return;
    if (onkey(key, event)) event.preventDefault();
  }

  const playLabel = $derived(
    !player ? '' : player.playing ? '暫停' : player.done || player.manual ? '再看一次' : player.index < 0 ? '播放' : '繼續',
  );
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="demo-ui"
  class:is-bg={surface === 'bg'}
  role="group"
  aria-label={label}
  tabindex="-1"
  {onkeydown}
  {@attach (node) => player?.attach(node)}
>
  <div class="demo-body" class:is-flush={flush} bind:this={body}>
    {@render children()}
    {#if player}
      <Cursor x={player.cursor.x} y={player.cursor.y} show={player.cursor.show} press={player.cursor.press} />
    {/if}
  </div>
  <div class="demo-foot">
    <p class="narration" class:sr-only={narration === 'hidden'} aria-live="polite">{player ? player.say : say}</p>
    {#if player}
      <div class="demo-ctrl">
        {#if steps}
          <span class="mono xsmall muted count" aria-hidden="true">{steps.at}／{steps.total}</span>
          <button class="btn btn--icon" onclick={steps.prev} disabled={!steps.canPrev} aria-label="上一步">
            <Icon name="chevron-left" size={14} /><span class="nav-text">上一步</span>
          </button>
          <button class="btn btn--icon" onclick={steps.next} disabled={!steps.canNext} aria-label="下一步">
            <span class="nav-text">下一步</span><Icon name="chevron-right" size={14} />
          </button>
        {:else if !player.manual}
          <span class="mono xsmall muted count" aria-hidden="true">{Math.max(0, player.index + 1)}／{player.total}</span>
        {/if}
        <button
          class="btn btn--icon"
          onclick={() => player.toggle()}
          aria-label={player.playing ? '暫停示範' : player.done || player.manual ? '從頭再看一次示範' : '播放示範'}
        >
          <Icon name={player.playing ? 'pause' : 'play'} size={14} />{playLabel}
        </button>
        <button class="btn btn--icon" onclick={() => player.restart()} aria-label="重播示範" title="重播示範">
          <Icon name="rotate-ccw" size={14} />
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  /* 容器查詢的名稱 demo：各演示依自己的寬度（不是視窗寬度）改排列。 */
  .demo-ui {
    container: demo / inline-size;
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
    color: var(--c-text);
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
  }

  .demo-ui.is-bg {
    background: var(--c-bg);
  }

  .demo-ui:focus {
    outline: none;
  }

  .demo-ui:focus-visible {
    outline: 2px solid var(--c-focus);
    outline-offset: 2px;
  }

  .demo-body {
    position: relative;
    min-width: 0;
    padding: var(--space-4);
  }

  .demo-body.is-flush {
    padding: 0;
  }

  .demo-foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-2) var(--space-3) var(--space-2) var(--space-4);
    border-top: 1px solid var(--c-border);
    background: var(--c-panel);
  }

  /* 旁白保留兩行高：每一步的字數不同，下方按鈕不會上下跳。 */
  .narration {
    flex: 1 1 16rem;
    min-width: 0;
    min-height: calc(2em * 1.6);
    display: flex;
    align-items: center;
    font-size: var(--fs-md);
    line-height: 1.6;
    color: var(--c-text);
  }

  /* 視覺隱藏（全域 .sr-only）時不保留兩行高、不當 flex 容器，免得撐出一塊看不見的區域。 */
  .narration.sr-only {
    display: block;
    min-height: 0;
  }

  .demo-ctrl {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-left: auto;
  }

  .count {
    min-width: 5ch;
    text-align: right;
  }

  @container demo (width < 420px) {
    .demo-body:not(.is-flush) {
      padding: var(--space-3);
    }

    /* 窄的時候「上一步／下一步」只留箭頭（aria-label 仍是完整的字）。 */
    .nav-text {
      display: none;
    }
  }
</style>
