// 哪一組是玩家的左手。姿態模型靠肩膀與手臂分左右，機位拍不到肩膀時它的左右標籤幾乎是亂的，
// 所以把每格偵測到的兩組手當成「身分未定」，用兩個狀態（不對調／對調）的 Viterbi 決定每格的左右：
// - 模型標籤：依這格肩、肘的可見度加權，看得到肩肘時才信
// - 盤面順序：左手通常在右手左邊（盤面座標 x 較小）
// - 連續性：手不會瞬間跳到很遠；越久沒看到越不計較
// 兩組疊在同一隻手上時只留分數高的那組。參數由資料集離線評估訂定（Demo 假設）。

import type { HandSample } from './assign';
import type { Vec2 } from './homography';

/** 兩組相距小於這個值（盤面半徑）視為疊在同一隻手 */
const COLLAPSE = 0.25;
/** 每格位移代價的上限（盤面半徑） */
const MOVE_CAP = 1.5;
/** 多久沒看到後開始打折（秒） */
const GAP = 0.2;
/** 左手在右手右邊時，每單位距離的代價 */
const ORDER_WEIGHT = 1.5;
/** 違反模型左右標籤的代價（乘上肩肘可見度） */
const MODEL_WEIGHT = 1;

export interface IdentityFrame {
  t: number;
  /** 模型標為左手的那組（盤面座標；沒偵測到為 null） */
  a: HandSample | null;
  /** 模型標為右手的那組 */
  b: HandSample | null;
  /** 這格肩、肘的可見度 0–1 */
  body: number;
}

export interface Assigned {
  L: HandSample | null;
  R: HandSample | null;
}

/** 肩肘關鍵點分數（取最高者）換成可見度 0–1。 */
export function bodyVisibility(score: number): number {
  return Math.min(1, Math.max(0, (score - 0.3) / 0.3));
}

interface Memory {
  l: Vec2 | null;
  tl: number;
  r: Vec2 | null;
  tr: number;
}

function distance(p: Vec2, q: Vec2): number {
  return Math.hypot(p.x - q.x, p.y - q.y);
}

function move(p: Vec2 | null, last: Vec2 | null, since: number, now: number, step: number): number {
  if (!p || !last) return 0;
  const dt = Math.max(now - since, step);
  return Math.min(distance(p, last), MOVE_CAP) / Math.max(1, dt / GAP);
}

/** 疊在同一隻手的兩組只留分數高的。 */
function dedupe(frame: IdentityFrame): [HandSample | null, HandSample | null] {
  const { a, b } = frame;
  if (a && b && distance(a.palm, b.palm) < COLLAPSE) return a.score >= b.score ? [a, null] : [null, b];
  return [a, b];
}

/** 決定每格的左右手。mirror：影片是左右翻轉的（模型看到的左右要對調）；step：取樣間隔（秒）。 */
export function assignHands(frames: IdentityFrame[], mirror: boolean, step: number): Assigned[] {
  const n = frames.length;
  if (n === 0) return [];
  const hands = frames.map(dedupe);
  // 狀態 0：a 組是左手；1：對調
  const preferred = mirror ? 1 : 0;
  const back = new Uint8Array(n * 2);
  let cost = [0, 0];
  let memory: Memory[] = [
    { l: null, tl: 0, r: null, tr: 0 },
    { l: null, tl: 0, r: null, tr: 0 },
  ];
  for (let k = 0; k < n; k += 1) {
    const frame = frames[k];
    const [a, b] = hands[k];
    const nextCost = [Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY];
    const nextMemory: Memory[] = [memory[0], memory[1]];
    for (let s = 0; s < 2; s += 1) {
      const left = s === 0 ? a : b;
      const right = s === 0 ? b : a;
      let unary = 0;
      if ((a || b) && s !== preferred) unary += MODEL_WEIGHT * frame.body;
      if (left && right) unary += ORDER_WEIGHT * Math.max(0, left.palm.x - right.palm.x);
      let best = Number.POSITIVE_INFINITY;
      let from = 0;
      for (let p = 0; p < 2; p += 1) {
        const m = memory[p];
        const c =
          cost[p] +
          move(left?.palm ?? null, m.l, m.tl, frame.t, step) +
          move(right?.palm ?? null, m.r, m.tr, frame.t, step);
        if (c < best) {
          best = c;
          from = p;
        }
      }
      nextCost[s] = best + unary;
      back[k * 2 + s] = from;
      const m = memory[from];
      nextMemory[s] = {
        l: left ? left.palm : m.l,
        tl: left ? frame.t : m.tl,
        r: right ? right.palm : m.r,
        tr: right ? frame.t : m.tr,
      };
    }
    const low = Math.min(nextCost[0], nextCost[1]);
    cost = [nextCost[0] - low, nextCost[1] - low];
    memory = nextMemory;
  }
  const out: Assigned[] = new Array(n);
  let s = cost[0] <= cost[1] ? 0 : 1;
  for (let k = n - 1; k >= 0; k -= 1) {
    const [a, b] = hands[k];
    out[k] = s === 0 ? { L: a, R: b } : { L: b, R: a };
    s = back[k * 2 + s];
  }
  return out;
}
