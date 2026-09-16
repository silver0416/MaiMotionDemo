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

/** C 在中央、範圍最大，外形畫得比其他區大一些。 */
export function touchRadius(area: string | null | undefined): number {
  return area === 'C' ? 0.125 : 0.105;
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

// ---- maimai 風格的 Touch 音符外形 ----
//
// 實機的 Touch 不是從盤面中心飛出來，而是四片三角形由外往落點「收攏」，
// 收到位的那一刻就是判定時間。Touch Hold 另外有目標方框，
// 剩餘時間畫在方框外面一整圈。
// 以下只描述外形；時間一律由 noteVisual 依絕對播放時間換算，
// 這裡的函式都是純函式，同樣的輸入必得同樣的圖形。

interface PetalSpec {
  /** 收攏到位後，尖端與落點中心的距離（touchRadius 的倍率）；中間要留得下落點編號。 */
  rest: number;
  /** 三角形的長度與底邊半寬。 */
  length: number;
  half: number;
  /** 剛出現時尖端的距離；與 rest 的差就是收攏行程。 */
  spread: number;
}

/** 單點 Touch：正向四片，收攏後幾乎填滿落點外形。 */
const TOUCH_PETAL: PetalSpec = { rest: 0.42, length: 0.56, half: 0.42, spread: 2.6 };
/** Touch Hold：斜向四片，收小一號才進得了方框，行程與單點 Touch 一致。 */
const HOLD_PETAL: PetalSpec = { rest: 0.34, length: 0.5, half: 0.34, spread: 2.6 };

/** Touch Hold 目標方框的頂點距離；要罩得住斜向三角形的底邊兩角。 */
const HOLD_FRAME = 1.26;
/** Touch Hold 剩餘時間環的半徑，畫在方框外面一圈。 */
const HOLD_RING = 1.48;

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/**
 * 收攏中的四片三角形，尖端一律朝落點。
 * gather 0 = 剛出現的最大擴散，1 = 收攏到位（判定時間）。
 * diagonal=true 時四片轉 45°，Touch Hold 走斜向，不靠顏色就能和單點 Touch 分開。
 */
export function touchPetals(
  at: Point,
  radius: number,
  baseAngle: number,
  gather: number,
  diagonal: boolean,
): Point[][] {
  const spec = diagonal ? HOLD_PETAL : TOUCH_PETAL;
  const g = clamp01(gather);
  const tip = radius * (spec.spread + (spec.rest - spec.spread) * g);
  const back = tip + radius * spec.length;
  const half = radius * spec.half;
  const base = baseAngle + (diagonal ? Math.PI / 4 : 0);
  const petals: Point[][] = [];
  for (let i = 0; i < 4; i += 1) {
    const theta = base + (i * Math.PI) / 2;
    const dx = Math.cos(theta);
    const dy = Math.sin(theta);
    petals.push([
      { x: at.x + dx * tip, y: at.y + dy * tip },
      { x: at.x + dx * back - dy * half, y: at.y + dy * back + dx * half },
      { x: at.x + dx * back + dy * half, y: at.y + dy * back - dx * half },
    ]);
  }
  return petals;
}

/** Touch Hold 的目標方框：頂點對齊四片斜向三角形，固定在落點，不隨收攏改變。 */
export function touchHoldFrame(at: Point, radius: number, baseAngle: number): Point[] {
  const corner = radius * HOLD_FRAME;
  const base = baseAngle + Math.PI / 4;
  return Array.from({ length: 4 }, (_, i) => {
    const theta = base + (i * Math.PI) / 2;
    return { x: at.x + Math.cos(theta) * corner, y: at.y + Math.sin(theta) * corner };
  });
}

/** Touch Hold 剩餘時間環的半徑（盤面單位）。 */
export function touchHoldRingRadius(radius: number): number {
  return radius * HOLD_RING;
}

/** 音符外形的外緣半徑，指派標籤靠這個值讓開。 */
export function touchOuterRadius(radius: number, isHold: boolean): number {
  return isHold ? touchHoldRingRadius(radius) : radius;
}

/**
 * 煙火預告虛線圈的半徑。畫成圓形，和落點參考的多邊形分開，
 * 也不會像多邊形那樣切過收攏中的三角形；Touch Hold 要再讓開外圈的剩餘時間環。
 */
export function touchSparkRadius(radius: number, isHold: boolean): number {
  return radius * (isHold ? 1.72 : 1.2);
}
