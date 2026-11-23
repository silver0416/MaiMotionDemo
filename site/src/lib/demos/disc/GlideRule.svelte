<script lang="ts">
  /**
   * 示意：滑移還是抬手。選兩顆鍵的關係與間隔，小盤面上同一隻手照判斷結果示範
   * 「貼著滑過去」或「抬手再按」。規則：距離在「相鄰連擊滑移距離」以內，
   * 而且間隔在「同手連打判定間隔」以內才滑移；同一鍵一律抬手重按。
   * 動畫放慢播放，時間不是實際秒數；數字對照 App 的預設參數。
   */
  import './disc.css';
  import DiscBoard from '$app/components/DiscBoard.svelte';
  import Icon from '$app/components/Icon.svelte';
  import NumberField from '$app/components/NumberField.svelte';
  import { BUTTONS, buttonPoint } from '$app/lib/disc';
  import { scalePoint } from '$app/lib/notes';
  import { DEFAULT_BASE, MODE_LABEL } from '$app/lib/contract';
  import { reducedMotion } from '$app/lib/press';
  import { segmented } from '$app/lib/segmented';
  import type { MotionMode, Point } from '$app/lib/types';
  import { CENTER, DISC_RADIUS, NOTE_DOT, NOTE_RADIUS, bandWidth, edgeWidth, len, linePath, px } from './draw';

  const GLIDE_DISTANCE = DEFAULT_BASE.glideDistance;
  const REPEAT_SECONDS = DEFAULT_BASE.repetitionSeconds;

  type Relation = 'same' | 'adjacent' | 'skip';
  const RELATIONS: { id: Relation; label: string; target: number }[] = [
    { id: 'same', label: '同一鍵', target: 1 },
    { id: 'adjacent', label: '相鄰', target: 2 },
    { id: 'skip', label: '隔一鍵', target: 3 },
  ];

  const uid = $props.id();
  let relation = $state<Relation>('adjacent');
  let interval = $state(0.1);

  const A = buttonPoint(1);
  const B = $derived(buttonPoint(RELATIONS.find((item) => item.id === relation)!.target));
  /** 兩次接觸的距離（盤面半徑為 1）：相鄰約 0.77、隔一鍵約 1.41、同一鍵 0。 */
  const distance = $derived(Math.hypot(B.x - A.x, B.y - A.y));

  const nearEnough = $derived(relation !== 'same' && distance <= GLIDE_DISTANCE + 1e-9);
  const fastEnough = $derived(interval <= REPEAT_SECONDS + 1e-9);
  const glide = $derived(nearEnough && fastEnough);

  function setInterval(value: number) {
    interval = Math.min(0.4, Math.max(0.03, Math.round(value * 100) / 100));
  }

  // ---- 慢動作循環：A 敲擊 → 移到 B → B 敲擊 → 回到 A ----

  const CYCLE = 2.4;
  /** 減少動態時停在這一格：正在移動的途中，看得出是滑移還是抬手。 */
  const STILL_PHASE = 0.85;

  let phase = $state(0);
  let playing = $state(false);
  let userPaused = $state(false);
  let inView = $state(false);
  let pageVisible = $state(true);

  interface Pose {
    point: Point;
    mode: MotionMode;
    contact: boolean;
  }

  function lerp(a: Point, b: Point, k: number): Point {
    return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
  }

  /** 手在循環第 t 秒的位置與狀態。 */
  function pose(t: number, from: Point, to: Point, glides: boolean): Pose {
    const s = ((t % CYCLE) + CYCLE) % CYCLE;
    const tap = (point: Point): Pose => ({ point, mode: 'tap', contact: true });
    const move = (a: Point, b: Point, k: number): Pose => {
      const eased = k < 0.5 ? 2 * k * k : 1 - 2 * (1 - k) * (1 - k);
      if (glides) return { point: lerp(a, b, eased), mode: 'glide', contact: true };
      // 抬手：往盤面中心稍微離開再落下，同一鍵也看得出「抬起來重按」。
      const lift = Math.sin(Math.PI * k) * 0.12;
      const p = lerp(a, b, eased);
      return { point: scalePoint(p, 1 - lift), mode: 'travel', contact: false };
    };
    if (s < 0.5) return tap(from);
    if (s < 1.2) return move(from, to, (s - 0.5) / 0.7);
    if (s < 1.7) return tap(to);
    return move(to, from, (s - 1.7) / 0.7);
  }

  const hand = $derived(pose(phase, A, B, glide));

  /** 最近軌跡：往回取 0.6 秒的位置。 */
  const trail = $derived.by(() => {
    const points: Point[] = [];
    for (let i = 12; i >= 0; i -= 1) points.push(pose(phase - (i / 12) * 0.6, A, B, glide).point);
    return linePath(points);
  });

  $effect(() => {
    if (reducedMotion()) phase = STILL_PHASE;
  });

  $effect(() => {
    const auto = inView && pageVisible && !userPaused && !reducedMotion();
    if (auto) playing = true;
    else if (!inView || !pageVisible) playing = false;
  });

  $effect(() => {
    if (!playing) return;
    let last = 0;
    let frame = requestAnimationFrame(function step(now) {
      const dt = last ? Math.min((now - last) / 1000, 0.1) : 0;
      last = now;
      phase = (phase + dt) % CYCLE;
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  });

  function toggle() {
    playing = !playing;
    userPaused = !playing;
  }

  function replay() {
    phase = 0;
    playing = true;
    userPaused = false;
  }

  let root = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!root || typeof IntersectionObserver === 'undefined') {
      inView = true;
      return;
    }
    const observer = new IntersectionObserver(
      (records) => {
        for (const record of records) inView = record.isIntersecting;
      },
      { threshold: 0.35 },
    );
    observer.observe(root);
    return () => observer.disconnect();
  });

  $effect(() => {
    const update = () => (pageVisible = document.visibilityState !== 'hidden');
    update();
    document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  });

  // 只看盤面右上角（1、2、3 號鍵附近），字比整個盤面大一些。
  const VIEW = (() => {
    const x0 = CENTER.x + -0.42 * DISC_RADIUS;
    const y0 = CENTER.y + -1.2 * DISC_RADIUS;
    const size = 1.78 * DISC_RADIUS;
    return `${x0} ${y0} ${size} ${size}`;
  })();

  const handR = len(0.075);
  const handPx = $derived(px(hand.point));
</script>

<div class="glide" bind:this={root}>
  <div class="glide-body">
    <div class="stage-box">
      <!-- 點盤面切換播放／暫停；鍵盤用下方按鈕。 -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="mm-frame stage-hit" onclick={toggle}>
        <svg
          class="mm-disc"
          viewBox={VIEW}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={`右手從 1 號鍵到 ${RELATIONS.find((item) => item.id === relation)?.target} 號鍵：${glide ? '滑移' : '抬手再按'}`}
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
          <g class="notes" aria-hidden="true">
            {#each relation === 'same' ? [A] : [A, B] as point, index (index)}
              {@const p = px(point)}
              {@const band = bandWidth(hand.contact && hand.mode === 'tap' && Math.hypot(hand.point.x - point.x, hand.point.y - point.y) < 1e-6)}
              <g class="note">
                <circle class="note-edge" cx={p.x} cy={p.y} r={len(NOTE_RADIUS)} stroke-width={edgeWidth(band)} />
                <circle class="note-ring" cx={p.x} cy={p.y} r={len(NOTE_RADIUS)} stroke-width={band} />
                <circle class="note-dot" cx={p.x} cy={p.y} r={len(NOTE_DOT)} />
              </g>
            {/each}
          </g>
          <g class="tracks" aria-hidden="true">
            <path class="trail trail--right" d={trail} />
          </g>
          <g class="hands" aria-hidden="true">
            <g class="hand hand--right" class:is-contact={hand.contact}>
              <circle cx={handPx.x} cy={handPx.y} r={handR} />
              {#if hand.contact}
                <circle class="contact-ring" cx={handPx.x} cy={handPx.y} r={len(0.115)} />
              {/if}
              <text x={handPx.x} y={handPx.y} text-anchor="middle" dominant-baseline="central">R</text>
              <text class="hand-mode" x={handPx.x} y={handPx.y + len(0.17)} text-anchor="middle">
                {MODE_LABEL[hand.mode]}
              </text>
            </g>
          </g>
        </svg>
      </div>
      <div class="player">
        <button
          type="button"
          class="btn btn--round play-btn"
          class:is-playing={playing}
          onclick={toggle}
          aria-label={playing ? '暫停' : '播放'}
          title={playing ? '暫停' : '播放'}
        >
          <span class="morph" aria-hidden="true"><span class="morph-a"></span><span class="morph-b"></span></span>
        </button>
        <button type="button" class="btn btn--round" onclick={replay} aria-label="重播" title="重播">
          <Icon name="rotate-ccw" />
        </button>
        <span class="slow">放慢示範</span>
      </div>
    </div>

    <div class="glide-controls">
      <div class="field">
        <span class="field-label" id={`${uid}-relation`}>兩顆鍵的關係</span>
        <div class="seg relation" role="group" aria-labelledby={`${uid}-relation`} use:segmented>
          {#each RELATIONS as item (item.id)}
            <button type="button" class="seg-btn" aria-pressed={relation === item.id} onclick={() => (relation = item.id)}>
              {item.label}
            </button>
          {/each}
        </div>
      </div>
      <NumberField label="兩次接觸的間隔" value={interval} min={0.03} max={0.4} step={0.01} unit="秒" onValue={setInterval} />

      <dl class="verdict" aria-live="polite">
        <dt>距離</dt>
        <dd>
          {#if relation === 'same'}
            同一鍵，距離 <span class="mono">0</span>：同一鍵一律抬手重按
          {:else}
            約 <span class="mono">{distance.toFixed(2)}</span>，{nearEnough ? '在' : '超過'}「相鄰連擊滑移距離」<span class="mono"
              >{GLIDE_DISTANCE}</span
            >{nearEnough ? ' 以內' : ''}
            <span class={['state', nearEnough ? 'is-yes' : 'is-no']}>{nearEnough ? '成立' : '不成立'}</span>
          {/if}
        </dd>
        <dt>間隔</dt>
        <dd>
          <span class="mono">{interval.toFixed(2)}</span> 秒，{fastEnough ? '在' : '超過'}「同手連打判定間隔」<span class="mono"
            >{REPEAT_SECONDS}</span
          > 秒{fastEnough ? '以內' : ''}
          <span class={['state', fastEnough ? 'is-yes' : 'is-no']}>{fastEnough ? '成立' : '不成立'}</span>
        </dd>
        <dt>結果</dt>
        <dd class="conclusion">
          {#if glide}
            滑移：手不抬起，貼著面板滑過去
          {:else if relation === 'same'}
            抬手：抬起來，在同一鍵重按
          {:else}
            抬手：抬起來再按下一顆
          {/if}
        </dd>
      </dl>
    </div>
  </div>
</div>

<style>
  .glide {
    container-type: inline-size;
    width: 100%;
    max-width: 480px;
    margin: 0 auto;
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
  }

  .glide-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .stage-box {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .stage-hit {
    position: relative;
    inset: auto;
    width: min(100%, 300px);
    aspect-ratio: 1;
    margin: 0 auto;
    cursor: pointer;
  }

  .player {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
  }

  .slow {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  .glide-controls {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .relation {
    width: 100%;
  }

  .verdict {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-2) var(--space-3);
    margin: 0;
  }

  .verdict dt {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    padding-top: 1px;
  }

  .verdict dd {
    margin: 0;
    color: var(--c-text);
  }

  .state {
    margin-left: var(--space-1);
    white-space: nowrap;
    font-weight: 700;
  }

  .state.is-yes {
    color: var(--c-text-strong);
  }

  .state.is-no {
    color: var(--c-text-dim);
  }

  .conclusion {
    font-weight: 700;
    color: var(--c-text-strong);
  }

  .morph {
    position: relative;
    flex: none;
    width: 12px;
    height: 12px;
  }

  .morph-a,
  .morph-b {
    position: absolute;
    inset: 0;
    background: currentColor;
    transition: clip-path 220ms var(--ease-out);
  }

  .morph-a {
    clip-path: polygon(8% 0%, 54% 25%, 54% 75%, 8% 100%);
  }

  .morph-b {
    clip-path: polygon(54% 25%, 100% 50%, 100% 50%, 54% 75%);
  }

  .play-btn.is-playing .morph-a {
    clip-path: polygon(8% 0%, 40% 0%, 40% 100%, 8% 100%);
  }

  .play-btn.is-playing .morph-b {
    clip-path: polygon(64% 0%, 96% 0%, 96% 100%, 64% 100%);
  }

  @media (prefers-reduced-motion: reduce) {
    .morph-a,
    .morph-b {
      transition: none;
    }
  }
</style>
