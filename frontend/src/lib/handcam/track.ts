// 手持鏡頭追蹤：每一格估計「這一格像素 → 參考格像素」的透視。
// 特徵點：FAST-9 角點＋BRIEF 256 位元描述子，漢明距離比對（比值檢查）後以 RANSAC 估計。
// 參考格只取盤面外圈框體（盤面半徑 1.08–1.75）的特徵點：螢幕內容一直在變，框體不會動。

import { apply, multiply, ransac, type Mat3, type Vec2 } from './homography';
import type { RgbaFrame } from './pose';

/** 追蹤在縮小一半的灰階影像上做。 */
const SCALE = 2;
const FAST_THRESHOLD = 18;
const MAX_FEATURES = 1200;
const GRID_X = 16;
const GRID_Y = 9;
const PATCH = 15; // BRIEF 取樣半徑
const BORDER = PATCH + 2;
const RATIO = 0.8;
const RANSAC_PX = 2; // 縮小影像上的像素（原圖 4 px）
export const MIN_INLIERS = 12;

interface Gray {
  data: Uint8Array;
  width: number;
  height: number;
}

interface Features {
  points: Vec2[];
  /** 每點 8 個 uint32 */
  desc: Uint32Array;
}

function toGray(frame: RgbaFrame): Gray {
  const width = Math.floor(frame.width / SCALE);
  const height = Math.floor(frame.height / SCALE);
  const out = new Uint8Array(width * height);
  const src = frame.data;
  const w = frame.width;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      let sum = 0;
      for (let dy = 0; dy < SCALE; dy += 1) {
        for (let dx = 0; dx < SCALE; dx += 1) {
          const at = ((y * SCALE + dy) * w + (x * SCALE + dx)) * 4;
          sum += src[at] * 77 + src[at + 1] * 150 + src[at + 2] * 29;
        }
      }
      out[y * width + x] = sum >> (8 + 2);
    }
  }
  return { data: out, width, height };
}

/** 5×5 二項式模糊（BRIEF 前先平滑，降低雜訊）。 */
function blur(g: Gray): Uint8Array {
  const { data, width, height } = g;
  const tmp = new Uint16Array(width * height);
  const out = new Uint8Array(width * height);
  const k = [1, 4, 6, 4, 1];
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      let s = 0;
      for (let i = -2; i <= 2; i += 1) {
        const xx = Math.min(width - 1, Math.max(0, x + i));
        s += data[y * width + xx] * k[i + 2];
      }
      tmp[y * width + x] = s;
    }
  }
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      let s = 0;
      for (let i = -2; i <= 2; i += 1) {
        const yy = Math.min(height - 1, Math.max(0, y + i));
        s += tmp[yy * width + x] * k[i + 2];
      }
      out[y * width + x] = s >> 8;
    }
  }
  return out;
}

const CIRCLE: [number, number][] = [
  [0, -3], [1, -3], [2, -2], [3, -1], [3, 0], [3, 1], [2, 2], [1, 3],
  [0, 3], [-1, 3], [-2, 2], [-3, 1], [-3, 0], [-3, -1], [-2, -2], [-1, -3],
];

/** FAST-9：圓周 16 點中有連續 9 點都比中心亮（或暗）超過門檻。回傳分數圖（0 表示不是角點）。 */
function fast(g: Gray, allow: ((x: number, y: number) => boolean) | null): Float32Array {
  const { data, width, height } = g;
  const score = new Float32Array(width * height);
  const offsets = CIRCLE.map(([dx, dy]) => dy * width + dx);
  const ring = new Int16Array(16);
  for (let y = BORDER; y < height - BORDER; y += 1) {
    for (let x = BORDER; x < width - BORDER; x += 1) {
      const at = y * width + x;
      const c = data[at];
      const hi = c + FAST_THRESHOLD;
      const lo = c - FAST_THRESHOLD;
      // 快速排除：上下左右四點至少要有三點同方向
      const p0 = data[at + offsets[0]];
      const p4 = data[at + offsets[4]];
      const p8 = data[at + offsets[8]];
      const p12 = data[at + offsets[12]];
      const brighter = (p0 > hi ? 1 : 0) + (p4 > hi ? 1 : 0) + (p8 > hi ? 1 : 0) + (p12 > hi ? 1 : 0);
      const darker = (p0 < lo ? 1 : 0) + (p4 < lo ? 1 : 0) + (p8 < lo ? 1 : 0) + (p12 < lo ? 1 : 0);
      if (brighter < 3 && darker < 3) continue;
      if (allow && !allow(x, y)) continue;
      for (let k = 0; k < 16; k += 1) ring[k] = data[at + offsets[k]] - c;
      let best = 0;
      for (const sign of [1, -1]) {
        let run = 0;
        for (let k = 0; k < 16 + 9; k += 1) {
          const d = sign * ring[k % 16];
          if (d > FAST_THRESHOLD) {
            run += 1;
            if (run >= 9) {
              let s = 0;
              for (let j = 0; j < 16; j += 1) s += Math.max(0, sign * ring[j] - FAST_THRESHOLD);
              best = Math.max(best, s);
              break;
            }
          } else run = 0;
        }
      }
      score[at] = best;
    }
  }
  return score;
}

/** BRIEF 取樣點對（固定亂數種子、高斯分佈），所有影片共用。 */
const PAIRS: Int8Array = (() => {
  let state = 0x9e3779b9;
  const random = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 4294967296;
  };
  const gauss = () => {
    const u = Math.max(random(), 1e-9);
    const v = random();
    return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
  };
  const clamp = (value: number) => Math.max(-PATCH, Math.min(PATCH, Math.round(value)));
  const out = new Int8Array(256 * 4);
  for (let k = 0; k < 256; k += 1) {
    for (let j = 0; j < 4; j += 1) out[k * 4 + j] = clamp(gauss() * (PATCH / 2.5));
  }
  return out;
})();

function features(g: Gray, allow: ((x: number, y: number) => boolean) | null): Features {
  const score = fast(g, allow);
  const { width, height } = g;
  // 3×3 非極大值抑制後依網格分配，避免特徵點全擠在同一處
  const cells: { at: number; s: number }[][] = Array.from({ length: GRID_X * GRID_Y }, () => []);
  for (let y = BORDER; y < height - BORDER; y += 1) {
    for (let x = BORDER; x < width - BORDER; x += 1) {
      const at = y * width + x;
      const s = score[at];
      if (s === 0) continue;
      let peak = true;
      for (let dy = -1; dy <= 1 && peak; dy += 1) {
        for (let dx = -1; dx <= 1; dx += 1) {
          if ((dx || dy) && score[at + dy * width + dx] > s) {
            peak = false;
            break;
          }
        }
      }
      if (!peak) continue;
      const cell = Math.min(GRID_Y - 1, Math.floor((y / height) * GRID_Y)) * GRID_X + Math.min(GRID_X - 1, Math.floor((x / width) * GRID_X));
      cells[cell].push({ at, s });
    }
  }
  const perCell = Math.ceil(MAX_FEATURES / (GRID_X * GRID_Y)) * 2;
  let chosen: { at: number; s: number }[] = [];
  for (const cell of cells) {
    cell.sort((a, b) => b.s - a.s);
    chosen.push(...cell.slice(0, perCell));
  }
  chosen.sort((a, b) => b.s - a.s);
  chosen = chosen.slice(0, MAX_FEATURES);
  const smooth = blur(g);
  const desc = new Uint32Array(chosen.length * 8);
  const points: Vec2[] = [];
  chosen.forEach(({ at }, index) => {
    const x = at % width;
    const y = (at - x) / width;
    points.push({ x, y });
    for (let k = 0; k < 256; k += 1) {
      const a = smooth[(y + PAIRS[k * 4 + 1]) * width + x + PAIRS[k * 4]];
      const b = smooth[(y + PAIRS[k * 4 + 3]) * width + x + PAIRS[k * 4 + 2]];
      if (a < b) desc[index * 8 + (k >> 5)] |= 1 << (k & 31);
    }
  });
  return { points, desc };
}

function popcount(v: number): number {
  v -= (v >>> 1) & 0x55555555;
  v = (v & 0x33333333) + ((v >>> 2) & 0x33333333);
  return (((v + (v >>> 4)) & 0x0f0f0f0f) * 0x01010101) >>> 24;
}

/** 這一格每個特徵找參考格最近與次近的描述子，通過比值檢查的才留下。 */
function match(a: Features, b: Features): [Vec2[], Vec2[]] {
  const src: Vec2[] = [];
  const dst: Vec2[] = [];
  const na = a.points.length;
  const nb = b.points.length;
  for (let i = 0; i < na; i += 1) {
    let best = 1e9;
    let second = 1e9;
    let bestJ = -1;
    const ai = i * 8;
    for (let j = 0; j < nb; j += 1) {
      const bj = j * 8;
      let d = 0;
      for (let w = 0; w < 8 && d < second; w += 1) d += popcount(a.desc[ai + w] ^ b.desc[bj + w]);
      if (d < best) {
        second = best;
        best = d;
        bestJ = j;
      } else if (d < second) second = d;
    }
    if (bestJ >= 0 && best < RATIO * second && best < 80) {
      src.push(a.points[i]);
      dst.push(b.points[bestJ]);
    }
  }
  return [src, dst];
}

const DOWN: Mat3 = Float64Array.from([1 / SCALE, 0, 0, 0, 1 / SCALE, 0, 0, 0, 1]);
const UP: Mat3 = Float64Array.from([SCALE, 0, 0, 0, SCALE, 0, 0, 0, 1]);

export interface TrackResult {
  /** 這一格像素 → 參考格像素；失敗為 null */
  toRef: Mat3 | null;
  inliers: number;
}

export class CameraTracker {
  #ref: Features;
  readonly refFeatures: number;

  /**
   * reference：參考格；boardFromRef：參考格像素 → 盤面的透視（使用者標的環點求得）。
   * 只用參考格中盤面半徑 1.08–1.75 的框體特徵。
   */
  constructor(reference: RgbaFrame, boardFromRef: Mat3) {
    const gray = toGray(reference);
    const allow = (x: number, y: number) => {
      const p = apply(boardFromRef, x * SCALE, y * SCALE);
      const r = Math.hypot(p.x, p.y);
      return r > 1.08 && r < 1.75;
    };
    this.#ref = features(gray, allow);
    this.refFeatures = this.#ref.points.length;
  }

  track(frame: RgbaFrame): TrackResult {
    const current = features(toGray(frame), null);
    if (current.points.length < MIN_INLIERS) return { toRef: null, inliers: 0 };
    const [src, dst] = match(current, this.#ref);
    if (src.length < 15) return { toRef: null, inliers: 0 };
    const result = ransac(src, dst, RANSAC_PX, 500);
    if (!result || result.inliers < MIN_INLIERS) return { toRef: null, inliers: result?.inliers ?? 0 };
    // 縮小座標的結果換回原圖像素：UP · H · DOWN
    return { toRef: multiply(UP, multiply(result.h, DOWN)), inliers: result.inliers };
  }
}
