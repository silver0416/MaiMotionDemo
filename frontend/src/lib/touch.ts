// Touch 區的顯示規則：外形、落點標記與煙火效果。
// 所有座標都取 Rust 輸出的 chart.touchSensors 與 note.position，
// 前端只負責外形與時間換算，不重建感應區的角度或半徑。

import type { Note, Point, TouchSensor } from './types';

export const TOUCH_AREAS = ['A', 'B', 'C', 'D', 'E'] as const;

/** 每個 Touch 區在盤面上的位置說明，與 Rust 幾何一致（A／D 外圈、B／E 內圈、C 中央）。 */
export const TOUCH_AREA_PLACE: Record<string, string> = {
  A: '外圈・鍵位方向',
  B: '內圈・鍵位方向',
  C: '中央',
  D: '外圈・鍵位之間',
  E: '內圈・鍵位之間',
};

/**
 * Touch 落點的顯示名稱。
 * C 區沒有編號，一律顯示 C；simai 的 C1／C2 在核心已合併為同一個 C。
 */
export function touchName(area: string | null | undefined, index: number): string {
  if (!area) return String(index);
  return `${area}${index > 0 ? index : ''}`;
}

/** 落點鍵：note 與 sensor 用同一組鍵對應（C 的 index 為 0）。 */
export function sensorKey(area: string | null | undefined, index: number): string {
  return `${area ?? '?'}${index}`;
}

export function isTouchNote(note: Note): boolean {
  return note.kind === 'touch' || note.kind === 'touchHold';
}

export function isFireworkTouch(note: Note): boolean {
  return isTouchNote(note) && note.modifiers.fireworks;
}

interface ShapeSpec {
  /** 邊數：外圈 A／D 四邊、內圈 B／E 三角、C 八邊 */
  sides: number;
  /** 相對於落點半徑方向的旋轉量 */
  turn: number;
}

// 同一家族的多邊形，只用邊數與朝向區分區域，不新增顏色。
const SHAPES: Record<string, ShapeSpec> = {
  A: { sides: 4, turn: 0 },
  B: { sides: 3, turn: 0 },
  C: { sides: 8, turn: Math.PI / 8 },
  D: { sides: 4, turn: Math.PI / 4 },
  E: { sides: 3, turn: Math.PI },
};

/**
 * Touch 音符外形的朝向。實機的 Touch 圖示一律是正的，
 * 不會跟著落點轉去面向圓心，所以三角形與方框都用這個固定角度。
 * 落點「參考標記」仍用 radialAngle，靠朝向區分 B 與 E、A 與 D。
 */
export const TOUCH_ICON_ANGLE = -Math.PI / 2;

/** 由盤面中心指向落點的角度；C 在中心，固定朝上讓八邊形方向穩定。 */
export function radialAngle(position: Point): number {
  if (Math.hypot(position.x, position.y) < 1e-9) return -Math.PI / 2;
  return Math.atan2(position.y, position.x);
}

/** 落點外形的盤面座標頂點，供 polylinePath 轉成畫面路徑。 */
export function touchPolygon(
  area: string | null | undefined,
  at: Point,
  radius: number,
  angle: number,
): Point[] {
  const spec = SHAPES[area ?? ''] ?? SHAPES.A;
  const base = angle + spec.turn;
  const points: Point[] = [];
  for (let i = 0; i < spec.sides; i += 1) {
    const theta = base + (i * 2 * Math.PI) / spec.sides;
    points.push({ x: at.x + Math.cos(theta) * radius, y: at.y + Math.sin(theta) * radius });
  }
  return points;
}

/**
 * Touch 音符的大小：收攏到位時，落點中心到三角形底邊的距離（盤面單位）。
 * 照遊戲，每一區都一樣大：三角形的高是 0.5 個遊戲單位（量自遊戲錄影，判定線半徑是 4.8）。
 * 參數保留是因為呼叫端都用落點的區域來問。
 */
export function touchRadius(_area: string | null | undefined): number {
  return 0.5 / 4.8;
}

/** 落點參考標記的大小（盤面單位）：畫得比音符小一級，音符落下時仍是畫面上最亮的一層。 */
export function sensorMarkRadius(area: string): number {
  if (area === 'C') return 0.085;
  if (area === 'B' || area === 'E') return 0.068;
  return 0.075;
}

/** 依區域分組的落點，畫圖例與清單說明時共用。 */
export function sensorsByArea(sensors: TouchSensor[]): { area: string; items: TouchSensor[] }[] {
  return TOUCH_AREAS.map((area) => ({
    area: area as string,
    items: sensors.filter((sensor) => sensor.area === area),
  })).filter((group) => group.items.length > 0);
}

// ---- 煙火 ----

/** 煙火全長；效果只由絕對播放時間換算，拖曳到同一時間得到同一畫面。 */
export const FIREWORK_SECONDS = 0.6;

/** 火花線段數量。 */
export const FIREWORK_SPARKS = 8;

export interface FireworkBurst {
  note: Note;
  /** 0–1，等於 (播放時間 − 判定時間) / FIREWORK_SECONDS */
  progress: number;
}

/**
 * 目前該畫出來的煙火。
 * 只看起始判定時間，Touch Hold 結束不重播；同一時間有多顆就回傳多筆。
 */
export function fireworkBursts(notes: Note[], time: number): FireworkBurst[] {
  const out: FireworkBurst[] = [];
  for (const note of notes) {
    if (!isFireworkTouch(note)) continue;
    const age = time - note.timeSeconds;
    if (age < 0 || age > FIREWORK_SECONDS) continue;
    out.push({ note, progress: age / FIREWORK_SECONDS });
  }
  return out;
}

export interface FireworkShape {
  /** 擴散環半徑（盤面單位） */
  ringRadius: number;
  ringWidth: number;
  /** 火花線段的起訖半徑 */
  sparkInner: number;
  sparkOuter: number;
  /** 判定瞬間的亮點半徑，過了立刻為 0 */
  coreRadius: number;
  /** 0–1 的殘留強度，用於筆觸淡出 */
  fade: number;
}

/** 煙火幾何：輸入 0–1 的進度，輸出當下的半徑與強度。純函式，同進度同結果。 */
export function fireworkShape(progress: number): FireworkShape {
  const p = Math.min(1, Math.max(0, progress));
  const ease = 1 - (1 - p) * (1 - p);
  const fade = (1 - p) * (1 - p);
  const sparkInner = 0.055 + 0.2 * ease;
  return {
    ringRadius: 0.05 + 0.26 * ease,
    ringWidth: 5 - 3.4 * ease,
    sparkInner,
    sparkOuter: sparkInner + 0.075 * (1 - 0.65 * ease),
    coreRadius: p < 0.3 ? 0.075 * (1 - p / 0.3) : 0,
    fade,
  };
}

/** 火花方向：固定 8 向並偏 22.5°，避免與鍵位連線重疊。 */
export function sparkAngles(): number[] {
  const step = (Math.PI * 2) / FIREWORK_SPARKS;
  return Array.from({ length: FIREWORK_SPARKS }, (_, i) => i * step + step / 2);
}

// ---- 照遊戲的 Touch 音符外形 ----
//
// 尺寸量自使用者的遊戲錄影（2026-11-23），動作照 MajdataPlay 的 TouchDrop／TouchHoldDrop。
// 四片圓角的直角三角形，直角那個尖端朝落點，由外往落點「收攏」；收到位時四片剛好拼成一整塊
// （Touch 是正方形、Touch Hold 是菱形），四個尖端在落點相碰，那一刻就是判定時間。
// Touch 四片在上下左右；Touch Hold 四片在四個斜角，按住之後外面多一圈粗的圓角菱形。
// 以下只描述外形，全部以 size（touchRadius，三角形的高）為基準等比例縮放；
// 時間一律由 noteVisual 依絕對播放時間換算，這裡都是純函式。

/** 三角形底邊的半寬，是高（size）的幾倍：直角三角形，所以是 1。 */
const FAN_HALF_BASE = 1;
/** 剛出現時尖端離落點多遠，是 size 的幾倍（遊戲是 0.3 個單位）；收攏到位是 0。 */
const FAN_START = 0.3 / 0.5;
/** 三角形三個角的圓角半徑，是 size 的幾倍。 */
const FAN_CORNER = 0.1;

/** Touch 判定方框（收到位那一刻亮起，比拼起來的正方形大一點）的半邊長與圓角半徑，都是 size 的倍數。 */
export const TOUCH_JUST_HALF = 0.57 / 0.5;
export const TOUCH_JUST_CORNER = 0.14 / 0.5;

/**
 * Touch Hold 外框：粗的圓角菱形，內緣貼著四片三角形拼成的菱形。
 * 筆觸中心線的邊到落點中心的距離、圓角半徑、筆寬，都是 size 的倍數。
 */
const HOLD_BORDER_SIDE = 0.64 / 0.5;
const HOLD_BORDER_CORNER = 0.2 / 0.5;
export const TOUCH_HOLD_BAND = 0.27 / 0.5;

/** 落點正中心的圓點（盤面單位）：遊戲裡是一顆帶白邊的實心圓。這是裡面那顆的半徑。 */
export const TOUCH_DOT = 0.085 / 4.8;
/** 圓點外面那圈白邊的寬度（盤面單位）。 */
export const TOUCH_DOT_RING = 0.04 / 4.8;

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** 三角形中間挖掉的小三角形，是整片的幾倍（對內心縮放）。 */
export const TOUCH_FAN_HOLE = 0.36;

/**
 * 一片三角形：尖端朝落點、底邊在外。只記它在哪個方向、離落點多遠，
 * 實際頂點由 fanLoop 換算；整片對「內心」縮放就是三條邊同時平移，
 * 白邊、挖空的小三角形都靠這個做，寬度處處相同。
 */
export interface TouchFan {
  /** 這一片在落點的哪個方向（弧度，x 向右、y 向下） */
  theta: number;
  /** 尖端、底邊離落點的距離（盤面單位） */
  tip: number;
  back: number;
  /** 底邊的半寬 */
  half: number;
  /** 內心離落點的距離，與內切圓半徑 */
  center: number;
  inradius: number;
}

/**
 * 四片三角形。gather 0 = 剛出現的位置，1 = 收攏到位（判定時間，四個尖端在落點相碰）。
 * diagonal=false 是 Touch：依序上、右、下、左。
 * diagonal=true 是 Touch Hold：依序右上、右下、左下、左上（遊戲的紅、黃、綠、藍）。
 */
export function touchFans(size: number, gather: number, diagonal: boolean): TouchFan[] {
  const tip = size * FAN_START * (1 - clamp01(gather));
  const back = tip + size;
  const half = size * FAN_HALF_BASE;
  const inradius = (half * size) / (half + Math.hypot(half, size));
  const first = diagonal ? -Math.PI / 4 : -Math.PI / 2;
  return Array.from({ length: 4 }, (_, i) => ({
    theta: first + (i * Math.PI) / 2,
    tip,
    back,
    half,
    center: back - inradius,
    inradius,
  }));
}

function fanPoint(at: Point, fan: TouchFan, along: number, across: number): Point {
  const dx = Math.cos(fan.theta);
  const dy = Math.sin(fan.theta);
  return { x: at.x + dx * along - dy * across, y: at.y + dy * along + dx * across };
}

/** 對內心縮放後的尖端、底邊與半寬。 */
function fanScaled(fan: TouchFan, scale: number): { tip: number; back: number; half: number } {
  return {
    tip: fan.center - (fan.center - fan.tip) * scale,
    back: fan.center + fan.inradius * scale,
    half: fan.half * scale,
  };
}

/**
 * 把多邊形的每個角換成一小段圓弧（用二次曲線取幾個點近似），回傳新的一圈頂點。
 * 兩條邊各退 radius ÷ tan(半個內角)，邊太短就退到邊長的 45% 為止。
 */
function roundCorners(points: Point[], radius: number): Point[] {
  if (radius <= 0) return points;
  const out: Point[] = [];
  const count = points.length;
  for (let i = 0; i < count; i += 1) {
    const v = points[i];
    const p = points[(i + count - 1) % count];
    const n = points[(i + 1) % count];
    const lp = Math.hypot(p.x - v.x, p.y - v.y) || 1;
    const ln = Math.hypot(n.x - v.x, n.y - v.y) || 1;
    const ux = (p.x - v.x) / lp;
    const uy = (p.y - v.y) / lp;
    const wx = (n.x - v.x) / ln;
    const wy = (n.y - v.y) / ln;
    const half = Math.acos(Math.min(1, Math.max(-1, ux * wx + uy * wy))) / 2;
    const back = Math.min(radius / Math.tan(half), Math.min(lp, ln) * 0.45);
    const a = { x: v.x + ux * back, y: v.y + uy * back };
    const b = { x: v.x + wx * back, y: v.y + wy * back };
    for (let k = 0; k <= ROUND_STEPS; k += 1) {
      const t = k / ROUND_STEPS;
      const s = 1 - t;
      out.push({
        x: s * s * a.x + 2 * s * t * v.x + t * t * b.x,
        y: s * s * a.y + 2 * s * t * v.y + t * t * b.y,
      });
    }
  }
  return out;
}

/** 三角形每個圓角用幾段折線。 */
const ROUND_STEPS = 3;

/**
 * 一片三角形的輪廓（圓角，一圈頂點）。scale 對內心縮放：>1 往外一圈、<1 往內一圈；
 * 圓角跟著等比例縮放，所以內外兩圈的間距處處相同。
 */
export function fanLoop(at: Point, fan: TouchFan, scale = 1): Point[] {
  const f = fanScaled(fan, scale);
  return roundCorners(
    [fanPoint(at, fan, f.tip, 0), fanPoint(at, fan, f.back, f.half), fanPoint(at, fan, f.back, -f.half)],
    (fan.back - fan.tip) * FAN_CORNER * scale,
  );
}

/** 每個圓角用幾段折線近似。 */
const CORNER_STEPS = 5;

/**
 * Touch Hold 的圓角菱形外框（筆觸中心線，筆寬是 size × TOUCH_HOLD_BAND），切成四段折線，
 * 從正上方順時針：右上、右下、左下、左上（和 touchFans 斜向四片同一個順序，各自對應一片）。
 * 每一段從一個頂點的圓角中點，沿邊走到下一個頂點的圓角中點；按住的進度照這個順序一段一段亮起。
 */
export function touchHoldBorder(at: Point, size: number): Point[][] {
  const corner = size * HOLD_BORDER_CORNER;
  // 圓角的圓心：尖頂點（邊距 × √2）往落點退 corner × √2。
  const center = size * HOLD_BORDER_SIDE * Math.SQRT2 - corner * Math.SQRT2;
  const arc = (vertex: number, from: number, to: number): Point[] => {
    const cx = at.x + Math.cos(vertex) * center;
    const cy = at.y + Math.sin(vertex) * center;
    const points: Point[] = [];
    for (let i = 0; i <= CORNER_STEPS; i += 1) {
      const theta = vertex + from + ((to - from) * i) / CORNER_STEPS;
      points.push({ x: cx + Math.cos(theta) * corner, y: cy + Math.sin(theta) * corner });
    }
    return points;
  };
  const quarters: Point[][] = [];
  for (let k = 0; k < 4; k += 1) {
    const vertex = -Math.PI / 2 + (k * Math.PI) / 2;
    quarters.push([
      ...arc(vertex, 0, Math.PI / 4),
      ...arc(vertex + Math.PI / 2, -Math.PI / 4, 0),
    ]);
  }
  return quarters;
}

/** Touch 判定方框：圓角正方形，收到位那一刻亮起。回傳一圈頂點（筆觸中心線）。 */
export function touchJustBorder(at: Point, size: number): Point[] {
  const half = size * TOUCH_JUST_HALF;
  const corner = size * TOUCH_JUST_CORNER;
  const points: Point[] = [];
  for (let k = 0; k < 4; k += 1) {
    // 四個角依序：右上、右下、左下、左上；每個角是四分之一圓。
    const sx = k === 0 || k === 1 ? 1 : -1;
    const sy = k === 1 || k === 2 ? 1 : -1;
    const cx = at.x + sx * (half - corner);
    const cy = at.y + sy * (half - corner);
    const from = -Math.PI / 2 + (k * Math.PI) / 2;
    for (let i = 0; i <= CORNER_STEPS; i += 1) {
      const theta = from + ((Math.PI / 2) * i) / CORNER_STEPS;
      points.push({ x: cx + Math.cos(theta) * corner, y: cy + Math.sin(theta) * corner });
    }
  }
  return points;
}

/** Touch Hold 外框第 index 段（從正上方順時針）亮到哪裡：0–1。progress 是按住的完成比例。 */
export function touchHoldBorderFill(progress: number, index: number): number {
  return clamp01(progress * 4 - index);
}

/** Touch Hold 外框頂點離落點多遠（到外緣）。 */
export function touchHoldOuter(size: number): number {
  return (
    size * (HOLD_BORDER_SIDE * Math.SQRT2 - HOLD_BORDER_CORNER * (Math.SQRT2 - 1)) +
    (size * TOUCH_HOLD_BAND) / 2
  );
}

/** 音符外形的外緣半徑，指派標籤靠這個值讓開。Touch 拼起來是正方形，取邊與角之間。 */
export function touchOuterRadius(radius: number, isHold: boolean): number {
  return isHold ? touchHoldOuter(radius) : radius * 1.2;
}

/**
 * 煙火預告虛線圈的半徑。畫成圓形，和落點參考的多邊形分開；
 * 要罩得住收攏到位的四片三角形，Touch Hold 再讓開外面的菱形外框。
 */
export function touchSparkRadius(radius: number, isHold: boolean): number {
  return radius * (isHold ? 2.08 : 1.6);
}
