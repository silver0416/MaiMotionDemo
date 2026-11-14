/**
 * 演示清單：<Demo id="…" /> 的 id → 元件檔（相對於 src/lib/demos/）與 props。
 * - 盤面演示（kind: 'disc'，徽章「核心實際結果」）：一律是 ./disc/DiscDemo.svelte，props { name: id }；
 *   資料由 scripts/gen-demos.mjs 呼叫 Rust 核心產生，存成 ./data/<id>.json。
 * - 示意動畫（kind: 'ui'，徽章「示意」）：./ui/<PascalCase>.svelte；disc-legend、palm-radius、glide-rule 例外放 ./disc/。
 * ratio 是載入前預留的長寬比（寬 / 高），避免版面跳動；元件做好後若比例不同，改這裡。
 * reserve：高度不是固定比例（盤面正方形＋固定高度的原文列與播放列、示意動畫的固定列高與旁白列），
 * 改用「高 = k × 寬 + b」預留，寬是舞台寬度（./disc/ 的最寬 480px，其他撐滿內容寬）；
 * 滑鼠與觸控（pointer: coarse）各一組，由兩個寬度實測的高度算出（盤面類 480px 與手機 343px；
 * ./ui/ 的是桌面內容寬 736px 與手機 343px，取剛載入、還沒互動時的高度）。
 * 不支援 container query 的瀏覽器退回 ratio。元件改版面後要重量。
 */

export type DemoKind = 'disc' | 'ui';

/** 高 = k × 寬 + b（px）。 */
export type ReserveLine = [k: number, b: number];

export interface DemoReserve {
  fine: ReserveLine;
  coarse: ReserveLine;
  /**
   * 盤面演示的 simai 原文字數。原文太長時那一列會出現橫向捲軸（滑鼠操作時高 8.5px；
   * 觸控時原文列由 44px 的複製鈕撐高，不受影響）。要和 scripts/gen-demos.mjs 的 DEMOS 一致。
   */
  chars?: number;
  /**
   * 分段預留（./ui/ 的示意動畫）：旁白與按鈕換行、容器查詢換排列，高度隨寬度是階梯狀，一條直線對不準。
   * 每段 [from, k, b]：舞台寬 ≥ from（px）起，高 = k × 寬 + b；依 from 由小到大排，第一段 from 為 0。
   * 有 steps 時 Demo.svelte 改用它（容器查詢），fine／coarse 那條直線只在 steps 不適用時當底。
   */
  steps?: { fine: ReserveSeg[]; coarse: ReserveSeg[] };
}

/** 舞台寬 ≥ from（px）時，高 = k × 寬 + b（px）。 */
export type ReserveSeg = [from: number, k: number, b: number];

export interface DemoEntry {
  kind: DemoKind;
  /** 相對於 src/lib/demos/ 的元件路徑。 */
  path: string;
  props: Record<string, unknown>;
  /** CSS aspect-ratio。 */
  ratio: string;
  reserve?: DemoReserve;
}

/** 盤面演示：盤面（正方形）加上方原文列、下方播放列；480px 寬時高 607.6px（ratio 只在不支援 reserve 時用）。 */
const DISC_RATIO = '480 / 607.6';
/** 盤面演示實測：滑鼠 480→607.6、332.8→460.4；觸控 480→645.6、343.2→512.8。 */
const DISC_RESERVE: Omit<DemoReserve, 'chars'> = { fine: [1, 127.6], coarse: [0.9708, 179.6] };

/** 盤面演示的 simai 原文字數（和 gen-demos.mjs 的 DEMOS 相同）。 */
const DISC_SOURCE_CHARS: Record<(typeof DISC_IDS)[number], number> = {
  'tap-alternate': 25,
  'hold-other-hand': 25,
  'slide-basic': 24,
  swap: 36,
  cross: 27,
  'slide-long': 27,
  jack: 18,
  abab: 22,
  'abab-far': 22,
  stair: 26,
  palm: 25,
  'palm-hold': 56,
  wifi: 18,
  'wifi-one-hand': 24,
  'slide-palm': 34,
  'touch-kinds': 35,
  shapes: 73,
};

/** 畫在盤面上的三個示意動畫，實測同上（480px／滑鼠 332.8px／觸控 343.2px）。 */
const UI_DISC_RESERVE: Record<string, DemoReserve> = {
  // 滑鼠 480→932.4、332.8→927.6；觸控 480→932.4、343.2→938
  // steps：2026-11-15 在舞台寬 300～480px 逐 px 重量（圖例換行造成階梯；480px 時已是 953.4）。
  'disc-legend': {
    fine: [0.0326, 916.7],
    coarse: [-0.0409, 952],
    steps: {
      fine: [[0, 1, 835], [308, 1, 635.8], [318, 1, 616.6], [332, 1, 597.4], [356, 1, 578.2], [380, 1, 559], [402, 1, 539.8], [466, 1, 473.4]],
      coarse: [[0, 1, 835], [308, 1, 635.8], [318, 1, 616.6], [332, 1, 597.4], [356, 1, 578.2], [380, 1, 559], [402, 1, 539.8], [466, 1, 473.4]],
    },
  },
  // 滑鼠 480→686.8、332.8→558.8；觸控 480→708.8、343.2→591.2
  'palm-radius': {
    fine: [0.8696, 269.4],
    coarse: [0.8596, 296.2],
    steps: {
      fine: [[0, 1, 250.2], [324, 1, 225.4], [348, 1, 206.2]],
      coarse: [[0, 1, 272.2], [324, 1, 247.4], [348, 1, 228.2]],
    },
  },
  // 滑鼠 480→580.8、332.8→601.6；觸控 480→612.8、343.2→633.6
  'glide-rule': {
    fine: [-0.1413, 648.6],
    coarse: [-0.152, 685.8],
    steps: {
      fine: [[0, 0, 621.2], [325, 0, 600.4], [346, 0, 579.6]],
      coarse: [[0, 0, 653.2], [325, 0, 632.4], [346, 0, 611.6]],
    },
  },
};
/**
 * ./ui/ 的七個示意動畫（2026-11-15 實測，剛載入、還沒互動時的高度）。
 * fine／coarse：舞台寬 736px（桌面內容寬）與 343px（375px 視窗）兩點連線，註解依序是 滑鼠 736→、343→；觸控 736→、343→。
 * steps：在舞台寬 300～760px 逐 px 量出的分段（段內誤差 < 0.6px）；共同的斷點 422（Stage 容器 420px）、
 * 457（旁白列換行），其他是各演示自己的容器查詢或文字換行。元件改了版面要重量。
 */
const UI_RESERVE: Record<string, DemoReserve> = {
  // 424.3、397.6；480.3、501.6（盤面縮圖隨寬度等比，段內斜率 0.625）
  'layout-tour': {
    fine: [0.0681, 374.2],
    coarse: [-0.054, 520.1],
    steps: {
      fine: [[0, 0.625, 183.2], [390, 0.625, 162.4], [422, 0.625, 165.4], [602, 0.625, -35.7]],
      coarse: [[0, 0.625, 287.2], [390, 0.625, 266.4], [422, 0.625, 269.4], [498, 0.625, 221.4], [602, 0.625, 20.3]],
    },
  },
  // 377.4、469.0；387.8、499.0
  'simai-timing': {
    fine: [-0.2331, 548.9],
    coarse: [-0.2829, 596],
    steps: {
      fine: [[0, 0, 469], [406, 0, 451.4], [407, 0, 409.4], [422, 0, 417.4], [457, 0, 377.4]],
      coarse: [[0, 0, 499], [406, 0, 481.4], [407, 0, 429.4], [422, 0, 437.4], [457, 0, 387.8]],
    },
  },
  // 416.5、448.5；418.9、460.5
  'cost-groups': {
    fine: [-0.0814, 476.5],
    coarse: [-0.1058, 496.8],
    steps: {
      fine: [[0, 0, 448.5], [422, 0, 456.5], [457, 0, 416.5]],
      coarse: [[0, 0, 460.5], [422, 0, 468.5], [457, 0, 418.9]],
    },
  },
  // 295.6、483.6；334.0、627.6
  'loop-markers': {
    fine: [-0.4784, 647.7],
    coarse: [-0.7471, 883.8],
    steps: {
      fine: [[0, 0, 559.6], [328, 0, 519.6], [340, 0, 483.6], [346, 0, 443.6], [422, 0, 451.6], [457, 0, 411.6], [546, 0, 371.6], [608, 0, 331.6], [654, 0, 295.6]],
      coarse: [[0, 0, 679.6], [328, 0, 627.6], [346, 0, 575.6], [364, 0, 527.6], [422, 0, 535.6], [457, 0, 486], [546, 0, 434], [608, 0, 382], [702, 0, 334]],
    },
  },
  // 526.6、714.6；661.0、906.6
  'annotate-keys': {
    fine: [-0.4784, 878.7],
    coarse: [-0.6249, 1120.9],
    steps: {
      fine: [[0, 0, 714.6], [422, 0, 722.6], [457, 0, 682.6], [562, 0, 526.6]],
      coarse: [[0, 0, 906.6], [422, 0, 914.6], [457, 0, 865], [562, 0, 661]],
    },
  },
  // 497.6、529.6；680.0、721.6
  'branch-lanes': {
    fine: [-0.0814, 557.5],
    coarse: [-0.1058, 757.9],
    steps: {
      fine: [[0, 0, 529.6], [422, 0, 537.6], [457, 0, 497.6]],
      coarse: [[0, 0, 773.6], [303, 0, 721.6], [422, 0, 729.6], [457, 0, 680]],
    },
  },
  // 498.3、771.1；552.8、883.1
  'video-align': {
    fine: [-0.6941, 1009.2],
    coarse: [-0.8406, 1171.5],
    steps: {
      fine: [[0, 0, 771.1], [370, 0, 743.9], [422, 0, 751.9], [457, 0, 711.9], [522, 0, 498.3]],
      coarse: [[0, 0, 883.1], [358, 0, 835.1], [422, 0, 843.1], [457, 0, 793.5], [462, 0, 766.3], [522, 0, 552.8]],
    },
  },
};
/** 示意動畫預設比例（只在不支援 container query 時用）。 */
const UI_RATIO = '16 / 10';

const DISC_IDS = [
  'tap-alternate',
  'hold-other-hand',
  'slide-basic',
  'swap',
  'cross',
  'slide-long',
  'jack',
  'abab',
  'abab-far',
  'stair',
  'palm',
  'palm-hold',
  'wifi',
  'wifi-one-hand',
  'slide-palm',
  'touch-kinds',
  'shapes',
] as const;

const UI_IDS = [
  'layout-tour',
  'disc-legend',
  'simai-timing',
  'cost-groups',
  'palm-radius',
  'glide-rule',
  'loop-markers',
  'annotate-keys',
  'branch-lanes',
  'video-align',
] as const;

/** 這三個示意動畫畫在盤面上，元件放 ./disc/。 */
const UI_IN_DISC = new Set<string>(['disc-legend', 'palm-radius', 'glide-rule']);

export type DemoId = (typeof DISC_IDS)[number] | (typeof UI_IDS)[number];

function pascal(id: string): string {
  return id
    .split('-')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');
}

export const DEMOS: Record<DemoId, DemoEntry> = Object.fromEntries([
  ...DISC_IDS.map((id) => [
    id,
    {
      kind: 'disc',
      path: './disc/DiscDemo.svelte',
      props: { name: id },
      ratio: DISC_RATIO,
      reserve: { ...DISC_RESERVE, chars: DISC_SOURCE_CHARS[id] },
    },
  ]),
  ...UI_IDS.map((id) => [
    id,
    {
      kind: 'ui',
      path: `./${UI_IN_DISC.has(id) ? 'disc' : 'ui'}/${pascal(id)}.svelte`,
      props: {},
      ratio: UI_IN_DISC.has(id) ? DISC_RATIO : UI_RATIO,
      ...((UI_DISC_RESERVE[id] ?? UI_RESERVE[id]) ? { reserve: UI_DISC_RESERVE[id] ?? UI_RESERVE[id] } : {}),
    },
  ]),
]) as Record<DemoId, DemoEntry>;

export function demoEntry(id: string): DemoEntry | undefined {
  return (DEMOS as Record<string, DemoEntry>)[id];
}
