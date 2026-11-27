/*
 * 「照遊戲上色」用到的顏色（量自遊戲畫面與 MajdataPlay，來由見 maimotion-ui 的 disc-stage.md §2.1）。
 * 盤面的 CSS 只寫 var(--g-…)，實際的顏色由這裡依「彩度」換算後，寫在盤面 <svg> 的 style 上。
 * 2026-11-25 使用者覺得遊戲的顏色太鮮豔、和介面不搭，要能降低彩度：在 OKLab 裡只縮小彩度，
 * 明暗與色相不變，100% 就是遊戲原本的顏色，0% 是同樣明暗的灰。
 */

export const GAME_COLORS = {
  /** Tap／Hold：淺、深、EX 光暈。 */
  tap: '#ff7fd0',
  'tap-deep': '#ff1f7f',
  'tap-ex': '#ffc0f2',
  /** Slide 的星星與軌道。 */
  slide: '#22b8f0',
  touch: '#1aa6f7',
  /** 同時押：淺、深；連線、同時押的星星與軌道。 */
  each: '#fbfd1e',
  'each-deep': '#f4cc2d',
  'each-line': '#faf52c',
  /** 同時押與 Break 的 EX 光暈。 */
  'gold-ex': '#fffe77',
  /** Break：淺、深；Break 的星星與軌道。 */
  break: '#ffb24a',
  'break-deep': '#ec7a10',
  'break-star': '#f0913a',
  /** Touch Hold 的四片三角形（右上、右下、左下、左上）與外框四段（淡一階）。 */
  'th-1': '#f64303',
  'th-2': '#f3ee00',
  'th-3': '#10a667',
  'th-4': '#0093f6',
  'thb-1': '#f5713a',
  'thb-2': '#f2d62a',
  'thb-3': '#2fbf7a',
  'thb-4': '#3aa6ee',
} as const;

/** 打擊效果的光暈（照遊戲上色時）：六角形與星星、Touch 的圓、按住 Hold 時的光圈。 */
export const GAME_GLOW = {
  hit: '#f2e75c',
  disc: '#f8ee7c',
  loop: '#fff76a',
} as const;

/** 彩度設定（百分比）的範圍；預設 100 是遊戲原本的顏色。 */
export const SATURATION_RANGE = { min: 0, max: 100, step: 5, initial: 100 } as const;

/** 設定值（百分比）換成 0–1 的倍率；壞掉的存檔值當成 100%。 */
export function saturationFactor(percent: number): number {
  if (!Number.isFinite(percent)) return 1;
  return Math.min(1, Math.max(0, percent / 100));
}

/** 盤面 <svg> 上的 style：每個遊戲顏色一個 --g-… 變數。 */
export function gameColorStyle(factor: number): string {
  return Object.entries(GAME_COLORS)
    .map(([name, hex]) => `--g-${name}:${desaturate(hex, factor)}`)
    .join(';');
}

/** 只縮小彩度（OKLab 的 a、b 乘上 factor），明暗與色相不變；factor 1 原樣回傳。 */
export function desaturate(hex: string, factor: number): string {
  if (factor >= 1) return hex;
  const value = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [(value >> 16) & 255, (value >> 8) & 255, value & 255].map((c) => toLinear(c / 255));
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = (1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s) * Math.max(0, factor);
  const B = (0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s) * Math.max(0, factor);
  const l2 = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3;
  const m2 = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3;
  const s2 = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
  const out = [
    4.0767416621 * l2 - 3.3077115913 * m2 + 0.2309699292 * s2,
    -1.2684380046 * l2 + 2.6097574011 * m2 - 0.3413193965 * s2,
    -0.0041960863 * l2 - 0.7034186147 * m2 + 1.707614701 * s2,
  ];
  return `#${out.map((c) => Math.round(fromLinear(c) * 255).toString(16).padStart(2, '0')).join('')}`;
}

function toLinear(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function fromLinear(c: number): number {
  const v = Math.min(1, Math.max(0, c));
  return v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055;
}
