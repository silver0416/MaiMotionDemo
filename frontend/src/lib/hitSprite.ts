// 打擊效果的光暈圖：每一種圖形先算好一張，播放時只放大、旋轉、調不透明度（為什麼見 hitEffect.ts「光暈怎麼畫」）。
// 算法照原本的濾鏡：線（或填色）畫一次 → 兩種寬度的高斯模糊 → 寬的墊底、窄的疊兩次。
// 高斯模糊用三次方框模糊近似（軟體算圖的常見做法），每張圖幾毫秒；算好的圖整個 App 共用。
// 圖是點陣的，但光暈本來就是柔的，放大到盤面最大也看不出顆粒。
// 第一次打開「打擊效果」時趁空檔把目前配色用得到的圖先算好（warmGlowSprites），播放時不必臨時算。

import { GLOW_SPECS, STAR_INNER, STAR_TIP, glowColor, glowHalf, type GlowKind } from './hitEffect';

/** 每張圖的邊長（像素）。按住 Hold 的光圈會放到最大，給大一點。 */
const SIZE: Record<GlowKind, number> = {
  hex0: 384,
  hex1: 384,
  hex2: 384,
  star0: 384,
  star1: 384,
  star2: 384,
  disc: 384,
  loop: 512,
};

const KINDS = Object.keys(GLOW_SPECS) as GlowKind[];
const cache = new Map<string, string>();

function cacheKey(kind: GlowKind, colored: boolean): string {
  return `${kind}:${colored ? 'colored' : 'plain'}`;
}

/**
 * 某一種光暈圖（data URL，CSP 只允許 data: 的圖）。第一次用到時才算，之後直接取。
 * 圖的正中心是圖形的中心，半邊長是外接圓半徑的 `glowHalf(kind)` 倍；
 * 六角形與星星的第一個頂點朝右（0°），用的時候再轉到該朝的方向。
 */
export function glowSprite(kind: GlowKind, colored: boolean): string {
  const key = cacheKey(kind, colored);
  let url = cache.get(key);
  if (url === undefined) {
    url = render(kind, colored);
    cache.set(key, url);
  }
  return url;
}

/** 趁空檔把某個配色用得到的光暈圖一張一張先算好；已經算過的跳過。 */
export function warmGlowSprites(colored: boolean): void {
  if (typeof document === 'undefined') return;
  const queue = KINDS.filter((kind) => !cache.has(cacheKey(kind, colored)));
  const step = (): void => {
    const kind = queue.shift();
    if (kind === undefined) return;
    glowSprite(kind, colored);
    later(step);
  };
  later(step);
}

/** 一張一張排到空檔；一直沒有空檔（例如正在播放）也最多等 0.1 秒就算下一張，不會拖到用的時候才臨時算。 */
function later(task: () => void): void {
  if (typeof requestIdleCallback === 'function') requestIdleCallback(task, { timeout: 100 });
  else setTimeout(task, 16);
}

function render(kind: GlowKind, colored: boolean): string {
  const spec = GLOW_SPECS[kind];
  const size = SIZE[kind];
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext('2d');
  if (!context) return '';
  // 外接圓半徑換成幾個像素；圖形畫在正中間。
  const unit = size / 2 / glowHalf(kind);
  const middle = size / 2;
  context.fillStyle = '#fff';
  context.strokeStyle = '#fff';
  context.beginPath();
  if (spec.shape === 'disc' || spec.shape === 'ring') {
    context.arc(middle, middle, unit, 0, Math.PI * 2);
  } else {
    const star = spec.shape === 'star';
    const corners = star ? 10 : 6;
    for (let index = 0; index < corners; index += 1) {
      const reach = star ? (index % 2 === 1 ? STAR_INNER : STAR_TIP) : 1;
      const theta = (index * 2 * Math.PI) / corners;
      const x = middle + Math.cos(theta) * reach * unit;
      const y = middle + Math.sin(theta) * reach * unit;
      if (index === 0) context.moveTo(x, y);
      else context.lineTo(x, y);
    }
    context.closePath();
  }
  if (spec.line > 0) {
    context.lineWidth = spec.line * unit;
    context.lineJoin = 'round';
    context.stroke();
  } else {
    context.fill();
  }

  const image = context.getImageData(0, 0, size, size);
  const pixels = image.data;
  const count = size * size;
  const shape = new Float32Array(count);
  for (let index = 0; index < count; index += 1) shape[index] = (pixels[index * 4 + 3] / 255) * spec.alpha;
  const wide = gaussian(shape, size, spec.wide * unit);
  const soft = gaussian(shape, size, spec.soft * unit);
  const [red, green, blue] = hexRgb(glowColor(kind, colored));
  for (let index = 0; index < count; index += 1) {
    // 寬的墊底、窄的疊兩次（一般的「蓋上去」混色）。
    const rest = 1 - soft[index];
    const alpha = 1 - (1 - wide[index]) * rest * rest;
    pixels[index * 4] = red;
    pixels[index * 4 + 1] = green;
    pixels[index * 4 + 2] = blue;
    pixels[index * 4 + 3] = Math.round(Math.min(1, Math.max(0, alpha)) * 255);
  }
  context.putImageData(image, 0, 0);
  return canvas.toDataURL('image/png');
}

function hexRgb(hex: string): [number, number, number] {
  const value = Number.parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

/** 高斯模糊（三次方框模糊近似）；邊界外當作透明。 */
function gaussian(source: Float32Array, size: number, sigma: number): Float32Array {
  const out = source.slice();
  if (sigma < 0.3) return out;
  const temp = new Float32Array(source.length);
  for (const width of boxWidths(sigma)) {
    const radius = (width - 1) / 2;
    boxPass(out, temp, size, radius, true);
    boxPass(temp, out, size, radius, false);
  }
  return out;
}

/** 三個方框的寬度（奇數），疊起來的變異數最接近 sigma²。 */
function boxWidths(sigma: number): number[] {
  const passes = 3;
  const ideal = Math.sqrt((12 * sigma * sigma) / passes + 1);
  let lower = Math.floor(ideal);
  if (lower % 2 === 0) lower -= 1;
  const upper = lower + 2;
  const lowerCount = Math.round(
    (12 * sigma * sigma - passes * lower * lower - 4 * passes * lower - 3 * passes) / (-4 * lower - 4),
  );
  return Array.from({ length: passes }, (_, index) => (index < lowerCount ? lower : upper));
}

/** 沿著一個方向的方框平均（滑動加總），結果寫到 target。 */
function boxPass(source: Float32Array, target: Float32Array, size: number, radius: number, horizontal: boolean): void {
  const scale = 1 / (radius * 2 + 1);
  const stride = horizontal ? 1 : size;
  for (let line = 0; line < size; line += 1) {
    const start = horizontal ? line * size : line;
    let sum = 0;
    for (let index = 0; index < radius && index < size; index += 1) sum += source[start + index * stride];
    for (let index = 0; index < size; index += 1) {
      const enter = index + radius;
      if (enter < size) sum += source[start + enter * stride];
      const leave = index - radius - 1;
      if (leave >= 0) sum -= source[start + leave * stride];
      target[start + index * stride] = sum * scale;
    }
  }
}
