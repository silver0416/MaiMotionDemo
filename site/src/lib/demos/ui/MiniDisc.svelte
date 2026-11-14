<script lang="ts" module>
  /** 外圈鍵位 1–8 在盤面座標（圓心 0,0、外環半徑 1）的位置：1 號在正上方偏右，順時針。 */
  export function buttonPoint(button: number, radius = 1): { x: number; y: number } {
    const angle = ((-90 + 22.5 + (button - 1) * 45) * Math.PI) / 180;
    return { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius };
  }

  export interface DiscMark {
    button: number;
    /** select：選中（琥珀）；left／right：標了哪隻手；hit：手剛好拍到 */
    tone: 'select' | 'left' | 'right' | 'hit';
  }
</script>

<script lang="ts">
  /**
   * 示意用的極簡盤面：黑底、白色外環、八個鍵位點。只畫目前要看的那幾顆，不畫音符外形與軌跡；
   * 真正的盤面演示（核心實際結果）在 ../disc/。顏色沿用盤面固定色（disc-stage.md）。
   */
  interface Props {
    label: string;
    marks?: DiscMark[];
    slide?: { from: number; to: number; on: boolean } | null;
    /** 影片裡的手（盤面座標） */
    hand?: { x: number; y: number } | null;
    numbers?: boolean;
    /** 撐滿外層（外層決定大小，盤面置中、保持正方形） */
    fit?: boolean;
  }

  let { label, marks = [], slide = null, hand = null, numbers = true, fit = false }: Props = $props();

  const BUTTONS = [1, 2, 3, 4, 5, 6, 7, 8];
  const letter = (tone: DiscMark['tone']) => (tone === 'left' ? 'L' : tone === 'right' ? 'R' : '');
</script>

<svg class="mini-disc" class:is-fit={fit} viewBox="-1.3 -1.3 2.6 2.6" role="img" aria-label={label}>
  <rect x="-1.3" y="-1.3" width="2.6" height="2.6" class="floor" />
  <circle r="1" class="ring" />
  <circle r="0.42" class="inner" />
  {#if slide}
    {@const a = buttonPoint(slide.from)}
    {@const b = buttonPoint(slide.to)}
    <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} class="slide" class:is-on={slide.on} />
  {/if}
  {#each BUTTONS as button (button)}
    {@const p = buttonPoint(button)}
    <circle cx={p.x} cy={p.y} r="0.045" class="dot" />
    {#if numbers}
      {@const q = buttonPoint(button, 1.17)}
      <text x={q.x} y={q.y} class="num">{button}</text>
    {/if}
  {/each}
  {#each marks as mark (`${mark.button}-${mark.tone}`)}
    {@const p = buttonPoint(mark.button)}
    <g class="mark" class:is-left={mark.tone === 'left'} class:is-right={mark.tone === 'right'} class:is-hit={mark.tone === 'hit'}>
      <circle cx={p.x} cy={p.y} r={mark.tone === 'select' || mark.tone === 'hit' ? 0.22 : 0.15} />
      {#if letter(mark.tone)}
        <text x={p.x} y={p.y}>{letter(mark.tone)}</text>
      {/if}
    </g>
  {/each}
  {#if hand}
    <circle cx={hand.x} cy={hand.y} r="0.11" class="hand" />
  {/if}
</svg>

<style>
  .mini-disc {
    display: block;
    width: 100%;
    height: auto;
    aspect-ratio: 1;
    border-radius: var(--radius-md);
  }

  .mini-disc.is-fit {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    aspect-ratio: auto;
  }

  .floor {
    fill: #000;
  }

  .ring {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 0.025;
  }

  .inner {
    fill: none;
    stroke: #4d5765;
    stroke-width: 0.012;
  }

  .dot {
    fill: #8d97a6;
  }

  .num {
    fill: #7b8595;
    font-family: var(--font-mono);
    font-size: 0.15px;
    font-weight: 700;
    text-anchor: middle;
    dominant-baseline: central;
  }

  .slide {
    stroke: #6d7788;
    stroke-width: 0.06;
    stroke-linecap: round;
    stroke-dasharray: 0.09 0.07;
  }

  .slide.is-on {
    stroke: #eef2f8;
    stroke-dasharray: none;
  }

  .mark circle {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 0.05;
  }

  .mark.is-left circle {
    fill: var(--c-left);
    stroke: var(--c-left);
  }

  .mark.is-right circle {
    fill: var(--c-right);
    stroke: var(--c-right);
  }

  .mark.is-hit circle {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 0.07;
  }

  .mark text {
    fill: #0b0d11;
    font-family: var(--font-mono);
    font-size: 0.17px;
    font-weight: 700;
    text-anchor: middle;
    dominant-baseline: central;
  }

  .hand {
    fill: #eef2f8;
    stroke: #0b0d11;
    stroke-width: 0.03;
  }
</style>
