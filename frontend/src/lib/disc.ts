import type { Point } from './types';

/** resource/maimai.png 的原始像素尺寸；SVG viewBox 直接用這個座標系。 */
export const IMAGE_WIDTH = 1014;
export const IMAGE_HEIGHT = 980;

export interface Calibration {
  /** 圓心 X，佔圖片寬度比例 */
  centerX: number;
  /** 圓心 Y，佔圖片高度比例 */
  centerY: number;
  /** 盤面半徑（盤面座標 1.0），佔圖片寬度比例 */
  radius: number;
}

/**
 * 由 resource/maimai.png 的白色外圈以最小平方圓擬合得到：
 * 圓心 (500.50, 485.50)、半徑 480.03 像素。
 * 盤面位置一律由 Rust 座標換算，這組數值只是把背景圖對齊上去。
 */
export const DEFAULT_CALIBRATION: Calibration = {
  centerX: 0.4936,
  centerY: 0.4954,
  radius: 0.4734,
};

export function centerPx(calibration: Calibration): Point {
  return { x: calibration.centerX * IMAGE_WIDTH, y: calibration.centerY * IMAGE_HEIGHT };
}

export function radiusPx(calibration: Calibration): number {
  return calibration.radius * IMAGE_WIDTH;
}

/** 盤面座標（中心 0,0、半徑 1、y 向下）→ SVG viewBox 像素座標。 */
export function toStage(calibration: Calibration, point: Point): Point {
  const center = centerPx(calibration);
  const r = radiusPx(calibration);
  return { x: center.x + point.x * r, y: center.y + point.y * r };
}

/** 盤面長度 → viewBox 像素長度。 */
export function toStageLength(calibration: Calibration, length: number): number {
  return length * radiusPx(calibration);
}

/**
 * 第 k 鍵的盤面座標，定義見 docs/RULEBOOK.md §3 與 src/geometry.rs：
 * θ = -π/2 + π/8 + (k-1)·π/4，位置 (cos θ, sin θ)。
 * 只用於畫鍵位標記；音符位置一律取 Rust 回傳的 position。
 */
export function buttonPoint(button: number): Point {
  const angle = -Math.PI / 2 + Math.PI / 8 + (button - 1) * (Math.PI / 4);
  return { x: Math.cos(angle), y: Math.sin(angle) };
}

export const BUTTONS = [1, 2, 3, 4, 5, 6, 7, 8];

export function polylinePath(points: Point[], calibration: Calibration): string {
  if (points.length === 0) return '';
  let d = '';
  for (let i = 0; i < points.length; i += 1) {
    const p = toStage(calibration, points[i]);
    d += `${i === 0 ? 'M' : 'L'}${p.x.toFixed(2)} ${p.y.toFixed(2)}`;
  }
  return d;
}

/** 以 path 樣本取出等距的箭頭位置與角度，用來畫 Slide 的連續箭頭。 */
export interface Chevron {
  x: number;
  y: number;
  angle: number;
  u: number;
}

export function chevrons(samples: { u: number; x: number; y: number }[], count: number): Chevron[] {
  if (samples.length < 2) return [];
  const result: Chevron[] = [];
  for (let i = 0; i < count; i += 1) {
    const u = (i + 0.5) / count;
    let hi = 1;
    while (hi < samples.length - 1 && samples[hi].u < u) hi += 1;
    const a = samples[hi - 1];
    const b = samples[hi];
    const span = b.u - a.u;
    const k = span <= 0 ? 0 : (u - a.u) / span;
    const x = a.x + (b.x - a.x) * k;
    const y = a.y + (b.y - a.y) * k;
    const angle = (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI;
    result.push({ x, y, angle, u });
  }
  return result;
}

export function pointOnSamples(samples: { u: number; x: number; y: number }[], u: number): Point {
  if (samples.length === 0) return { x: 0, y: 0 };
  const clamped = Math.min(1, Math.max(0, u));
  let hi = 1;
  while (hi < samples.length - 1 && samples[hi].u < clamped) hi += 1;
  const a = samples[hi - 1];
  const b = samples[hi];
  const span = b.u - a.u;
  const k = span <= 0 ? 0 : (clamped - a.u) / span;
  return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
}
