<script lang="ts">
  /**
   * 會在盤面上走動的吉祥物：站上的標誌（環＝盤面，粉、藍兩點＝左右手），兩個點一起移到盤面上的別處再回來。
   *
   * 規則：
   * - 兩個點的距離永遠不變：兩點是同一組，整組只做平移＋旋轉，移動途中距離也一樣。
   * - 不交叉：旋轉角度只在 −90°～90° 之間（藍點不會跑到粉點左邊），角度在這個範圍內內插，途中也不會交叉。
   * - 目的地隨機：1～8 號按鈕的位置（相鄰兩顆、或其中一點落在某顆按鈕上），或盤面內任何地方。
   * - 走四次之後回到原位，停住。每次移動的時間隨機，最長 1.5 秒。
   * - 不循環：第一次捲進畫面時走一輪；滑鼠移上去或點一下再走一輪。
   * - prefers-reduced-motion：完全不動，就是一個靜止的標誌。
   *
   * 純裝飾（aria-hidden）。座標用 Logo.svelte 的 viewBox（0～100，圓心 50,50）。
   */
  import { onMount } from 'svelte';

  interface Props {
    size?: number;
  }

  let { size = 48 }: Props = $props();

  /** 兩點各離中點多遠（Logo 的兩點在 36 與 64，中點 50）。 */
  const HALF = 14;
  /** 盤面半徑：按鈕在環的中線上，點的圓心最遠到這裡。 */
  const RADIUS = 38;
  /** 相鄰兩顆按鈕的圓心角一半（22.5°）。 */
  const STEP = Math.PI / 8;
  const MOVES = 4;
  const MAX_MS = 1500;
  const MIN_MS = 450;

  /** 兩點中點相對圓心的位移，以及粉點指向藍點的方向（度，−90～90）。 */
  interface Pose {
    x: number;
    y: number;
    deg: number;
  }

  const HOME: Pose = { x: 0, y: 0, deg: 0 };

  let pose = $state<Pose>(HOME);
  let ms = $state(0);
  let root: SVGSVGElement;
  let running = false;
  let timer = 0;

  const rand = (min: number, max: number) => min + Math.random() * (max - min);

  /** 第 k 顆按鈕（0＝1 號，順時針）的位置，r 是離圓心的距離。 */
  function button(k: number, r: number): [number, number] {
    const angle = STEP + 2 * STEP * k;
    return [r * Math.sin(angle), -r * Math.cos(angle)];
  }

  function pick(prev: Pose): Pose {
    for (let attempt = 0; attempt < 60; attempt++) {
      let next: Pose;
      if (Math.random() < 0.35) {
        // 相鄰兩顆按鈕：兩點距離固定，所以稍微往內一點（弦長剛好等於兩點距離的那個半徑）。
        const k = Math.floor(Math.random() * 8);
        const r = HALF / Math.sin(STEP);
        const a = button(k, r);
        const b = button((k + 1) % 8, r);
        const [left, right] = a[0] <= b[0] ? [a, b] : [b, a];
        next = {
          x: (left[0] + right[0]) / 2,
          y: (left[1] + right[1]) / 2,
          deg: (Math.atan2(right[1] - left[1], right[0] - left[0]) * 180) / Math.PI,
        };
      } else {
        // 其中一點落在某顆按鈕、或盤面內任意一點；另一點在固定距離外、不交叉的方向上。
        const deg = rand(-90, 90);
        const ux = Math.cos((deg * Math.PI) / 180);
        const uy = Math.sin((deg * Math.PI) / 180);
        let anchor: [number, number];
        if (Math.random() < 0.6) {
          anchor = button(Math.floor(Math.random() * 8), RADIUS);
        } else {
          const r = RADIUS * Math.sqrt(Math.random());
          const angle = rand(0, 2 * Math.PI);
          anchor = [r * Math.cos(angle), r * Math.sin(angle)];
        }
        // side 1：落點是粉點（藍點在它的 +方向）；−1：落點是藍點。
        const side = Math.random() < 0.5 ? 1 : -1;
        const other: [number, number] = [anchor[0] + side * 2 * HALF * ux, anchor[1] + side * 2 * HALF * uy];
        if (Math.hypot(other[0], other[1]) > RADIUS) continue;
        next = { x: anchor[0] + side * HALF * ux, y: anchor[1] + side * HALF * uy, deg };
      }
      // 太近的不算一次移動。
      if (Math.hypot(next.x - prev.x, next.y - prev.y) < 12) continue;
      return next;
    }
    return HOME;
  }

  function run() {
    if (running || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    running = true;
    let step = 0;
    const move = () => {
      const last = step === MOVES;
      ms = Math.round(rand(MIN_MS, MAX_MS));
      pose = last ? HOME : pick(pose);
      step += 1;
      timer = window.setTimeout(
        () => {
          if (last) running = false;
          else move();
        },
        ms + (last ? 0 : rand(120, 380)),
      );
    };
    move();
  }

  onMount(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches || !('IntersectionObserver' in window)) return;
    let start = 0;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        start = window.setTimeout(run, 700);
      },
      { threshold: 0.6 },
    );
    observer.observe(root);
    return () => {
      observer.disconnect();
      window.clearTimeout(start);
      window.clearTimeout(timer);
    };
  });
</script>

<!-- 純裝飾：滑鼠移上去或點一下只是再走一輪，沒有任何功能，所以不做成按鈕。 -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<svg
  class="roam"
  width={size}
  height={size}
  viewBox="0 0 100 100"
  aria-hidden="true"
  bind:this={root}
  onpointerenter={run}
  onclick={run}
>
  <circle class="roam-ring" cx="50" cy="50" r="38" />
  <g
    class="roam-hands"
    style:transform={`translate(${pose.x.toFixed(2)}px, ${pose.y.toFixed(2)}px) rotate(${pose.deg.toFixed(2)}deg)`}
    style:transition-duration={`${ms}ms`}
  >
    <circle class="roam-left" cx="36" cy="50" r="10" />
    <circle class="roam-right" cx="64" cy="50" r="10" />
  </g>
</svg>

<style>
  .roam {
    flex: none;
    display: block;
    overflow: visible;
    color: var(--c-text-strong);
  }

  .roam-ring {
    fill: none;
    stroke: currentColor;
    stroke-width: 10.5;
  }

  /* 兩點是一組：以圓心為軸平移＋旋轉。transform 的兩個函式分開內插，所以整組是剛體移動，兩點距離不變。 */
  .roam-hands {
    transform-origin: 50px 50px;
    transition-property: transform;
    transition-timing-function: var(--ease-out);
  }

  .roam-left {
    fill: var(--c-left);
  }

  .roam-right {
    fill: var(--c-right);
  }

  @media (prefers-reduced-motion: reduce) {
    .roam-hands {
      transition: none;
    }
  }
</style>
