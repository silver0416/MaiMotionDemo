<script lang="ts" module>
  import type { Point } from '../lib/types';

  /**
   * 盤面底圖的純向量版本，取代原本的 resource/maimai.png。
   * 形狀全部以盤面座標（圓心 0,0、外環半徑 1）定義，數值量自原圖（scratch 量測：圓心 500.5,485.5、半徑 480）：
   * - 內圈 16 塊磚：8 塊方磚（中心半徑 0.6455、半邊 0.1，一角朝圓心）與 8 塊七邊磚（中心半徑 0.458，位於各鍵位方向），8 方向對稱。
   * - 外區點線：每個鍵位方向 ±13.9° 各一條徑向點線（0.728 → 外環），鍵位下方一條內凹弧線（最低 0.667）。
   * - 外環與 8 個鍵位白點。
   * 圓心與半徑跟著校準走，所以底圖一定和 Rust 座標的音符對齊。
   */

  const DEG = Math.PI / 180;

  function rotate(point: Point, angle: number): Point {
    const c = Math.cos(angle);
    const s = Math.sin(angle);
    return { x: point.x * c - point.y * s, y: point.x * s + point.y * c };
  }

  function polar(radius: number, angle: number): Point {
    return { x: radius * Math.cos(angle), y: radius * Math.sin(angle) };
  }

  function closed(points: Point[]): string {
    return `${points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(4)} ${p.y.toFixed(4)}`).join('')}Z`;
  }

  /** 1 號鍵方向（-67.5°）那塊七邊磚，量自原圖；其餘七塊由旋轉與鏡射得到。 */
  const HEPTAGON: Point[] = [
    { x: 0.0198, y: -0.4718 },
    { x: 0.124, y: -0.576 },
    { x: 0.3198, y: -0.4948 },
    { x: 0.3177, y: -0.3427 },
    { x: 0.2635, y: -0.2906 },
    { x: 0.1135, y: -0.2969 },
    { x: 0.0177, y: -0.3969 },
  ];

  /** 沿 -45° 對角線鏡射：(x, y) → (-y, -x)。-67.5° 的七邊磚鏡射後就是 -22.5° 那塊。 */
  const mirrored = HEPTAGON.map((p) => ({ x: -p.y, y: -p.x }));

  const TILES: string[] = [];
  for (let quarter = 0; quarter < 4; quarter += 1) {
    const turn = quarter * 90 * DEG;
    TILES.push(closed(HEPTAGON.map((p) => rotate(p, turn))));
    TILES.push(closed(mirrored.map((p) => rotate(p, turn))));
  }
  // 方磚：位置角 θ，四角在 θ + 90°·j 方向（一角朝圓心）；對角方向的看起來是正方形，正上下左右的是菱形。
  const SQUARE_RADIUS = 0.6455;
  const SQUARE_CORNER = 0.1414;
  for (let k = 0; k < 8; k += 1) {
    const theta = (-90 + k * 45) * DEG;
    const center = polar(SQUARE_RADIUS, theta);
    const corners = [0, 1, 2, 3].map((j) => {
      const corner = polar(SQUARE_CORNER, theta + j * 90 * DEG);
      return { x: center.x + corner.x, y: center.y + corner.y };
    });
    TILES.push(closed(corners));
  }

  /** 第 k 鍵（1–8）的方向角，與 lib/disc.ts buttonPoint 相同。 */
  function buttonAngle(button: number): number {
    return (-90 + 22.5 + (button - 1) * 45) * DEG;
  }

  const SPOKE_OFFSET = 13.9 * DEG;
  const SPOKE_INNER = 0.728;
  const SPOKE_OUTER = 0.985;
  /** 鍵位下方弧線：偏離鍵位角度（度）→ 半徑，量自原圖，左右對稱。 */
  const ARC_PROFILE: [number, number][] = [
    [0, 0.667],
    [3, 0.67],
    [5.8, 0.675],
    [7.2, 0.677],
    [8.5, 0.685],
    [10, 0.696],
    [11.3, 0.706],
    [12.5, 0.717],
    [13.9, SPOKE_INNER],
  ];

  const LINES: string[] = [];
  for (let button = 1; button <= 8; button += 1) {
    const base = buttonAngle(button);
    for (const side of [-1, 1]) {
      const angle = base + side * SPOKE_OFFSET;
      const a = polar(SPOKE_INNER, angle);
      const b = polar(SPOKE_OUTER, angle);
      LINES.push(`M${a.x.toFixed(4)} ${a.y.toFixed(4)}L${b.x.toFixed(4)} ${b.y.toFixed(4)}`);
    }
    const profile = [
      ...ARC_PROFILE.slice(1)
        .reverse()
        .map(([deg, r]) => polar(r, base - deg * DEG)),
      ...ARC_PROFILE.map(([deg, r]) => polar(r, base + deg * DEG)),
    ];
    LINES.push(profile.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(4)} ${p.y.toFixed(4)}`).join(''));
  }

  const BUTTON_DOTS: Point[] = [1, 2, 3, 4, 5, 6, 7, 8].map((button) => polar(1, buttonAngle(button)));
</script>

<script lang="ts">
  interface Props {
    /** 盤面圓心（viewBox 像素）。 */
    center: Point;
    /** 外環半徑（viewBox 像素）。 */
    radius: number;
  }

  let { center, radius }: Props = $props();
</script>

<!-- 底圖不帶語意，讀屏略過；線寬用 non-scaling-stroke，以 viewBox 像素指定，和原圖一樣粗細。 -->
<g class="board" aria-hidden="true" transform={`translate(${center.x} ${center.y}) scale(${radius})`}>
  {#each TILES as d, index (index)}
    <path class="tile" {d} />
  {/each}
  {#each LINES as d, index (index)}
    <path class="dotted" {d} />
  {/each}
  <circle class="ring" cx="0" cy="0" r="1" />
  {#each BUTTON_DOTS as dot, index (index)}
    <circle class="button-dot" cx={dot.x} cy={dot.y} r="0.03" />
  {/each}
</g>

<style>
  /* 盤面永遠是黑底白線，不隨淺色配色變色（和原本的 PNG 一樣）。 */
  .tile {
    fill: #121014;
    stroke: #5c5a5b;
    stroke-width: 2;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }

  .dotted {
    fill: none;
    stroke: #b9b9bd;
    stroke-width: 2.2;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-dasharray: 0 9;
    vector-effect: non-scaling-stroke;
  }

  .ring {
    fill: none;
    stroke: #ffffff;
    stroke-width: 5.5;
    vector-effect: non-scaling-stroke;
  }

  .button-dot {
    fill: #ffffff;
  }
</style>
