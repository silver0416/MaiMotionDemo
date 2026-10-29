// 由逐格的手部位置推每顆音符的左右手。規則與門檻沿用可行性研究在「四月の雨」定案、
// 並以「言ノ葉カルマ MASTER」獨立驗證的版本（整體約 89–91%，保留信心前約 80% 時約 95%）：
// - 接觸：判定前 0.10 秒到判定時刻，每隻手（掌心或指尖取較近者）到目標的平均距離，較近者為該手；
//   同時兩顆以總距離最小的方式各分一隻手。
// - 信心：兩手距離差 ×「最近那隻手真的在目標附近」的折扣；≥ 0.54 為高、≥ 0.091 為中，其餘不填。
// - Slide 滑行：滑行期間（時間提前 0.1 秒）哪隻手較貼近星星的預期位置。
// 目標：按鍵是該鍵方向上盤面半徑 0.7–1.3 的線段（螢幕 A 區或實體鍵都算）；Touch 是感應區代表點。

import { buttonAngle, type Vec2 } from './homography';

export interface HandSample {
  /** 掌心（盤面座標） */
  palm: Vec2;
  /** 指尖平均（盤面座標） */
  tips: Vec2;
  score: number;
}

export interface FrameSample {
  /** 影片時間（秒） */
  t: number;
  /** 校正失敗的格為 false，該格不提供手的位置 */
  ok: boolean;
  /** 玩家真正的左右手（已由 identity.ts 決定，含影片翻轉） */
  L: HandSample | null;
  R: HandSample | null;
}

export interface HandcamNote {
  key: string;
  kind: string;
  button: number;
  touchArea: string | null;
  position: Vec2;
  time: number;
  hasHead: boolean;
  motionStart: number | null;
  motionEnd: number | null;
  /** Slide 路徑（弧長參數 u 與盤面座標）；其他音符為 null */
  path: { u: number; x: number; y: number }[] | null;
}

export type Tier = 'high' | 'mid' | 'low';

export interface NoteGuess {
  key: string;
  hand: 'L' | 'R' | null;
  confidence: number;
  tier: Tier;
  track: 'L' | 'R' | null;
  trackConfidence: number;
  trackTier: Tier | null;
}

const WINDOW_START = -0.1;
const WINDOW_END = 0;
const SLIDE_SHIFT = -0.1;
export const HIGH = 0.54;
export const MID = 0.091;
export const TRACK_HIGH = 0.563;
export const TRACK_MID = 0.061;
const MISSING = 9;

function segmentDistance(p: Vec2, angle: number, r0 = 0.7, r1 = 1.3): number {
  const ax = Math.cos(angle);
  const ay = Math.sin(angle);
  const t = Math.min(r1, Math.max(r0, p.x * ax + p.y * ay));
  return Math.hypot(p.x - t * ax, p.y - t * ay);
}

function isButtonNote(note: HandcamNote): boolean {
  return note.kind === 'tap' || note.kind === 'hold' || note.kind === 'slide';
}

function targetDistance(note: HandcamNote, p: Vec2): number {
  if (isButtonNote(note)) return segmentDistance(p, buttonAngle(note.button));
  return Math.hypot(p.x - note.position.x, p.y - note.position.y);
}

function handDistance(note: HandcamNote, hand: HandSample | null): number {
  if (!hand) return MISSING;
  return Math.min(targetDistance(note, hand.palm), targetDistance(note, hand.tips));
}

/** 依時間排序的格；用二分搜尋找最接近 t 的格。 */
export class Frames {
  readonly list: FrameSample[];
  readonly times: Float64Array;
  constructor(list: FrameSample[]) {
    this.list = [...list].sort((a, b) => a.t - b.t);
    this.times = Float64Array.from(this.list.map((f) => f.t));
  }

  nearest(t: number): FrameSample | null {
    const times = this.times;
    if (times.length === 0) return null;
    let lo = 0;
    let hi = times.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (times[mid] < t) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0 && Math.abs(times[lo - 1] - t) <= Math.abs(times[lo] - t)) lo -= 1;
    return this.list[lo];
  }

  /** 時間落在 [from, to] 的格。 */
  between(from: number, to: number): FrameSample[] {
    const out: FrameSample[] = [];
    let lo = 0;
    let hi = this.times.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.times[mid] < from) lo = mid + 1;
      else hi = mid;
    }
    for (let k = lo; k < this.times.length && this.times[k] <= to; k += 1) out.push(this.list[k]);
    return out;
  }
}

/** 判定窗內每隻手到目標的平均距離；沒有可用格時為 MISSING。 */
function windowDistances(frames: Frames, note: HandcamNote, offset: number): [number, number] {
  const at = note.time + offset;
  const list = frames.between(at + WINDOW_START - 1e-6, at + WINDOW_END + 1e-6);
  // 取樣率低時窗內可能沒有格：退回最接近判定時刻的一格
  const usable = list.length > 0 ? list : [frames.nearest(at)].filter((f): f is FrameSample => !!f);
  const sums = [0, 0];
  const counts = [0, 0];
  for (const frame of usable) {
    if (!frame.ok) continue;
    const dl = handDistance(note, frame.L);
    const dr = handDistance(note, frame.R);
    if (dl < MISSING) {
      sums[0] += dl;
      counts[0] += 1;
    }
    if (dr < MISSING) {
      sums[1] += dr;
      counts[1] += 1;
    }
  }
  return [counts[0] ? sums[0] / counts[0] : MISSING, counts[1] ? sums[1] / counts[1] : MISSING];
}

function confidence(margin: number, dmin: number, dl: number, dr: number): number {
  if (Math.max(dl, dr) >= MISSING) return 0;
  return margin * Math.min(1, Math.max(0, 1 - Math.max(dmin - 0.25, 0) / 0.5));
}

function tier(value: number, high: number, mid: number): Tier {
  return value >= high ? 'high' : value >= mid ? 'mid' : 'low';
}

function median(values: number[]): number {
  if (values.length === 0) return Number.NaN;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

function slideTrack(frames: Frames, note: HandcamNote, offset: number, step: number): { hand: 'L' | 'R'; conf: number } | null {
  if (!note.path || note.path.length < 2 || note.motionStart === null || note.motionEnd === null) return null;
  const t0 = note.motionStart + offset + SLIDE_SHIFT;
  const t1 = note.motionEnd + offset + SLIDE_SHIFT;
  if (t1 <= t0) return null;
  const path = note.path;
  const star = (u: number): Vec2 => {
    if (u <= path[0].u) return path[0];
    for (let k = 1; k < path.length; k += 1) {
      if (path[k].u >= u) {
        const a = path[k - 1];
        const b = path[k];
        const f = b.u > a.u ? (u - a.u) / (b.u - a.u) : 0;
        return { x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f };
      }
    }
    return path[path.length - 1];
  };
  const acc: [number[], number[]] = [[], []];
  let samples = 0;
  for (let t = t0; t < t1; t += step) {
    samples += 1;
    const frame = frames.nearest(t);
    if (!frame || !frame.ok) continue;
    const p = star((t - t0) / (t1 - t0));
    [frame.L, frame.R].forEach((hand, index) => {
      if (!hand) return;
      acc[index].push(Math.min(Math.hypot(hand.palm.x - p.x, hand.palm.y - p.y), Math.hypot(hand.tips.x - p.x, hand.tips.y - p.y)));
    });
  }
  const need = Math.max(2, Math.floor(samples / 3));
  const m = acc.map((values) => (values.length >= need ? median(values) : MISSING));
  if (Math.min(m[0], m[1]) >= MISSING) return null;
  const min = Math.min(m[0], m[1]);
  const conf = Math.abs(m[0] - m[1]) * Math.min(1, Math.max(0, 1 - Math.max(min - 0.3, 0) / 0.5));
  return { hand: m[0] < m[1] ? 'L' : 'R', conf: Math.max(m[0], m[1]) >= MISSING ? Math.min(conf, TRACK_HIGH * 0.99) : conf };
}

/** 推每顆音符的左右手。offset：影片時間 − 譜面時間；step：取樣間隔（秒）。 */
export function guessHands(notes: HandcamNote[], frames: Frames, offset: number, step: number): NoteGuess[] {
  const heads = notes.filter((note) => note.hasHead);
  const distances = new Map<string, [number, number]>();
  for (const note of heads) distances.set(note.key, windowDistances(frames, note, offset));
  const contact = new Map<string, { hand: 'L' | 'R'; conf: number }>();
  const groups = new Map<string, HandcamNote[]>();
  for (const note of heads) {
    const id = note.time.toFixed(3);
    const group = groups.get(id);
    if (group) group.push(note);
    else groups.set(id, [note]);
  }
  for (const group of groups.values()) {
    if (group.length === 2) {
      const [a1, b1] = distances.get(group[0].key)!;
      const [a2, b2] = distances.get(group[1].key)!;
      const s1 = a1 + b2;
      const s2 = b1 + a2;
      const margin = Math.abs(s1 - s2) / 2;
      const hands: ['L' | 'R', 'L' | 'R'] = s1 < s2 ? ['L', 'R'] : ['R', 'L'];
      group.forEach((note, index) => {
        const [dl, dr] = distances.get(note.key)!;
        contact.set(note.key, { hand: hands[index], conf: confidence(margin, Math.min(dl, dr), dl, dr) });
      });
    } else {
      for (const note of group) {
        const [dl, dr] = distances.get(note.key)!;
        contact.set(note.key, { hand: dl < dr ? 'L' : 'R', conf: confidence(Math.abs(dl - dr), Math.min(dl, dr), dl, dr) });
      }
    }
  }
  return notes.map((note) => {
    const head = contact.get(note.key);
    // Touch 未經驗證：最多只給中信心
    const headTier = head ? tier(head.conf, isButtonNote(note) ? HIGH : Number.POSITIVE_INFINITY, MID) : 'low';
    const track = note.kind === 'slide' ? slideTrack(frames, note, offset, step) : null;
    return {
      key: note.key,
      hand: head ? head.hand : null,
      confidence: head?.conf ?? 0,
      tier: headTier,
      track: track ? track.hand : null,
      trackConfidence: track?.conf ?? 0,
      trackTier: track ? tier(track.conf, TRACK_HIGH, TRACK_MID) : null,
    };
  });
}

/**
 * 不看標註的對時微調：在 −0.30～+0.15 秒間找讓「最近那隻手到目標距離」中位數最小的平移量。
 * 研究中使用者手動對的 offset 比這個準則晚約 0.025 秒，建議值已加上這段。
 */
export function refineOffset(notes: HandcamNote[], frames: Frames, offset: number): { offset: number; shift: number; median: number; atEdge: boolean } {
  const heads = notes.filter((note) => note.hasHead && isButtonNote(note));
  let best = { shift: 0, median: Number.POSITIVE_INFINITY };
  const from = -0.3;
  const to = 0.15;
  for (let shift = from; shift <= to + 1e-9; shift += 0.0125) {
    const values: number[] = [];
    for (const note of heads) {
      const frame = frames.nearest(note.time + offset + shift);
      if (!frame || !frame.ok) continue;
      const d = Math.min(handDistance(note, frame.L), handDistance(note, frame.R));
      if (d < MISSING) values.push(d);
    }
    if (values.length < 10) continue;
    const m = median(values);
    if (m < best.median) best = { shift, median: m };
  }
  const atEdge = best.shift <= from + 1e-6 || best.shift >= to - 1e-6;
  return { offset: offset + best.shift + 0.025, shift: best.shift + 0.025, median: best.median, atEdge };
}

export interface CameraStats {
  /** 雙手同時偵測到的格數比例 */
  bothHands: number;
  /** 肩膀分數中位數（左、右） */
  shoulders: [number, number];
  /** 相鄰格左右身分疑似互換的比例 */
  swaps: number;
  /** 左右標籤疊在同一隻手的比例 */
  collapse: number;
  /** 校正（鏡頭追蹤）成功的格數比例 */
  tracked: number;
}

export interface RawStats {
  /** 每格：兩手偵測分數、肩分數、兩手掌心影像座標（像素） */
  hands: [number, number];
  shoulders: [number, number];
  palmsPx: [Vec2, Vec2] | null;
  tracked: boolean;
}

export function cameraStats(raw: RawStats[], frameWidth: number): CameraStats {
  const n = raw.length || 1;
  const both = raw.map((r) => r.hands[0] > 0.3 && r.hands[1] > 0.3);
  const scale = frameWidth / 1280;
  let swaps = 0;
  let swapPairs = 0;
  let collapse = 0;
  let bothCount = 0;
  raw.forEach((r, k) => {
    if (!both[k] || !r.palmsPx) return;
    bothCount += 1;
    const [l, rr] = r.palmsPx;
    const sep = Math.hypot(l.x - rr.x, l.y - rr.y);
    if (sep < 40 * scale) collapse += 1;
    const prev = raw[k - 1];
    if (k > 0 && both[k - 1] && prev.palmsPx && sep > 80 * scale) {
      const [pl, pr] = prev.palmsPx;
      const same = Math.hypot(l.x - pl.x, l.y - pl.y) + Math.hypot(rr.x - pr.x, rr.y - pr.y);
      const cross = Math.hypot(l.x - pr.x, l.y - pr.y) + Math.hypot(rr.x - pl.x, rr.y - pl.y);
      swapPairs += 1;
      if (cross < same * 0.5) swaps += 1;
    }
  });
  return {
    bothHands: bothCount / n,
    shoulders: [median(raw.map((r) => r.shoulders[0])), median(raw.map((r) => r.shoulders[1]))],
    swaps: swapPairs ? swaps / swapPairs : 0,
    collapse: bothCount ? collapse / bothCount : 0,
    tracked: raw.filter((r) => r.tracked).length / n,
  };
}

/** 機位不適合時的說明；空陣列表示可用。門檻依研究中可用與不可用影片的差距訂定。 */
export function cameraWarnings(stats: CameraStats): string[] {
  const out: string[] = [];
  if (stats.tracked < 0.85) out.push(`只有 ${Math.round(stats.tracked * 100)}% 的畫格對得上盤面，鏡頭晃動太大或盤面被擋住。`);
  if (stats.bothHands < 0.75) out.push(`只有 ${Math.round(stats.bothHands * 100)}% 的畫格同時看到兩隻手。`);
  if (Math.max(stats.shoulders[0], stats.shoulders[1]) < 0.35) {
    out.push('幾乎看不到玩家的肩膀，左右手改用手在盤面上的位置判斷；兩手交叉的地方容易判斷錯。');
  }
  return out;
}
