<script lang="ts">
  /**
   * 示意：盤面圖例。一個靜態盤面擺齊各種圖形，下方一份清單；
   * 指到或點清單項目，盤面上對應的圖形加亮、其餘變暗；點盤面上的圖形也會選到清單項目。
   * 位置是為了說明擺的，不是核心算出的結果（外框會標「示意」）。筆觸與 App 的 DiscStage 相同。
   */
  import './disc.css';
  import DiscBoard from '$app/components/DiscBoard.svelte';
  import { BUTTONS, buttonPoint } from '$app/lib/disc';
  import { scalePoint } from '$app/lib/notes';
  import { touchFans, touchRadius } from '$app/lib/touch';
  import type { PathSample, Point, SlidePath } from '$app/lib/types';
  import {
    CENTER,
    DISC_RADIUS,
    EX_GLOW_BLUR_PX,
    NOTE_DOT,
    NOTE_RADIUS,
    STAR_DOT,
    TOUCH_DOT_RADIUS,
    TOUCH_DOT_RIM_RADIUS,
    VIEW_BOX,
    bandWidth,
    borderFill,
    diamondPath,
    edgeWidth,
    exGlowOffset,
    exGlowWidth,
    fanBody,
    holdPath,
    len,
    linePath,
    palmCrossPath,
    px,
    slideTracks,
    starOnPath,
    touchHoldBand,
    touchHoldQuarters,
  } from './draw';

  type ItemId =
    | 'tap'
    | 'hold'
    | 'slide'
    | 'touch'
    | 'touchHold'
    | 'break'
    | 'ex'
    | 'left'
    | 'right'
    | 'trail'
    | 'tags'
    | 'handover'
    | 'palm';

  /** 用語對照 src/content/start/disc.md。 */
  const ITEMS: { id: ItemId; name: string; text: string }[] = [
    { id: 'tap', name: 'Tap', text: '粗圓環，正中心一個小點' },
    { id: 'hold', name: 'Hold', text: '空心長條，長度代表要按多久' },
    { id: 'slide', name: 'Slide', text: '星星沿著一整排箭頭滑過去，走過的箭頭會消失' },
    { id: 'touch', name: 'Touch', text: '四片三角形往落點收攏，尖端相碰就是判定' },
    { id: 'touchHold', name: 'Touch Hold', text: '斜向的四片三角形；按住後外框從正上方亮起，亮滿一圈就放開' },
    { id: 'break', name: 'Break', text: '換成橘色，形狀不變' },
    { id: 'ex', name: 'EX', text: '音符周圍多一圈柔和的光暈' },
    { id: 'left', name: '左手 L', text: '粉紅色；碰到盤面時實心，外面多一圈接觸環' },
    { id: 'right', name: '右手 R', text: '藍色；沒碰盤面時空心，下方寫目前在做什麼' },
    { id: 'trail', name: '最近軌跡', text: '手後面拖著的粗線，是最近一小段的移動' },
    { id: 'tags', name: '音符旁的標籤', text: '這顆分給哪隻手：L、R，中途換手寫 L→R' },
    { id: 'handover', name: '換手菱形', text: 'Slide 中途換手的位置，上面寫「換手 L→R」' },
    { id: 'palm', name: '手掌虛線圓', text: '一隻手掌同時蓋住好幾個 Touch' },
  ];

  /** EX 光暈用的模糊濾鏡 id。 */
  const uid = $props.id();
  const exGlowId = `${uid}-ex-glow`;

  let pinned = $state<ItemId | null>(null);
  let hovered = $state<ItemId | null>(null);
  const active = $derived(hovered ?? pinned);

  function pick(id: ItemId) {
    pinned = pinned === id ? null : id;
  }

  /** 圖形是否變暗：有選取時，不屬於選取項目的都變暗。 */
  function dim(id: ItemId): boolean {
    return active !== null && active !== id;
  }

  // ---- 擺位（盤面座標，圓心 0,0、外環半徑 1） ----

  const P1 = buttonPoint(1);
  const P2 = buttonPoint(2);
  const P3 = buttonPoint(3);
  const P4 = buttonPoint(4);
  const P6 = buttonPoint(6);
  const P8 = buttonPoint(8);

  /** 6 → 4 的直線 Slide，取樣點只為了畫箭頭帶。 */
  const SLIDE: PathSample[] = Array.from({ length: 11 }, (_, i) => {
    const u = i / 10;
    return { u, x: P6.x + (P4.x - P6.x) * u, y: P6.y + (P4.y - P6.y) * u };
  });
  const SLIDE_U = 0.3;
  const HANDOVER_U = 0.62;
  const SLIDE_PATH: SlidePath = { id: 'legend', shape: '-', startButton: 6, endButton: 4, samples: SLIDE, branches: [] };
  const ARROWS = slideTracks(SLIDE_PATH, SLIDE_U);
  /** 圖例裡的環帶都是平常的寬度（沒有判定中）。 */
  const BAND = bandWidth(false);
  const EDGE = edgeWidth(BAND);
  const at = (u: number): Point => ({ x: P6.x + (P4.x - P6.x) * u, y: P6.y + (P4.y - P6.y) * u });
  const HANDOVER_AT = px(at(HANDOVER_U));

  const B2: Point = { x: 0.4296, y: -0.1779 };
  const B3: Point = { x: 0.4296, y: 0.1779 };
  const C: Point = { x: 0, y: 0 };
  const PALM_CENTER: Point = { x: 0.47, y: 0 };
  const PALM_RADIUS = 0.27;

  const RIGHT_HAND: Point = { x: -0.05, y: -0.6 };
  const TRAIL: Point[] = [P1, { x: 0.22, y: -0.9 }, { x: 0.06, y: -0.8 }, { x: -0.02, y: -0.68 }, RIGHT_HAND];

  const tapAt = (p: Point) => px(p);
  const C_PX = px(C);
  /** Touch 畫成正在往落點收攏的樣子；Touch Hold 已經收到位、按住了 35%（外框亮起三分之一多一點）。 */
  const TOUCH_GATHER = 0.6;
  const TOUCH_HOLD_PROGRESS = 0.35;
  const HOLD_BAND = touchHoldBand(touchRadius('C'));
  const L_PX = px(P8);
  const R_PX = px(RIGHT_HAND);
  const handR = len(0.075);
</script>

<div class="legend">
  <div class="stage-box">
    <div class="mm-frame">
      <svg class="mm-disc legend-svg" class:has-active={active !== null} viewBox={VIEW_BOX} role="img" aria-label="盤面圖例：各種音符、手、軌跡、標籤、換手菱形與手掌虛線圓">
        <DiscBoard center={CENTER} radius={DISC_RADIUS} />

        <!-- EX 的光暈：墊在音符底下的那一筆過這個模糊濾鏡（同 DiscStage）。 -->
        <filter id={exGlowId} x="-60%" y="-60%" width="220%" height="220%" color-interpolation-filters="sRGB">
          <feGaussianBlur stdDeviation={EX_GLOW_BLUR_PX} />
        </filter>

        <g class="buttons" aria-hidden="true">
          {#each BUTTONS as button (button)}
            {@const outer = px(buttonPoint(button))}
            {@const inner = px(scalePoint(buttonPoint(button), 0.86))}
            <circle cx={outer.x} cy={outer.y} r={len(0.045)} />
            <text x={inner.x} y={inner.y} dominant-baseline="central" text-anchor="middle">{button}</text>
          {/each}
        </g>

        <!-- 點盤面上的圖形會選到清單的對應項目；鍵盤用下方清單。 -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <g class="items" onclick={(event) => {
          const id = (event.target as Element).closest('[data-id]')?.getAttribute('data-id') as ItemId | null;
          if (id) pick(id);
        }}>
          <!-- 手掌虛線圓（畫在最下層，同 DiscStage） -->
          <g class="item palm is-right" class:is-dim={dim('palm')} class:is-on={active === 'palm'} data-id="palm">
            <circle class="palm-edge" cx={px(PALM_CENTER).x} cy={px(PALM_CENTER).y} r={len(PALM_RADIUS)} />
            <path class="palm-center" d={palmCrossPath(px(PALM_CENTER))} />
            <circle class="palm-cover" cx={px(B2).x} cy={px(B2).y} r={len(0.165)} />
            <circle class="palm-cover" cx={px(B3).x} cy={px(B3).y} r={len(0.165)} />
            <circle class="hit" cx={px(PALM_CENTER).x} cy={px(PALM_CENTER).y} r={len(PALM_RADIUS)} />
          </g>

          <!-- Slide：箭頭帶（星星走過的不畫）、起點星星與移動中的星星 -->
          <g class="item" class:is-dim={dim('slide')} class:is-on={active === 'slide'} data-id="slide">
            <g class="slide-path">
              {#each ARROWS as track, index (index)}
                {#each track.strip.pieces as piece, k (k)}
                  {#if k >= track.first}
                    <path class="slide-under" d={track.strip.unders[k]} />
                    <path class="slide-chevrons" d={piece} />
                  {/if}
                {/each}
              {/each}
            </g>
            <g class="note">
              <path class="note-star is-moving" d={starOnPath(SLIDE, SLIDE_U)} />
              <circle class="note-dot is-moving" cx={px(at(SLIDE_U)).x} cy={px(at(SLIDE_U)).y} r={len(STAR_DOT)} />
            </g>
            <path class="hit" d={linePath(SLIDE)} />
          </g>

          <!-- Hold -->
          <g class="item note" class:is-dim={dim('hold')} class:is-on={active === 'hold'} data-id="hold">
            <!-- 頭的帽子中心在鍵位上；尾在 0.7 倍的位置，尾端中心有一顆小點 -->
            <path class="note-edge" d={holdPath(P8, 1, 0.7)} stroke-width={EDGE} />
            <path class="hold-bar" d={holdPath(P8, 1, 0.7)} stroke-width={BAND} />
            <circle class="note-dot" cx={px(scalePoint(P8, 0.7)).x} cy={px(scalePoint(P8, 0.7)).y} r={len(NOTE_DOT)} />
          </g>

          <!-- Tap、Break、EX -->
          <g class="item note" class:is-dim={dim('tap')} class:is-on={active === 'tap'} data-id="tap">
            <circle class="note-edge" cx={tapAt(P1).x} cy={tapAt(P1).y} r={len(NOTE_RADIUS)} stroke-width={EDGE} />
            <circle class="note-ring" cx={tapAt(P1).x} cy={tapAt(P1).y} r={len(NOTE_RADIUS)} stroke-width={BAND} />
            <circle class="note-dot" cx={tapAt(P1).x} cy={tapAt(P1).y} r={len(NOTE_DOT)} />
          </g>
          <g class="item note is-break" class:is-dim={dim('break')} class:is-on={active === 'break'} data-id="break">
            <circle class="note-edge" cx={tapAt(P2).x} cy={tapAt(P2).y} r={len(NOTE_RADIUS)} stroke-width={EDGE} />
            <circle class="note-ring" cx={tapAt(P2).x} cy={tapAt(P2).y} r={len(NOTE_RADIUS)} stroke-width={BAND} />
            <circle class="note-dot" cx={tapAt(P2).x} cy={tapAt(P2).y} r={len(NOTE_DOT)} />
          </g>
          <g class="item note is-ex" class:is-dim={dim('ex')} class:is-on={active === 'ex'} data-id="ex">
            <circle
              class="note-ex"
              cx={tapAt(P3).x}
              cy={tapAt(P3).y}
              r={len(NOTE_RADIUS) + exGlowOffset(BAND)}
              stroke-width={exGlowWidth(BAND)}
              filter={`url(#${exGlowId})`}
            />
            <circle class="note-edge" cx={tapAt(P3).x} cy={tapAt(P3).y} r={len(NOTE_RADIUS)} stroke-width={EDGE} />
            <circle class="note-ring" cx={tapAt(P3).x} cy={tapAt(P3).y} r={len(NOTE_RADIUS)} stroke-width={BAND} />
            <circle class="note-dot" cx={tapAt(P3).x} cy={tapAt(P3).y} r={len(NOTE_DOT)} />
          </g>

          <!-- Touch（被手掌蓋住的兩顆）與 Touch Hold -->
          <g class="item note" class:is-dim={dim('touch')} class:is-on={active === 'touch'} data-id="touch">
            {#each [{ p: B2, name: 'B2' }, { p: B3, name: 'B3' }] as touch (touch.name)}
              {#each touchFans(touchRadius('B'), TOUCH_GATHER, false) as fan, index (index)}
                <path class="touch-fan" d={fanBody(touch.p, fan)} />
              {/each}
              <circle class="touch-dot-rim" cx={px(touch.p).x} cy={px(touch.p).y} r={TOUCH_DOT_RIM_RADIUS} />
              <circle class="touch-dot" cx={px(touch.p).x} cy={px(touch.p).y} r={TOUCH_DOT_RADIUS} />
            {/each}
          </g>
          <g class="item note" class:is-dim={dim('touchHold')} class:is-on={active === 'touchHold'} data-id="touchHold">
            <!-- 已經按住：粗的圓角菱形外框從正上方順時針亮到 35%（按下去之前沒有外框） -->
            {#each touchHoldQuarters(C, touchRadius('C')) as quarter, index (index)}
              {@const fill = borderFill(TOUCH_HOLD_PROGRESS, index)}
              {#if fill > 0}
                <path
                  class="touch-hold-edge"
                  d={quarter}
                  pathLength="1"
                  stroke-dasharray={`${fill} 1`}
                  stroke-width={HOLD_BAND.edge}
                />
                <path
                  class="touch-hold-fill"
                  d={quarter}
                  pathLength="1"
                  stroke-dasharray={`${fill} 1`}
                  stroke-width={HOLD_BAND.fill}
                />
              {/if}
            {/each}
            {#each touchFans(touchRadius('C'), 1, true) as fan, index (index)}
              <path class="touch-fan" d={fanBody(C, fan)} />
            {/each}
            <circle class="touch-dot-rim" cx={C_PX.x} cy={C_PX.y} r={TOUCH_DOT_RIM_RADIUS} />
            <circle class="touch-dot" cx={C_PX.x} cy={C_PX.y} r={TOUCH_DOT_RADIUS} />
          </g>

          <!-- 音符旁的標籤 -->
          <g class="item" class:is-dim={dim('tags')} class:is-on={active === 'tags'} data-id="tags">
            {#each [{ p: { x: 0.3, y: -0.7 }, text: 'R', cls: 'is-right' }, { p: { x: -0.6, y: -0.6 }, text: 'L', cls: 'is-left' }, { p: { x: -0.74, y: 0.27 }, text: 'L→R', cls: 'is-handover' }] as tag (tag.text)}
              {@const a = px(tag.p)}
              <text class={['note-tag', tag.cls]} x={a.x} y={a.y} text-anchor="middle" dominant-baseline="central">{tag.text}</text>
            {/each}
          </g>

          <!-- 最近軌跡 -->
          <g class="item" class:is-dim={dim('trail')} class:is-on={active === 'trail'} data-id="trail">
            <path class="trail trail--right" d={linePath(TRAIL)} />
            <path class="hit" d={linePath(TRAIL)} />
          </g>

          <!-- 換手菱形 -->
          <g class="item handover is-active" class:is-dim={dim('handover')} class:is-on={active === 'handover'} data-id="handover">
            <path class="handover-diamond" d={diamondPath()} transform={`translate(${HANDOVER_AT.x} ${HANDOVER_AT.y})`} />
            <text class="handover-label" x={HANDOVER_AT.x} y={HANDOVER_AT.y - len(0.11)} text-anchor="middle">換手 L→R</text>
          </g>

          <!-- 手：L 按住 Hold（接觸中）、R 抬手移動（沒有接觸） -->
          <g class="item" class:is-dim={dim('left')} class:is-on={active === 'left'} data-id="left">
            <g class="hand hand--left is-contact">
              <circle cx={L_PX.x} cy={L_PX.y} r={handR} />
              <circle class="contact-ring" cx={L_PX.x} cy={L_PX.y} r={len(0.115)} />
              <text x={L_PX.x} y={L_PX.y} text-anchor="middle" dominant-baseline="central">L</text>
              <text class="hand-mode" x={L_PX.x} y={L_PX.y + len(0.17)} text-anchor="middle">按住</text>
            </g>
          </g>
          <g class="item" class:is-dim={dim('right')} class:is-on={active === 'right'} data-id="right">
            <g class="hand hand--right">
              <circle cx={R_PX.x} cy={R_PX.y} r={handR} />
              <text x={R_PX.x} y={R_PX.y} text-anchor="middle" dominant-baseline="central">R</text>
              <text class="hand-mode" x={R_PX.x} y={R_PX.y + len(0.17)} text-anchor="middle">移動中</text>
            </g>
          </g>
        </g>
      </svg>
    </div>
  </div>

  <ul class="legend-list" aria-label="盤面圖例">
    {#each ITEMS as item (item.id)}
      <li>
        <button
          type="button"
          class="legend-item"
          aria-pressed={pinned === item.id}
          class:is-on={active === item.id}
          onclick={() => pick(item.id)}
          onpointerenter={(event) => {
            if (event.pointerType === 'mouse') hovered = item.id;
          }}
          onpointerleave={() => (hovered = null)}
          onfocus={() => (hovered = item.id)}
          onblur={() => (hovered = null)}
        >
          <span class="legend-name">{item.name}</span>
          <span class="legend-text">{item.text}</span>
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .legend {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    width: 100%;
    max-width: 480px;
    margin: 0 auto;
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
  }

  .stage-box {
    position: relative;
    width: 100%;
    aspect-ratio: 1;
  }

  .items {
    cursor: pointer;
  }

  /* 選取時其餘圖形變暗；只動 opacity（盤面筆觸允許的例外），選中的保持原樣。 */
  .item {
    transition: opacity 120ms ease;
  }

  .item.is-dim {
    opacity: 0.18;
  }

  /* 透明的點擊範圍，讓細線也點得到。 */
  .hit {
    fill: none;
    stroke: transparent;
    stroke-width: 40;
    pointer-events: stroke;
  }

  circle.hit {
    fill: transparent;
    pointer-events: all;
  }

  .legend-list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 2px var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .legend-list li {
    margin: 0;
  }

  /* 清單列：同 App 的清單列（圓角 16、透明底；hover／選中出底色）。 */
  .legend-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    min-height: 44px;
    padding: 8px 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    background: transparent;
    color: var(--c-text);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      background-color 120ms ease,
      transform 90ms ease-out;
  }

  .legend-item:active {
    transform: scale(0.99);
  }

  .legend-item.is-on {
    background: var(--c-control);
  }

  .legend-item[aria-pressed='true'] {
    background: var(--c-control-hover);
    border-color: var(--c-border-strong);
  }

  .legend-item:focus-visible {
    outline-offset: -2px;
  }

  .legend-name {
    font-size: var(--fs-md);
    font-weight: 600;
    color: var(--c-text-strong);
  }

  .legend-text {
    font-size: var(--fs-sm);
    color: var(--c-text-dim);
  }

  @media (prefers-reduced-motion: reduce) {
    .item,
    .legend-item {
      transition: none;
    }
  }
</style>
