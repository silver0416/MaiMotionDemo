import type { Point } from './types';

/** SVG viewBox 的座標系（沿用原本 resource/maimai.png 的像素尺寸；盤面底圖已改為 DiscBoard 向量繪製）。 */
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
 * 盤面位置一律由 Rust 座標換算；底圖（DiscBoard）也以這組圓心與半徑繪製，兩者永遠對齊。
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

type Sample = { u: number; x: number; y: number };

/*
 * Slide 箭頭的尺寸（盤面單位，鍵位半徑為 1），量自遊戲畫面：
 * 沒有中心線，整條軌道是一排實心的寬「＞」，兩側是和軌道平行的直邊。
 * 半寬比 Tap 外緣（約 0.114）窄，箭頭帶不會蓋過音符。
 */

/** 相鄰箭頭的間距。 */
export const CHEVRON_PITCH = 0.098;
/** 箭頭帶的半寬。 */
export const CHEVRON_HALF_WIDTH = 0.072;
/** 一個箭頭沿軌道方向的厚度；間距扣掉它就是箭頭之間的縫。 */
export const CHEVRON_THICKNESS = 0.066;
/** 尖端比兩翼往前多少。 */
export const CHEVRON_DEPTH = 0.033;
/** WiFi 的扇形條排得比較疏（遊戲裡一條 WiFi 只有十一條）。 */
export const FAN_PITCH = 0.15;
/** 每個箭頭底下的陰影往後（朝起點）錯開多少：兩條軌道交疊時，上面那條的箭頭才一個一個分得開。 */
export const CHEVRON_SHADOW = 0.016;

/** 一個箭頭：位置、沿路徑的單位切線、在路徑上的 u（和 slideU 同一個尺度）。 */
export interface ChevronMark {
  x: number;
  y: number;
  tx: number;
  ty: number;
  u: number;
}

/** 依路徑長度找出距起點 distance 的位置與 u。cumulative[i] 是第 i 個取樣點的累積長度。 */
function sampleAtDistance(samples: Sample[], cumulative: number[], distance: number): Sample {
  let lo = 1;
  let hi = samples.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (cumulative[mid] < distance) lo = mid + 1;
    else hi = mid;
  }
  const a = samples[lo - 1];
  const b = samples[lo];
  const span = cumulative[lo] - cumulative[lo - 1];
  const k = span <= 0 ? 0 : Math.min(1, Math.max(0, (distance - cumulative[lo - 1]) / span));
  return { u: a.u + (b.u - a.u) * k, x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
}

/**
 * 沿 Slide 路徑依「長度」等距排箭頭（不是依 u 等分），兩端各留半個間距。
 * 方向取前後各一段距離的弦，不取單一取樣線段，彎道上相鄰箭頭的角度才不會抖。
 */
export function chevronMarks(samples: Sample[], pitch = CHEVRON_PITCH): ChevronMark[] {
  if (samples.length < 2) return [];
  const cumulative = [0];
  for (let i = 1; i < samples.length; i += 1) {
    cumulative.push(
      cumulative[i - 1] + Math.hypot(samples[i].x - samples[i - 1].x, samples[i].y - samples[i - 1].y),
    );
  }
  const total = cumulative[cumulative.length - 1];
  if (total <= 1e-9) return [];
  const count = Math.max(1, Math.round(total / pitch));
  const step = total / count;
  const reach = step * 0.75;
  const marks: ChevronMark[] = [];
  for (let i = 0; i < count; i += 1) {
    const distance = (i + 0.5) * step;
    const p = sampleAtDistance(samples, cumulative, distance);
    const a = sampleAtDistance(samples, cumulative, Math.max(0, distance - reach));
    const b = sampleAtDistance(samples, cumulative, Math.min(total, distance + reach));
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const norm = Math.hypot(dx, dy) || 1;
    marks.push({ x: p.x, y: p.y, tx: dx / norm, ty: dy / norm, u: p.u });
  }
  return marks;
}

/**
 * 一條軌道的箭頭帶，已換成畫面座標。suffix[i] 是第 i 個以後所有箭頭合成的一個 path d，
 * 每條路徑只算一次；播放時只依 slideU 挑出還沒走過的那一段，不必每幀重組字串。
 */
export interface ChevronStrip {
  u: number[];
  suffix: string[];
  /** 墊在底下那一層：每個箭頭本身，加上往後錯開的陰影（同樣是第 i 個以後合成一條）。 */
  under: string[];
}

function suffixes(pieces: string[]): string[] {
  const suffix: string[] = new Array(pieces.length + 1);
  suffix[pieces.length] = '';
  for (let i = pieces.length - 1; i >= 0; i -= 1) suffix[i] = pieces[i] + suffix[i + 1];
  return suffix;
}

function stripOf(marks: ChevronMark[], pieces: string[], shadows: string[]): ChevronStrip {
  return {
    u: marks.map((m) => m.u),
    suffix: suffixes(pieces),
    under: suffixes(pieces.map((piece, index) => piece + shadows[index])),
  };
}

/**
 * 一個實心箭頭的六個頂點（封閉多邊形）。
 * left／right 是兩翼的中心，tip 是尖端的中心；三個點各沿切線前後展開半個厚度，
 * 所以兩側是和軌道平行的直邊，整個箭頭沿軌道方向的厚度處處相同。
 */
function chevronPiece(
  calibration: Calibration,
  left: Point,
  tip: Point,
  right: Point,
  tx: number,
  ty: number,
  thickness: number,
  back = 0,
): string {
  const h = thickness * 0.5;
  // back > 0 是整個箭頭沿切線往後退（陰影用）。
  const ox = -tx * back;
  const oy = -ty * back;
  const corners = [
    { x: left.x - tx * h + ox, y: left.y - ty * h + oy },
    { x: left.x + tx * h + ox, y: left.y + ty * h + oy },
    { x: tip.x + tx * h + ox, y: tip.y + ty * h + oy },
    { x: right.x + tx * h + ox, y: right.y + ty * h + oy },
    { x: right.x - tx * h + ox, y: right.y - ty * h + oy },
    { x: tip.x - tx * h + ox, y: tip.y - ty * h + oy },
  ];
  let d = '';
  for (let i = 0; i < corners.length; i += 1) {
    const p = toStage(calibration, corners[i]);
    d += `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)} ${p.y.toFixed(1)}`;
  }
  return `${d}Z`;
}

export function chevronStrip(
  samples: Sample[],
  calibration: Calibration,
  halfWidth = CHEVRON_HALF_WIDTH,
  depth = CHEVRON_DEPTH,
  pitch = CHEVRON_PITCH,
  thickness = CHEVRON_THICKNESS,
): ChevronStrip {
  const marks = chevronMarks(samples, pitch);
  const build = (back: number) =>
    marks.map((m) => {
      // 尖端朝前進方向；兩翼往後退 depth、往兩側展開 halfWidth。
      const tip = { x: m.x + m.tx * depth * 0.5, y: m.y + m.ty * depth * 0.5 };
      const bx = m.x - m.tx * depth * 0.5;
      const by = m.y - m.ty * depth * 0.5;
      const left = { x: bx - m.ty * halfWidth, y: by + m.tx * halfWidth };
      const right = { x: bx + m.ty * halfWidth, y: by - m.tx * halfWidth };
      return chevronPiece(calibration, left, tip, right, m.tx, m.ty, thickness, back);
    });
  return stripOf(marks, build(0), build(CHEVRON_SHADOW));
}

/**
 * WiFi 的扇形箭頭帶：照遊戲的造型，一條條越來越寬的「＞」從起點攤開到三個終點，
 * 不是三條各自的箭頭帶。尖端在中央那條線上，兩翼跟著兩條側線，再往外多出半個箭頭寬，
 * 所以扇形的外緣和單獨畫側線時一樣寬。三條線共用同一個 u，位置直接取 Rust 的取樣點。
 */
export function chevronFan(
  center: Sample[],
  sideA: Sample[],
  sideB: Sample[],
  calibration: Calibration,
  halfWidth = CHEVRON_HALF_WIDTH,
  depth = CHEVRON_DEPTH,
  pitch = FAN_PITCH,
  thickness = CHEVRON_THICKNESS,
): ChevronStrip {
  const marks = chevronMarks(center, pitch);
  const build = (back: number) => marks.map((m) => {
    const wing = (side: Sample[], sign: number): Point => {
      const p = pointOnSamples(side, m.u);
      const dx = p.x - m.x;
      const dy = p.y - m.y;
      const spread = Math.hypot(dx, dy);
      // 起點附近三條線還疊在一起，退回一般箭頭的寬度與方向。
      if (spread < 1e-6) {
        return {
          x: m.x - m.tx * depth * 0.5 - sign * m.ty * halfWidth,
          y: m.y - m.ty * depth * 0.5 + sign * m.tx * halfWidth,
        };
      }
      const k = (spread + halfWidth) / spread;
      return { x: m.x + dx * k - m.tx * depth * 0.5, y: m.y + dy * k - m.ty * depth * 0.5 };
    };
    const tip = { x: m.x + m.tx * depth * 0.5, y: m.y + m.ty * depth * 0.5 };
    return chevronPiece(calibration, wing(sideA, 1), tip, wing(sideB, -1), m.tx, m.ty, thickness, back);
  });
  return stripOf(marks, build(0), build(CHEVRON_SHADOW));
}

function firstRemaining(strip: ChevronStrip, slideU: number): number {
  let lo = 0;
  let hi = strip.u.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (strip.u[mid] <= slideU) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** 星星還沒走到的箭頭（u > slideU）合成的 path d；走過的箭頭不畫，照遊戲只留前方那段。 */
export function remainingChevrons(strip: ChevronStrip, slideU: number): string {
  return strip.suffix[firstRemaining(strip, slideU)];
}

/**
 * 同一批箭頭墊在底下的那一層（箭頭本身＋往後錯開的陰影），用黑色填滿再加黑色描邊：
 * 每個箭頭因此都有一圈黑外框、後面拖一道黑影。黑底上看不出來，疊在別的軌道上才分得開。
 */
export function remainingChevronShadows(strip: ChevronStrip, slideU: number): string {
  return strip.under[firstRemaining(strip, slideU)];
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

/** 路徑在 u 的前進方向（弧度）；取前後各一小段的弦，彎道上不會抖。 */
export function directionOnSamples(samples: Sample[], u: number): number {
  const a = pointOnSamples(samples, u - 0.02);
  const b = pointOnSamples(samples, u + 0.02);
  return Math.atan2(b.y - a.y, b.x - a.x);
}

/**
 * each 連線：同時的兩顆音符之間一段細圓弧，沿比較短的那一側，半徑跟著音符一起飛（factor）。
 * 照 MajdataPlay 的規則：兩顆在同一個鍵位就不畫；剛好正對面時從後面那顆順時針畫半圈。
 */
export function eachArcPath(calibration: Calibration, a: Point, b: Point, factor: number): string {
  const full = Math.PI * 2;
  const angleA = Math.atan2(a.y, a.x);
  const angleB = Math.atan2(b.y, b.x);
  let sweep = (((angleB - angleA) % full) + full) % full;
  if (sweep < 1e-6 || full - sweep < 1e-6) return '';
  let from = angleA;
  if (sweep > Math.PI - 1e-6) {
    from = angleB;
    sweep = full - sweep;
  }
  const radius = Math.hypot(a.x, a.y) * factor;
  const start = toStage(calibration, { x: Math.cos(from) * radius, y: Math.sin(from) * radius });
  const end = toStage(calibration, {
    x: Math.cos(from + sweep) * radius,
    y: Math.sin(from + sweep) * radius,
  });
  const r = toStageLength(calibration, radius);
  return `M${start.x.toFixed(1)} ${start.y.toFixed(1)}A${r.toFixed(1)} ${r.toFixed(1)} 0 0 1 ${end.x.toFixed(1)} ${end.y.toFixed(1)}`;
}
