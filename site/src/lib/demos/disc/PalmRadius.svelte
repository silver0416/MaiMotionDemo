<script lang="ts">
  /**
   * 示意：手掌半徑。33 個 Touch 落點（座標取核心輸出的 chart.touchSensors）、
   * 一個可拖曳（或用方向鍵移動）的掌心與「手掌半徑」滑桿，即時標出半徑內的落點。
   * 這裡只比距離，是幾何示意；實際能不能一掌蓋住由核心判定。
   */
  import './disc.css';
  import DiscBoard from '$app/components/DiscBoard.svelte';
  import NumberField from '$app/components/NumberField.svelte';
  import { BUTTONS, buttonPoint } from '$app/lib/disc';
  import { scalePoint } from '$app/lib/notes';
  import { radialAngle, sensorKey, touchName } from '$app/lib/touch';
  import { DEFAULT_BASE } from '$app/lib/contract';
  import type { Point, TouchSensor } from '$app/lib/types';
  import palmData from '../data/palm.json';
  import { CENTER, DISC_RADIUS, VIEW_BOX, len, palmCrossPath, px, sensorRadius, shapePath } from './draw';

  const SENSORS = palmData.chart.touchSensors as TouchSensor[];
  const DEFAULT_RADIUS = DEFAULT_BASE.palmRadius;
  const FALLBACK_RADIUS = DEFAULT_BASE.palmFallbackRadius;

  /** 起始掌心：和「手掌覆蓋」演示（C/B1/E1）核心算出的掌心相同。 */
  let center = $state<Point>({ x: 0, y: -0.3225 });
  let radius = $state(DEFAULT_RADIUS);

  const usingFallback = $derived(Math.abs(radius - FALLBACK_RADIUS) < 1e-9);

  function setRadius(value: number) {
    radius = Math.min(1, Math.max(0, Math.round(value * 100) / 100));
  }

  interface Mark {
    key: string;
    name: string;
    at: Point;
    outline: string;
    inside: boolean;
    distance: number;
  }

  const marks = $derived<Mark[]>(
    SENSORS.map((sensor) => {
      const area = String(sensor.area);
      const distance = Math.hypot(sensor.position.x - center.x, sensor.position.y - center.y);
      return {
        key: sensorKey(area, sensor.index),
        name: touchName(area, sensor.index),
        at: px(sensor.position),
        outline: shapePath(area, sensor.position, sensorRadius(area), radialAngle(sensor.position)),
        inside: radius > 0 && distance <= radius + 1e-9,
        distance,
      };
    }),
  );

  /** 半徑內的落點，由近到遠。 */
  const inside = $derived(marks.filter((mark) => mark.inside).sort((a, b) => a.distance - b.distance));

  // ---- 拖曳與鍵盤 ----

  let svg = $state<SVGSVGElement | null>(null);
  let dragging = $state<number | null>(null);

  /** 掌心限制在盤面內。 */
  function place(point: Point) {
    const r = Math.hypot(point.x, point.y);
    const limit = 1;
    const k = r > limit ? limit / r : 1;
    center = { x: Math.round(point.x * k * 1000) / 1000, y: Math.round(point.y * k * 1000) / 1000 };
  }

  function toDisc(event: MouseEvent): Point | null {
    if (!svg) return null;
    const matrix = svg.getScreenCTM();
    if (!matrix) return null;
    const p = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse());
    return { x: (p.x - CENTER.x) / DISC_RADIUS, y: (p.y - CENTER.y) / DISC_RADIUS };
  }

  /** 拖曳只從掌心開始，盤面其他地方在手機上仍可以捲動頁面；點一下盤面則把掌心移過去。 */
  function onDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
    dragging = event.pointerId;
    moved = false;
  }

  let moved = false;

  function onClick(event: MouseEvent) {
    if (moved) {
      moved = false;
      return;
    }
    const point = toDisc(event as PointerEvent);
    if (point) place(point);
  }

  function onMove(event: PointerEvent) {
    if (dragging !== event.pointerId) return;
    const point = toDisc(event);
    if (!point) return;
    moved = true;
    place(point);
  }

  function onUp(event: PointerEvent) {
    if (dragging === event.pointerId) dragging = null;
  }

  function onKey(event: KeyboardEvent) {
    const step = event.shiftKey ? 0.2 : 0.05;
    const moves: Record<string, Point> = {
      ArrowLeft: { x: -step, y: 0 },
      ArrowRight: { x: step, y: 0 },
      ArrowUp: { x: 0, y: -step },
      ArrowDown: { x: 0, y: step },
    };
    const move = moves[event.key];
    if (event.key === 'Home') {
      event.preventDefault();
      place({ x: 0, y: 0 });
      return;
    }
    if (!move) return;
    event.preventDefault();
    place({ x: center.x + move.x, y: center.y + move.y });
  }

  const centerPx = $derived(px(center));
  const handR = len(0.075);
</script>

<div class="palm-demo">
  <div class="stage-box">
    <div class="mm-frame">
      <!-- 點盤面把掌心移過去；鍵盤用掌心本身（方向鍵）。 -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <svg
        bind:this={svg}
        class="mm-disc palm-svg"
        class:is-dragging={dragging !== null}
        viewBox={VIEW_BOX}
        role="img"
        aria-label="盤面上的 33 個 Touch 落點與手掌範圍；拖曳掌心或點盤面移動掌心"
        onclick={onClick}
      >
        <DiscBoard center={CENTER} radius={DISC_RADIUS} />

        <g class="buttons" aria-hidden="true">
          {#each BUTTONS as button (button)}
            {@const outer = px(buttonPoint(button))}
            {@const inner = px(scalePoint(buttonPoint(button), 0.86))}
            <circle cx={outer.x} cy={outer.y} r={len(0.045)} />
            <text x={inner.x} y={inner.y} dominant-baseline="central" text-anchor="middle">{button}</text>
          {/each}
        </g>

        <!-- 33 個落點參考；半徑內的加重，外圍再加一圈手色虛線（同 App 的手掌覆蓋標示）。 -->
        <g class="sensors" aria-hidden="true">
          {#each marks as mark (mark.key)}
            <g class="sensor" class:is-used={mark.inside}>
              <path class="sensor-outline" d={mark.outline} />
              <text class="sensor-label" x={mark.at.x} y={mark.at.y} text-anchor="middle" dominant-baseline="central">
                {mark.name}
              </text>
            </g>
          {/each}
        </g>

        <g class="palm is-right" aria-hidden="true">
          {#if radius > 0}
            <circle class="palm-edge" cx={centerPx.x} cy={centerPx.y} r={len(radius)} />
          {/if}
          <path class="palm-center" d={palmCrossPath(centerPx)} />
          {#each inside as mark (mark.key)}
            <circle class="palm-cover" cx={mark.at.x} cy={mark.at.y} r={len(0.165)} />
          {/each}
        </g>

        <!-- 掌心：可聚焦，方向鍵移動（Shift 一次移多一點），Home 回到中央。 -->
        <g
          class="hand hand--right is-contact handle"
          role="button"
          tabindex="0"
          aria-label={`掌心位置 x ${center.x.toFixed(2)}、y ${center.y.toFixed(2)}；用方向鍵移動`}
          onkeydown={onKey}
          onpointerdown={onDown}
          onpointermove={onMove}
          onpointerup={onUp}
          onpointercancel={onUp}
        >
          <circle class="grab" cx={centerPx.x} cy={centerPx.y} r={len(0.2)} />
          <circle cx={centerPx.x} cy={centerPx.y} r={handR} />
          <text x={centerPx.x} y={centerPx.y} text-anchor="middle" dominant-baseline="central">R</text>
        </g>
      </svg>
    </div>
  </div>

  <div class="palm-controls">
    <NumberField
      label="手掌半徑"
      value={radius}
      min={0}
      max={1}
      step={0.01}
      onValue={setRadius}
    />
    <div class="row-line">
      <button
        type="button"
        class="btn"
        aria-pressed={usingFallback}
        onclick={() => setRadius(usingFallback ? DEFAULT_RADIUS : FALLBACK_RADIUS)}
      >
        備用手掌半徑 {FALLBACK_RADIUS}
      </button>
      <span class="area mono">面積約為盤面的 {Math.round(radius * radius * 100)}%</span>
    </div>
    <p class="result" aria-live="polite">
      <span class="count">半徑內有 <span class="mono">{inside.length}</span> 個落點</span>
      {#if inside.length > 0}
        <span class="names mono">{inside.map((mark) => mark.name).join('・')}</span>
      {/if}
    </p>
    <p class="note">
      這只是幾何示意，只比落點和掌心的距離。實際能不能一掌蓋住，還要看是不是同一判定時間、這隻手有沒有空，由核心判定。
    </p>
  </div>
</div>

<style>
  .palm-demo {
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

  .palm-svg {
    cursor: crosshair;
    user-select: none;
  }

  .palm-svg.is-dragging {
    cursor: grabbing;
  }

  .handle {
    cursor: grab;
    touch-action: none;
  }

  /* 掌心外圍一圈透明的抓取範圍，手指也抓得到。 */
  .mm-disc .handle circle.grab {
    fill: transparent;
    stroke: none;
  }

  .handle:focus {
    outline: none;
  }

  .handle:focus-visible circle {
    stroke: var(--c-focus);
  }

  .palm-controls {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .row-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
  }

  .area {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  .result {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    color: var(--c-text-strong);
  }

  .count {
    font-weight: 600;
  }

  .names {
    color: var(--c-text);
    overflow-wrap: anywhere;
  }

  .note {
    margin: 0;
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }
</style>
