// 打擊效果：音符判定的那一刻，在原地放出的光暈。照遊戲的樣子做：
//   - 一般音符是「六角形」：正中間一個六角形越放越大；旁邊四個六角形一開始兩兩靠在音符的左右兩側，
//     兩個順時針、兩個逆時針繞出去，散成十字，轉到音符的內外兩側再兩兩碰頭，同時慢慢往中間靠。
//     到淡掉為止四個都還沒收到正中間：結尾是內外兩側各一組、和中間的大六角形錯開——不會疊成同一個。
//   - 橘色的 Break 音符同一套動作，形狀換成星星。
//   - Touch 是一個實心圓：從小放大，一邊放大一邊淡出。
//   - Hold／Touch Hold 按下去的那一刻沒有這個效果，放開（結束時間）才放一次。
//     按住的那段時間另有一種：圓形的光圈一圈一圈從音符頭往外擴（檔案最後一段）。
// 數字的來源分兩種：
//   - 正中間那個的放大、淡出曲線的形狀、Touch 的實心圓：MajdataPlay tapEffect／breakEffect／touchPerfect
//     動畫檔的關鍵格與切線原樣搬過來（Hermite 曲線，所以有加減速，不是折線）。拿使用者的遊戲錄影
//     （2026-11-23）逐格比對，正中間那個的大小和動畫檔一致；淡出的形狀一樣，但錄影裡晚了一點才開始。
//   - 旁邊四個怎麼轉、怎麼靠攏、多大：動畫檔裡的曲線和錄影對不上（錄影裡一開始就轉得快、後面慢下來，
//     到最後都沒收到中間；動畫檔是越轉越快、最後收成一點）。所以這三條改用錄影量到的：
//     把五個圖形的外框逐格擬合到錄影的光暈上，中央、6 號鍵、4 號鍵（Break）三處量到同一條軌跡。
//     圖形的大小（SPRITE_RADIUS）與星星的朝向也是量的。
// 只由絕對播放時間換算，沒有自己的計時器：拖曳到同一時間得到同一畫面，慢放與循環也自動正確。
// 純粹是畫面效果，不代表判定結果，也不影響指派。

import { toStage, toStageLength, type Calibration } from './disc';
import type { Note, Point } from './types';

/** 效果全長（秒）。 */
export const HIT_SECONDS = 0.5;
/** Touch 的實心圓比較短。 */
const DISC_SECONDS = 0.3167;

/** 六角形（一般音符）、星星（Break）、實心圓（Touch）。 */
export type HitShape = 'hex' | 'star' | 'disc';

export interface Hit {
  key: string;
  at: Point;
  shape: HitShape;
  /** 判定之後過了幾秒 */
  age: number;
  /** 效果朝哪個方向擺：音符在盤面上的方位（從盤心看出去的角度，弧度，y 向下） */
  angle: number;
}

/** 1 個遊戲單位是盤面半徑的 1 ÷ 4.8；下面的尺寸都先用遊戲單位寫，再換成盤面單位。 */
const U = 1 / 4.8;

/** 盤面正中央沒有「方位」可言，遊戲裡是固定這樣擺（六角形尖端朝正上正下）。 */
const CENTER_ANGLE = (-30 * Math.PI) / 180;

function shapeOf(note: Note): HitShape {
  if (note.kind === 'touch') return 'disc';
  return note.modifiers.breakNote ? 'star' : 'hex';
}

function frameAngle(position: Point): number {
  if (Math.hypot(position.x, position.y) < 1e-3) return CENTER_ANGLE;
  return Math.atan2(position.y, position.x);
}

/**
 * 目前該畫出來的打擊效果。
 * Tap、Slide 起點、Touch 在判定時間放一次；Hold 與 Touch Hold 只在放開（結束時間）時放。
 * 沒有起點的 Slide（`?`、`!`、`*` 的第二條之後）沒有東西可以打，不放。
 */
export function hitEffects(notes: Note[], time: number): Hit[] {
  const out: Hit[] = [];
  for (const note of notes) {
    if (note.kind === 'slide' && !note.hasHead) continue;
    const held = note.kind === 'hold' || note.kind === 'touchHold';
    const at = held ? Math.max(note.endSeconds, note.timeSeconds) : note.timeSeconds;
    const age = time - at;
    const shape = shapeOf(note);
    if (age < 0 || age > (shape === 'disc' ? DISC_SECONDS : HIT_SECONDS)) continue;
    out.push({ key: note.id, at: note.position, shape, age, angle: frameAngle(note.position) });
  }
  return out;
}

// ---- 動畫曲線 ----

/** 一個關鍵格：時間、數值、進來與出去的斜率（每秒）。和 Unity 動畫檔的寫法一樣。 */
type Key = [time: number, value: number, slopeIn: number, slopeOut: number];

/** Hermite 內插（Unity AnimationCurve 沒有加權切線時的算法），範圍外停在頭尾的值。 */
function sample(keys: Key[], at: number): number {
  if (at <= keys[0][0]) return keys[0][1];
  for (let i = 1; i < keys.length; i += 1) {
    const [t1, v1, in1] = keys[i];
    if (at > t1) continue;
    const [t0, v0, , out0] = keys[i - 1];
    const span = t1 - t0;
    const s = (at - t0) / span;
    const s2 = s * s;
    const s3 = s2 * s;
    return (
      (2 * s3 - 3 * s2 + 1) * v0 +
      (s3 - 2 * s2 + s) * out0 * span +
      (-2 * s3 + 3 * s2) * v1 +
      (s3 - s2) * in1 * span
    );
  }
  return keys[keys.length - 1][1];
}

// 以下兩條是 tapEffect.anim 的關鍵格（breakEffect.anim 的數字一模一樣，只差在圖）。

/** 整組的放大倍率：很快衝到 0.7，再慢慢放到 1.3。 */
const ROOT_SCALE: Key[] = [
  [0, 0, 0, 0],
  [0.083333, 0.7, 2.888889, 2.888889],
  [0.45, 1.3, 0, 0],
];
/** 不透明度：前半段全亮，後面淡出。 */
const FADE: Key[] = [
  [0, 1, 0, 0],
  [0.233333, 0.995328, -0.044851, -0.044851],
  [0.416667, 0, 0, 0],
];

// 以下是錄影量到的（時間都從判定那一刻起算）。

/** 繞行轉了幾度：一開始就轉得快（每秒約 420°），過了 0.25 秒慢下來，停在轉了約半圈少一點的地方。 */
const ORBIT_TURN: Key[] = [
  [0, 16, 420, 420],
  [0.25, 121, 420, 400],
  [0.45, 165, 80, 80],
];
/** 四個圖形離正中間的距離，是起點距離（再乘上整組放大倍率）的幾倍：一路慢慢靠近，到淡掉還剩一點。 */
const ORBIT_REACH: Key[] = [
  [0, 1.05, 0, 0],
  [0.085, 1.05, 0, -4],
  [0.175, 0.7, -2.8, -2.8],
  [0.32, 0.38, -1.9, -1.9],
  [0.45, 0.13, -2, -2],
];
/** 淡出比動畫檔晚這麼久（量錄影裡光暈的亮度：0.37 秒還有八成半，0.43 秒剩四成，0.47 秒幾乎沒了）。 */
const FADE_LAG = 0.085;
/** 剛出現時比較淡，約 0.2 秒內亮起來（錄影裡頭幾格的亮度是全亮時的兩成到六成）。 */
const FADE_IN_START = 0.02;
const FADE_IN_END = 0.22;
const FADE_IN_FROM = 0.1;
/** 四個圖形的大小是正中間那個的幾倍：兩個大一點、兩個小一點，從頭到尾不變。 */
const SIZE_BIG = 0.79;
const SIZE_SMALL = 0.7;

/** 繞行的四個圖形：起點（動畫檔裡的座標，y 向上）、往哪邊轉、多大。 */
const ORBITERS: { x: number; y: number; turn: 1 | -1; size: number }[] = [
  { x: 0.552, y: 0.5, turn: 1, size: SIZE_BIG },
  { x: -0.431, y: -0.527, turn: 1, size: SIZE_BIG },
  { x: 0.473, y: -0.468, turn: -1, size: SIZE_SMALL },
  { x: -0.502, y: 0.492, turn: -1, size: SIZE_SMALL },
];

/** 倍率 1 時六角形線條中心的外接圓半徑（遊戲單位）。 */
const SPRITE_RADIUS = 0.58;
/** 星星：尖端比六角形的頂點稍微遠一點，內凹頂點是尖端的一半。 */
const STAR_TIP = 1.014;
const STAR_INNER = 0.5;
/** 線條粗細是外接圓半徑的幾倍（遊戲的圖是整張一起放大，線跟著變粗），以及上下限（像素）。 */
const LINE_RATIO = 0.08;
const LINE_MIN_PX = 1.5;
const LINE_MAX_PX = 8;
/** 光暈的模糊半徑是正中間那個圖形半徑的幾倍：裡面那條柔邊的亮線、外面那圈寬而淡的光。 */
const GLOW_SOFT_RATIO = 0.03;
const GLOW_WIDE_RATIO = 0.09;

// ---- 實心圓（Touch）：touchPerfect.anim 的 TouchCircle，圖的半徑約 2.57 個遊戲單位 ----

const DISC_SPRITE_RADIUS = 2.57;
const DISC_SCALE: Key[] = [
  [0, 0.1, 0, 0],
  [0.166667, 0.3, 0.6316, 0.6316],
  [DISC_SECONDS, 0.35, 0, 0],
];
const DISC_FADE: Key[] = [
  [0, 1, 0, 0],
  [DISC_SECONDS, 0, 0, 0],
];

/** 整組的不透明度。六角形／星星：約 0.2 秒內亮起來，最後淡出；實心圓：一邊放大一邊淡掉。 */
export function hitOpacity(hit: Hit): number {
  if (hit.shape === 'disc') return Math.max(0, sample(DISC_FADE, hit.age));
  const rise = Math.min(1, Math.max(0, (hit.age - FADE_IN_START) / (FADE_IN_END - FADE_IN_START)));
  const fadeIn = FADE_IN_FROM + (1 - FADE_IN_FROM) * rise * rise * (3 - 2 * rise);
  return Math.max(0, fadeIn * sample(FADE, hit.age - FADE_LAG));
}

/** 效果裡的一個圖形：相對於音符位置的偏移、外接圓半徑（都是盤面單位）。 */
export interface HitRing {
  dx: number;
  dy: number;
  radius: number;
}

/**
 * 六角形／星星效果在某一刻的樣子。純函式，同一個 age、同一個方位得到同一個結果。
 * 第一個是正中間那個，後面四個是繞行的。
 * 座標的擺法照遊戲：效果自己的 y 軸朝盤外、x 軸沿著切線，整組再轉 90°。
 */
export function hitRings(age: number, angle: number): HitRing[] {
  const scale = sample(ROOT_SCALE, age);
  const rings: HitRing[] = [{ dx: 0, dy: 0, radius: SPRITE_RADIUS * scale * U }];
  const turned = (sample(ORBIT_TURN, age) * Math.PI) / 180;
  const reach = sample(ORBIT_REACH, age) * scale;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  for (const orbiter of ORBITERS) {
    const theta = orbiter.turn * turned;
    // 繞行：起點座標轉 theta（動畫檔的座標，y 向上）。
    const px = (orbiter.x * Math.cos(theta) - orbiter.y * Math.sin(theta)) * reach;
    const py = (orbiter.x * Math.sin(theta) + orbiter.y * Math.cos(theta)) * reach;
    // 整組轉 90°：(x, y) → (-y, x)。之後 x 沿切線、y 朝盤外，換成畫面座標（y 向下）。
    const along = -py;
    const outward = px;
    rings.push({
      dx: (-along * sin + outward * cos) * U,
      dy: (along * cos + outward * sin) * U,
      radius: SPRITE_RADIUS * scale * orbiter.size * U,
    });
  }
  return rings;
}

/** 畫面上的一筆：路徑、線寬（像素）、是不是實心。 */
export interface HitStroke {
  d: string;
  width: number;
  filled: boolean;
}

function circlePath(c: Point, r: number): string {
  const x0 = (c.x + r).toFixed(2);
  const x1 = (c.x - r).toFixed(2);
  const y = c.y.toFixed(2);
  const rr = r.toFixed(2);
  return `M${x0} ${y}A${rr} ${rr} 0 1 1 ${x1} ${y}A${rr} ${rr} 0 1 1 ${x0} ${y}Z`;
}

/**
 * 六角形或星星的一圈。
 * 六角形：一個頂點朝盤外（另一個朝盤心）。
 * 星星：一個尖端沿著切線（錄影裡量到的；和六角形差 90°，是那張圖本來的朝向）。
 */
function outline(c: Point, r: number, shape: HitShape, angle: number): string {
  const star = shape === 'star';
  const corners = star ? 10 : 6;
  const first = star ? angle + Math.PI / 2 : angle;
  let d = '';
  for (let i = 0; i < corners; i += 1) {
    const reach = star ? r * (i % 2 === 1 ? STAR_INNER : STAR_TIP) : r;
    const theta = first + (i * 2 * Math.PI) / corners;
    d += `${i === 0 ? 'M' : 'L'}${(c.x + Math.cos(theta) * reach).toFixed(2)} ${(c.y + Math.sin(theta) * reach).toFixed(2)}`;
  }
  return `${d}Z`;
}

/** 這一刻最大的那個圖形有多大（盤面單位）：光暈的模糊半徑跟著它放大，像遊戲的圖整張一起放大。 */
function hitSize(hit: Hit): number {
  if (hit.shape === 'disc') return DISC_SPRITE_RADIUS * sample(DISC_SCALE, hit.age) * U;
  return SPRITE_RADIUS * sample(ROOT_SCALE, hit.age) * U;
}

/** 光暈的兩個模糊半徑（畫面像素）。 */
export function hitGlow(calibration: Calibration, hit: Hit): { soft: number; wide: number } {
  const r = toStageLength(calibration, hitSize(hit));
  return { soft: r * GLOW_SOFT_RATIO, wide: r * GLOW_WIDE_RATIO };
}

/** 一個效果這一刻要畫的每一筆（畫面座標）。 */
export function hitStrokes(calibration: Calibration, hit: Hit): HitStroke[] {
  if (hit.shape === 'disc') {
    const r = toStageLength(calibration, DISC_SPRITE_RADIUS * sample(DISC_SCALE, hit.age) * U);
    return [{ d: circlePath(toStage(calibration, hit.at), r), width: 0, filled: true }];
  }
  const out: HitStroke[] = [];
  for (const ring of hitRings(hit.age, hit.angle)) {
    const r = toStageLength(calibration, ring.radius);
    if (r <= 0.5) continue;
    const c = toStage(calibration, { x: hit.at.x + ring.dx, y: hit.at.y + ring.dy });
    out.push({
      d: outline(c, r, hit.shape, hit.angle),
      width: Math.min(LINE_MAX_PX, Math.max(LINE_MIN_PX, r * LINE_RATIO)),
      filled: false,
    });
  }
  return out;
}

// ---- 按住 Hold／Touch Hold 的期間：一圈一圈往外擴的圓 ----
// 遊戲裡按住的這段時間，音符頭（Touch Hold 是它所在的位置）會不停冒出圓形的光圈往外擴，放開就整個消失。
// 數字全部是錄影量到的（2026-11-23 的兩段錄影：6 號與 4 號鍵的 Hold、盤面中央的 Touch Hold，三處同一組）：
// 把靜止的畫面減掉之後，逐格量每一圈的半徑、亮度與線的剖面。
// MajdataPlay 的 Hold_Effect.prefab 是粒子系統（每秒 10 顆、每顆活 0.3 秒、大小先快後慢），
// 照它算出來的節奏和大小都和錄影對不上，所以沒有用它的數字。

/** 節拍：每 1/15 秒一拍，四拍一輪；前三拍各冒一圈，第四拍空著。從按下去那一刻起算。 */
const LOOP_BEAT = 1 / 15;
const LOOP_BEATS = 4;
const LOOP_EMITS = 3;
/** 一圈從冒出來到完全淡掉（秒）。亮度是直線往下掉，一冒出來就是最亮。 */
const LOOP_LIFE = 0.188;
/** 圈的半徑（遊戲單位）：冒出來時多大、每秒擴多少（等速）。 */
const LOOP_RADIUS = 0.65;
const LOOP_SPEED = 3.45;
/**
 * 線與光暈都跟著半徑放大：線寬、兩個模糊半徑各是半徑的幾倍。
 * 擬合錄影裡圈的剖面（半高寬約半徑的 0.17 倍，正中間幾乎全亮）得到的；線本身不透明（`.hit-ring`）。
 */
const LOOP_LINE_RATIO = 0.1;
const LOOP_SOFT_RATIO = 0.03;
const LOOP_WIDE_RATIO = 0.1;

/** 按住時的一圈光圈（畫面座標）：路徑、線寬、不透明度、兩個模糊半徑。 */
export interface LoopRing {
  key: string;
  d: string;
  width: number;
  opacity: number;
  soft: number;
  wide: number;
}

/**
 * 這一刻正被按住的 Hold／Touch Hold 身上的光圈。
 * 只由絕對播放時間換算：第幾拍冒的圈現在多大、多亮都是算出來的，拖曳到同一時間得到同一畫面。
 * 按下去之前的拍子也算進來（遊戲的效果一啟動就已經在循環中），所以按下去的第一格就有一圈比較大、比較淡的。
 */
export function holdLoopRings(calibration: Calibration, notes: Note[], time: number): LoopRing[] {
  const out: LoopRing[] = [];
  for (const note of notes) {
    if (note.kind !== 'hold' && note.kind !== 'touchHold') continue;
    const held = time - note.timeSeconds;
    if (held < 0 || time >= note.endSeconds) continue;
    const centre = toStage(calibration, note.position);
    for (let beat = Math.floor(held / LOOP_BEAT); held - beat * LOOP_BEAT < LOOP_LIFE; beat -= 1) {
      const slot = ((beat % LOOP_BEATS) + LOOP_BEATS) % LOOP_BEATS;
      if (slot >= LOOP_EMITS) continue;
      const age = held - beat * LOOP_BEAT;
      const r = toStageLength(calibration, (LOOP_RADIUS + LOOP_SPEED * age) * U);
      out.push({
        key: `${note.id}:${beat}`,
        d: circlePath(centre, r),
        width: Math.max(LINE_MIN_PX, r * LOOP_LINE_RATIO),
        opacity: 1 - age / LOOP_LIFE,
        soft: r * LOOP_SOFT_RATIO,
        wide: r * LOOP_WIDE_RATIO,
      });
    }
  }
  return out;
}
