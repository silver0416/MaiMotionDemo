<script lang="ts">
  /**
   * 會在盤面上走動的吉祥物：站上的標誌（環＝盤面，粉、藍兩點＝左右手），兩個點一起移到盤面上的別處再回來。
   *
   * 規則：
   * - 兩個點的距離永遠不變：兩點是同一組，整組只做平移＋旋轉，移動途中距離也一樣。
   * - 不交叉：旋轉角度只在 −90°～90° 之間（藍點不會跑到粉點左邊），角度在這個範圍內內插，途中也不會交叉。
   * - 目的地隨機：靠近 1～8 號按鈕（兩點一起靠到圈邊、或其中一點靠向某顆按鈕），或盤面內任何地方。
   * - 兩點只靠近圈、不壓到圈上：圓心離盤面中心最遠到 NEAR（點的外緣和圈的內緣之間留一點空隙）。
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
  /**
   * 點的圓心離盤面中心最遠的距離：圈的內緣在 38 − 10.5 / 2 = 32.75，點的半徑 10，
   * 所以 22.75 會剛好貼到圈；取 22，留一點空隙（靠近、不碰到）。
   */
  const NEAR = 22;
  /** 移動途中的上限：22.75 剛好貼到圈，途中最多到幾乎貼上。 */
  const LIMIT = 22.6;
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

  /** 第 k 顆按鈕（0＝1 號，順時針）的方向上、離圓心 r 的位置。 */
  function button(k: number, r: number): [number, number] {
    const angle = STEP + 2 * STEP * k;
    return [r * Math.sin(angle), -r * Math.cos(angle)];
  }

  /**
   * 從 from 移到 to 的途中，兩點會不會壓到圈：平移和旋轉用同一條時間曲線，途中的姿勢只看進度，
   * 所以沿進度取樣，檢查兩點的圓心都還在 LIMIT 以內。
   */
  function clear(from: Pose, to: Pose): boolean {
    for (let i = 1; i < 24; i++) {
      const p = i / 24;
      const x = from.x + (to.x - from.x) * p;
      const y = from.y + (to.y - from.y) * p;
      const rad = ((from.deg + (to.deg - from.deg) * p) * Math.PI) / 180;
      const dx = HALF * Math.cos(rad);
      const dy = HALF * Math.sin(rad);
      if (Math.hypot(x - dx, y - dy) > LIMIT || Math.hypot(x + dx, y + dy) > LIMIT) return false;
    }
    return true;
  }

  function pick(prev: Pose): Pose {
    for (let attempt = 0; attempt < 200; attempt++) {
      let next: Pose;
      if (Math.random() < 0.35) {
        // 兩點一起靠到圈邊：以某顆按鈕、或兩顆按鈕中間的方向為中線，兩點在它兩側、都貼近圈。
        const mid = STEP * Math.floor(Math.random() * 16);
        const spread = Math.asin(HALF / NEAR);
        const at = (angle: number): [number, number] => [NEAR * Math.sin(angle), -NEAR * Math.cos(angle)];
        const a = at(mid - spread);
        const b = at(mid + spread);
        const [left, right] = a[0] <= b[0] ? [a, b] : [b, a];
        next = {
          x: (left[0] + right[0]) / 2,
          y: (left[1] + right[1]) / 2,
          deg: (Math.atan2(right[1] - left[1], right[0] - left[0]) * 180) / Math.PI,
        };
      } else {
        // 其中一點靠向某顆按鈕（貼近圈）、或在盤面內任意一點；另一點在固定距離外、不交叉的方向上。
        const deg = rand(-90, 90);
        const ux = Math.cos((deg * Math.PI) / 180);
        const uy = Math.sin((deg * Math.PI) / 180);
        let anchor: [number, number];
        if (Math.random() < 0.6) {
          anchor = button(Math.floor(Math.random() * 8), NEAR);
        } else {
          const r = NEAR * Math.sqrt(Math.random());
          const angle = rand(0, 2 * Math.PI);
          anchor = [r * Math.cos(angle), r * Math.sin(angle)];
        }
        // side 1：落點是粉點（藍點在它的 +方向）；−1：落點是藍點。
        const side = Math.random() < 0.5 ? 1 : -1;
        const other: [number, number] = [anchor[0] + side * 2 * HALF * ux, anchor[1] + side * 2 * HALF * uy];
        if (Math.hypot(other[0], other[1]) > NEAR) continue;
        next = { x: anchor[0] + side * HALF * ux, y: anchor[1] + side * HALF * uy, deg };
      }
      // 太近的不算一次移動。
      if (Math.hypot(next.x - prev.x, next.y - prev.y) < 8) continue;
      // 過去的路上、以及之後回原位的路上都不能壓到圈（每個姿勢都能直接回家，找不到下一步時就回原位）。
      if (!clear(prev, next) || !clear(next, HOME)) continue;
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
