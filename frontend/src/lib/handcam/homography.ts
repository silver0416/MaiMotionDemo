// 平面透視（homography）：3×3 矩陣以列優先 9 個數字表示，把 (x, y) 對應到 (x', y')。
// 用在「影片畫面像素 → 盤面座標（半徑 1，y 向下，與 Rust 幾何一致）」與「這一格 → 參考格」。

export type Mat3 = Float64Array;
export interface Vec2 {
  x: number;
  y: number;
}

export function identity(): Mat3 {
  return Float64Array.from([1, 0, 0, 0, 1, 0, 0, 0, 1]);
}

export function apply(h: Mat3, x: number, y: number): Vec2 {
  const w = h[6] * x + h[7] * y + h[8];
  return { x: (h[0] * x + h[1] * y + h[2]) / w, y: (h[3] * x + h[4] * y + h[5]) / w };
}

export function multiply(a: Mat3, b: Mat3): Mat3 {
  const out = new Float64Array(9);
  for (let r = 0; r < 3; r += 1) {
    for (let c = 0; c < 3; c += 1) {
      out[r * 3 + c] = a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
    }
  }
  return out;
}

export function invert(h: Mat3): Mat3 | null {
  const [a, b, c, d, e, f, g, i, j] = h;
  const A = e * j - f * i;
  const B = -(d * j - f * g);
  const C = d * i - e * g;
  const det = a * A + b * B + c * C;
  if (!Number.isFinite(det) || Math.abs(det) < 1e-12) return null;
  const inv = Float64Array.from([
    A,
    -(b * j - c * i),
    b * f - c * e,
    B,
    a * j - c * g,
    -(a * f - c * d),
    C,
    -(a * i - b * g),
    a * e - b * d,
  ]);
  for (let k = 0; k < 9; k += 1) inv[k] /= det;
  return inv;
}

/** 以 Gauss 消去法解 n×n 線性方程（就地修改）；奇異時回傳 null。 */
function solve(m: number[][], v: number[]): number[] | null {
  const n = v.length;
  for (let col = 0; col < n; col += 1) {
    let pivot = col;
    for (let row = col + 1; row < n; row += 1) if (Math.abs(m[row][col]) > Math.abs(m[pivot][col])) pivot = row;
    if (Math.abs(m[pivot][col]) < 1e-12) return null;
    [m[col], m[pivot]] = [m[pivot], m[col]];
    [v[col], v[pivot]] = [v[pivot], v[col]];
    for (let row = col + 1; row < n; row += 1) {
      const factor = m[row][col] / m[col][col];
      if (factor === 0) continue;
      for (let k = col; k < n; k += 1) m[row][k] -= factor * m[col][k];
      v[row] -= factor * v[col];
    }
  }
  const out = new Array<number>(n);
  for (let row = n - 1; row >= 0; row -= 1) {
    let sum = v[row];
    for (let k = row + 1; k < n; k += 1) sum -= m[row][k] * out[k];
    out[row] = sum / m[row][row];
  }
  return out;
}

/** 正規化：平移到重心、縮放到平均距離 √2，提高數值穩定度。 */
function normalizer(points: Vec2[]): Mat3 {
  let cx = 0;
  let cy = 0;
  for (const p of points) {
    cx += p.x;
    cy += p.y;
  }
  cx /= points.length;
  cy /= points.length;
  let mean = 0;
  for (const p of points) mean += Math.hypot(p.x - cx, p.y - cy);
  mean /= points.length;
  const s = mean > 1e-12 ? Math.SQRT2 / mean : 1;
  return Float64Array.from([s, 0, -s * cx, 0, s, -s * cy, 0, 0, 1]);
}

/**
 * 以最小平方法（h33 = 1）求 src → dst 的透視矩陣；至少 4 組對應點。
 * 退化（三點共線等）時回傳 null。
 */
export function fit(src: Vec2[], dst: Vec2[]): Mat3 | null {
  const n = Math.min(src.length, dst.length);
  if (n < 4) return null;
  const ns = normalizer(src.slice(0, n));
  const nd = normalizer(dst.slice(0, n));
  // 正規方程 AᵀA h = Aᵀb（8 個未知數）
  const ata = Array.from({ length: 8 }, () => new Array<number>(8).fill(0));
  const atb = new Array<number>(8).fill(0);
  const add = (row: number[], rhs: number) => {
    for (let r = 0; r < 8; r += 1) {
      if (row[r] === 0) continue;
      for (let c = 0; c < 8; c += 1) ata[r][c] += row[r] * row[c];
      atb[r] += row[r] * rhs;
    }
  };
  for (let k = 0; k < n; k += 1) {
    const s = apply(ns, src[k].x, src[k].y);
    const d = apply(nd, dst[k].x, dst[k].y);
    add([s.x, s.y, 1, 0, 0, 0, -s.x * d.x, -s.y * d.x], d.x);
    add([0, 0, 0, s.x, s.y, 1, -s.x * d.y, -s.y * d.y], d.y);
  }
  const h = solve(ata, atb);
  if (!h || h.some((value) => !Number.isFinite(value))) return null;
  const hn = Float64Array.from([...h, 1]);
  const ndInv = invert(nd);
  if (!ndInv) return null;
  const out = multiply(ndInv, multiply(hn, ns));
  if (Math.abs(out[8]) < 1e-12) return null;
  for (let k = 0; k < 9; k += 1) out[k] /= out[8];
  return out;
}

/** 決定性的亂數（RANSAC 抽樣用），同一份輸入每次結果相同。 */
function rng(seed: number): () => number {
  let state = seed >>> 0 || 1;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 4294967296;
  };
}

export interface RansacResult {
  h: Mat3;
  inliers: number;
  mask: Uint8Array;
}

/** RANSAC 透視估計：threshold 為 dst 座標下的重投影誤差上限。 */
export function ransac(src: Vec2[], dst: Vec2[], threshold: number, iterations = 400, seed = 12345): RansacResult | null {
  const n = Math.min(src.length, dst.length);
  if (n < 4) return null;
  const random = rng(seed);
  const t2 = threshold * threshold;
  let best: Mat3 | null = null;
  let bestCount = 0;
  const pick = new Array<number>(4);
  for (let iter = 0; iter < iterations; iter += 1) {
    for (let k = 0; k < 4; k += 1) {
      let candidate: number;
      do candidate = Math.floor(random() * n);
      while (pick.slice(0, k).includes(candidate));
      pick[k] = candidate;
    }
    const h = fit(
      pick.map((k) => src[k]),
      pick.map((k) => dst[k]),
    );
    if (!h) continue;
    let count = 0;
    for (let k = 0; k < n; k += 1) {
      const p = apply(h, src[k].x, src[k].y);
      const dx = p.x - dst[k].x;
      const dy = p.y - dst[k].y;
      if (dx * dx + dy * dy < t2) count += 1;
    }
    if (count > bestCount) {
      bestCount = count;
      best = h;
      if (count > n * 0.9) break;
    }
  }
  if (!best || bestCount < 4) return null;
  // 以內點重新擬合
  const mask = new Uint8Array(n);
  const s: Vec2[] = [];
  const d: Vec2[] = [];
  for (let k = 0; k < n; k += 1) {
    const p = apply(best, src[k].x, src[k].y);
    const dx = p.x - dst[k].x;
    const dy = p.y - dst[k].y;
    if (dx * dx + dy * dy < t2) {
      mask[k] = 1;
      s.push(src[k]);
      d.push(dst[k]);
    }
  }
  const refined = fit(s, d) ?? best;
  let inliers = 0;
  for (let k = 0; k < n; k += 1) {
    const p = apply(refined, src[k].x, src[k].y);
    const dx = p.x - dst[k].x;
    const dy = p.y - dst[k].y;
    mask[k] = dx * dx + dy * dy < t2 ? 1 : 0;
    inliers += mask[k];
  }
  return { h: refined, inliers, mask };
}

/** 第 k 鍵（1–8）在盤面上的角度，與 src/geometry.rs 的 angle() 相同。 */
export function buttonAngle(k: number): number {
  return -Math.PI / 2 + Math.PI / 8 + (k - 1) * (Math.PI / 4);
}

export function buttonPoint(k: number, radius = 1): Vec2 {
  const a = buttonAngle(k);
  return { x: radius * Math.cos(a), y: radius * Math.sin(a) };
}
