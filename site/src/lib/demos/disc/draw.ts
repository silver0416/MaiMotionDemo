/**
 * 文件站盤面演示共用的繪製函式：照 App 的 DiscStage.svelte 搬過來，
 * 校準固定用 App 的預設值（DEFAULT_CALIBRATION），不縮放。
 * 只負責「畫在哪裡、長什麼樣」，不計算左右手；手與音符的資料一律來自 Rust 核心。
 */
import {
  DEFAULT_CALIBRATION,
  IMAGE_HEIGHT,
  IMAGE_WIDTH,
  centerPx,
  pointOnSamples,
  polylinePath,
  radiusPx,
  toStage,
  toStageLength,
} from '$app/lib/disc';
import { scalePoint } from '$app/lib/notes';
import { touchPolygon } from '$app/lib/touch';
import { palmLabelAnchor, palmLabelUpward } from '$app/lib/palm';
import type { PalmPlacement, PathSample, Point } from '$app/lib/types';

export const CALIBRATION = DEFAULT_CALIBRATION;
export const CENTER = centerPx(CALIBRATION);
export const DISC_RADIUS = radiusPx(CALIBRATION);

// 盤面圓幾乎佔滿畫布，額外留白避免標記與文字被裁掉（同 DiscStage）。
export const MARGIN_X = 90;
export const MARGIN_TOP = 105;
export const MARGIN_BOTTOM = 105;
export const VIEW_BOX = `${-MARGIN_X} ${-MARGIN_TOP} ${IMAGE_WIDTH + MARGIN_X * 2} ${
  IMAGE_HEIGHT + MARGIN_TOP + MARGIN_BOTTOM
}`;

/** 盤面座標 → viewBox 像素。 */
export function px(point: Point): Point {
  return toStage(CALIBRATION, point);
}

/** 盤面長度 → viewBox 像素長度。 */
export function len(value: number): number {
  return toStageLength(CALIBRATION, value);
}

export function linePath(points: Point[]): string {
  return polylinePath(points, CALIBRATION);
}

/** 盤面座標的封閉多邊形 → 畫面路徑，Touch 的三角形與方框共用。 */
export function closedPath(points: Point[]): string {
  return `${linePath(points)}Z`;
}

/** Touch 外形：A／D 四邊、B／E 三角、C 八邊，朝向以落點的半徑方向為基準。 */
export function shapePath(area: string | null, at: Point, radius: number, angle: number): string {
  return closedPath(touchPolygon(area, at, radius, angle));
}

export function starPath(at: Point, outer: number, inner: number): string {
  const p = px(at);
  const ro = len(outer);
  const ri = len(inner);
  let d = '';
  for (let i = 0; i < 10; i += 1) {
    const r = i % 2 === 0 ? ro : ri;
    const angle = (-90 + i * 36) * (Math.PI / 180);
    d += `${i === 0 ? 'M' : 'L'}${(p.x + Math.cos(angle) * r).toFixed(2)} ${(p.y + Math.sin(angle) * r).toFixed(2)}`;
  }
  return `${d}Z`;
}

/** Slide 軌道已通過的部分。 */
export function partialPath(samples: PathSample[], upTo: number): string {
  if (samples.length === 0 || upTo <= 0) return '';
  const points: Point[] = [];
  for (const sample of samples) {
    if (sample.u > upTo) break;
    points.push({ x: sample.x, y: sample.y });
  }
  points.push(pointOnSamples(samples, upTo));
  return linePath(points);
}

/** Hold 長條的半寬：取 Tap 空心圓的半徑，筆觸與 Tap 對齊。 */
export const HOLD_HALF_WIDTH = 0.085;
const HOLD_TIP = HOLD_HALF_WIDTH * 0.5;

/**
 * Hold 長條的輪廓（同 DiscStage.holdOutline）：頭在 factor 倍的鍵位上，尾往盤面中心延伸 length。
 */
export function holdOutline(position: Point, factor: number, length: number): Point[] {
  const head = scalePoint(position, factor);
  const tail = scalePoint(position, Math.max(factor - length, 0.06));
  const base = Math.hypot(position.x, position.y) || 1;
  const ux = position.x / base;
  const uy = position.y / base;
  const w = HOLD_HALF_WIDTH;
  const tip = HOLD_TIP;
  return [
    { x: head.x + ux * tip, y: head.y + uy * tip },
    { x: head.x - uy * w, y: head.y + ux * w },
    { x: tail.x - uy * w, y: tail.y + ux * w },
    { x: tail.x - ux * tip, y: tail.y - uy * tip },
    { x: tail.x + uy * w, y: tail.y - ux * w },
    { x: head.x + uy * w, y: head.y - ux * w },
  ];
}

/** Hold 長條在判定前的全長（同 DiscStage）。 */
export function holdLength(duration: number): number {
  return Math.min(0.62, Math.max(0.14, duration * 0.42));
}

/** 換手菱形，以 (0,0) 為中心，再用 transform 移到位置。 */
export function diamondPath(): string {
  const r = len(0.06);
  return `M0 ${-r} L${r} 0 L0 ${r} L${-r} 0 Z`;
}

/** Slide 箭頭，以 (0,0) 為尖端，再用 transform 移到位置並旋轉。 */
export function chevronPath(): string {
  return `M${-len(0.035)} ${-len(0.032)} L0 0 L${-len(0.035)} ${len(0.032)}`;
}

/** 掌心十字：中間留空，手標記蓋上去之後四個端點仍看得見。 */
export function palmCrossPath(at: Point): string {
  const tickIn = len(0.13);
  const tickOut = len(0.21);
  return `M${at.x + tickIn} ${at.y} H${at.x + tickOut} M${at.x - tickIn} ${at.y} H${at.x - tickOut} M${at.x} ${at.y + tickIn} V${at.y + tickOut} M${at.x} ${at.y - tickIn} V${at.y - tickOut}`;
}

const PALM_TITLE_SIZE = 27;
const PALM_LIST_SIZE = 23;

/** 手掌標籤的兩行基線位置（同 DiscStage.palmLabel），夾回 viewBox 之內。 */
export function palmLabel(placement: PalmPlacement): { x: number; title: number; list: number } {
  const anchor = px(palmLabelAnchor(placement));
  const gap = PALM_TITLE_SIZE + 4;
  let title = palmLabelUpward(placement) ? anchor.y - gap : anchor.y + gap * 0.7;
  let list = title + PALM_LIST_SIZE + 10;
  const top = -MARGIN_TOP + PALM_TITLE_SIZE + 6;
  const bottom = IMAGE_HEIGHT + MARGIN_BOTTOM - 6;
  const down = Math.max(0, top - title);
  title += down;
  list += down;
  const up = Math.max(0, list - bottom);
  title -= up;
  list -= up;
  const x = Math.min(Math.max(anchor.x, -MARGIN_X + 110), IMAGE_WIDTH + MARGIN_X - 110);
  return { x, title, list };
}

/** Touch 落點參考標記畫得比音符小一級（同 DiscStage.sensorRadius）。 */
export function sensorRadius(area: string): number {
  if (area === 'C') return 0.085;
  if (area === 'B' || area === 'E') return 0.068;
  return 0.075;
}
