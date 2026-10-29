// 手元影片預填的整個流程：參考格校正 → 逐格（鏡頭追蹤＋姿態偵測）→ 機位評估 → 決定左右手 → 自動對時 → 推每顆音符的手。

import {
  cameraStats,
  cameraWarnings,
  Frames,
  guessHands,
  refineOffset,
  type CameraStats,
  type FrameSample,
  type HandcamNote,
  type HandSample,
  type NoteGuess,
  type RawStats,
} from './assign';
import { openFrames } from './capture';
import { apply, buttonPoint, fit, invert, multiply, type Mat3, type Vec2 } from './homography';
import { assignHands, bodyVisibility, type IdentityFrame } from './identity';
import { LEFT_HAND, PoseRuntime, RIGHT_HAND, type PoseFiles, type PoseResult } from './pose';
import { CameraTracker } from './track';

/** 取樣間隔：每秒 30 格（研究中 60fps 影片隔格取樣，30fps 影片逐格）。 */
const STEP = 1 / 30;
/** 鏡頭追蹤失敗時，沿用上一次成功結果的最長時間。 */
const HOLD_TRACK = 0.5;
const PALM = [0, 5, 9, 13, 17];
const TIPS = [8, 12, 16, 20, 4];
/** 找不到玩家時直接估手的範圍：盤面中心往外這麼多倍判定環半徑的正方形 */
const FALLBACK_REACH = 1.3;

export interface RingPoint {
  /** 鍵號 1–8 */
  button: number;
  /** 參考格中的位置（分析畫面像素，已扣掉翻轉） */
  x: number;
  y: number;
}

export interface JobInput {
  url: string;
  files: PoseFiles;
  notes: HandcamNote[];
  /** 影片時間 − 譜面時間 */
  offset: number;
  mirror: boolean;
  /** 標環點的那一格（影片時間）與點位（影片原始像素，已扣掉翻轉） */
  referenceTime: number;
  ring: RingPoint[];
  /** 是否用不看標註的方式微調 offset */
  refine: boolean;
}

export interface JobProgress {
  phase: 'loading' | 'reference' | 'frames' | 'finishing';
  /** 0–1 */
  fraction: number;
  framesDone: number;
  framesTotal: number;
  /** 每秒處理格數 */
  rate: number;
}

export interface JobResult {
  guesses: NoteGuess[];
  stats: CameraStats;
  warnings: string[];
  offsetUsed: number;
  shift: number;
  /** 微調找到的值落在搜尋範圍邊緣（原本的對齊可能差太多） */
  shiftAtEdge: boolean;
  backend: string;
  /** 改在盤面範圍直接估手的畫格比例 */
  fallback: number;
  /** 讀影片的方式 */
  reader: 'webcodecs' | 'video';
  seconds: number;
  frames: number;
}

function weighted(kp: Float32Array, score: Float32Array, base: number, idx: number[]): Vec2 {
  let sx = 0;
  let sy = 0;
  let sw = 0;
  for (const k of idx) {
    const w = Math.max(score[base + k], 1e-3);
    sx += kp[(base + k) * 2] * w;
    sy += kp[(base + k) * 2 + 1] * w;
    sw += w;
  }
  return { x: sx / sw, y: sy / sw };
}

function handSample(pose: PoseResult, base: number, toBoard: Mat3 | null): HandSample | null {
  let sum = 0;
  for (let k = 0; k < 21; k += 1) sum += pose.score[base + k];
  const mean = sum / 21;
  if (mean < 0.15 || !toBoard) return null;
  const palmPx = weighted(pose.kp, pose.score, base, PALM);
  const tipsPx = weighted(pose.kp, pose.score, base, TIPS);
  return { palm: apply(toBoard, palmPx.x, palmPx.y), tips: apply(toBoard, tipsPx.x, tipsPx.y), score: mean };
}

/** 盤面在這一格的範圍（正方形框，分析畫面像素）；frameFromBoard 為盤面 → 這格像素。 */
function boardBox(frameFromBoard: Mat3): [number, number, number, number] | null {
  const center = apply(frameFromBoard, 0, 0);
  let radius = 0;
  for (let k = 0; k < 8; k += 1) {
    const a = (k / 8) * Math.PI * 2;
    const p = apply(frameFromBoard, Math.cos(a), Math.sin(a));
    radius = Math.max(radius, Math.hypot(p.x - center.x, p.y - center.y));
  }
  if (!Number.isFinite(radius) || radius <= 0) return null;
  const r = radius * FALLBACK_REACH;
  return [center.x - r, center.y - r, center.x + r, center.y + r];
}

/** 這格肩、肘的可見度（0–1）。 */
function bodyScore(pose: PoseResult): number {
  return bodyVisibility(Math.max(pose.score[5], pose.score[6], pose.score[7], pose.score[8]));
}

/** 由環點求參考格像素 → 盤面的透視；點太少或共線時回傳 null。 */
export function boardFromRing(ring: RingPoint[]): Mat3 | null {
  if (ring.length < 4) return null;
  return fit(
    ring.map((p) => ({ x: p.x, y: p.y })),
    ring.map((p) => buttonPoint(p.button)),
  );
}

export async function runJob(
  input: JobInput,
  onProgress: (progress: JobProgress) => void,
  signal: AbortSignal,
): Promise<JobResult> {
  const started = performance.now();
  const check = () => {
    if (signal.aborted) throw new DOMException('cancelled', 'AbortError');
  };
  onProgress({ phase: 'loading', fraction: 0, framesDone: 0, framesTotal: 0, rate: 0 });
  const runtime = await PoseRuntime.create(input.files);
  let reader: Awaited<ReturnType<typeof openFrames>> | null = null;
  try {
    reader = await openFrames(input.url);
    check();
    // 環點是影片原始像素；分析畫面可能縮小
    const scale = reader.scale;
    const ring = input.ring.map((p) => ({ ...p, x: p.x * scale, y: p.y * scale }));
    const boardFromRef = boardFromRing(ring);
    if (!boardFromRef) throw new Error('環點太少或位置不合理，請至少標 4 個不在同一直線上的鍵位點。');
    onProgress({ phase: 'reference', fraction: 0, framesDone: 0, framesTotal: 0, rate: 0 });
    const reference = await reader.seek(input.referenceTime, signal);
    const tracker = new CameraTracker(reference.frame, boardFromRef);
    const refFromBoard = invert(boardFromRef);
    check();

    const heads = input.notes.map((note) => note.time);
    const ends = input.notes.map((note) => Math.max(note.time, note.motionEnd ?? note.time));
    // 自動對時可能往前移 0.3 秒，判定窗再往前 0.1 秒
    const from = Math.max(0, Math.min(...heads) + input.offset - 0.6);
    const to = Math.min(reader.duration, Math.max(...ends) + input.offset + 0.4);
    const total = Math.max(1, Math.ceil((to - from) / STEP));

    const people: IdentityFrame[] = [];
    const usable: boolean[] = [];
    const raw: RawStats[] = [];
    let fallbackFrames = 0;
    let lastGood: { t: number; h: Mat3 } | null = null;
    let current = await reader.seek(from, signal);
    let done = 0;
    const loopStart = performance.now();
    for (;;) {
      check();
      const tracked = tracker.track(current.frame);
      let toBoard: Mat3 | null = null;
      if (tracked.toRef) {
        toBoard = multiply(boardFromRef, tracked.toRef);
        lastGood = { t: current.time, h: toBoard };
      } else if (lastGood && current.time - lastGood.t <= HOLD_TRACK) {
        toBoard = lastGood.h;
      }
      const frameFromBoard = toBoard ? invert(toBoard) : refFromBoard;
      const pose = await runtime.detect(current.frame, frameFromBoard ? (boardBox(frameFromBoard) ?? undefined) : undefined);
      usable.push(!!toBoard);
      if (pose) {
        if (pose.fallback) fallbackFrames += 1;
        people.push({
          t: current.time,
          a: handSample(pose, LEFT_HAND, toBoard),
          b: handSample(pose, RIGHT_HAND, toBoard),
          body: bodyScore(pose),
        });
        const handScore = (base: number) => {
          let sum = 0;
          for (let k = 0; k < 21; k += 1) sum += pose.score[base + k];
          return sum / 21;
        };
        raw.push({
          hands: [handScore(LEFT_HAND), handScore(RIGHT_HAND)],
          shoulders: [pose.score[5], pose.score[6]],
          palmsPx: [weighted(pose.kp, pose.score, LEFT_HAND, PALM), weighted(pose.kp, pose.score, RIGHT_HAND, PALM)],
          tracked: !!tracked.toRef,
        });
      } else {
        people.push({ t: current.time, a: null, b: null, body: 0 });
        raw.push({ hands: [0, 0], shoulders: [0, 0], palmsPx: null, tracked: !!tracked.toRef });
      }
      done += 1;
      const elapsed = (performance.now() - loopStart) / 1000;
      onProgress({
        phase: 'frames',
        fraction: Math.min(1, (current.time - from) / (to - from)),
        framesDone: done,
        framesTotal: total,
        rate: elapsed > 0 ? done / elapsed : 0,
      });
      if (current.time >= to) break;
      const next = await reader.next(current.time + STEP - 0.004, signal);
      if (!next) break;
      current = next;
    }

    onProgress({ phase: 'finishing', fraction: 1, framesDone: done, framesTotal: total, rate: 0 });
    // 決定每格哪組是左手（模型的左右在看不到肩膀時不可靠）；之後的左右都是玩家真正的左右，不再依翻轉對調
    const hands = assignHands(people, input.mirror, STEP);
    const samples: FrameSample[] = people.map((frame, k) => ({ t: frame.t, ok: usable[k], L: hands[k].L, R: hands[k].R }));
    const frames = new Frames(samples);
    const stats = cameraStats(raw, reader.width);
    let offsetUsed = input.offset;
    let shift = 0;
    let shiftAtEdge = false;
    if (input.refine) {
      const refined = refineOffset(input.notes, frames, input.offset);
      if (Number.isFinite(refined.median)) {
        offsetUsed = refined.offset;
        shift = refined.shift;
        shiftAtEdge = refined.atEdge;
      }
    }
    const guesses = guessHands(input.notes, frames, offsetUsed, STEP);
    return {
      guesses,
      stats,
      warnings: cameraWarnings(stats),
      reader: reader.kind,
      offsetUsed,
      shift,
      shiftAtEdge,
      backend: runtime.backend,
      fallback: done > 0 ? fallbackFrames / done : 0,
      seconds: (performance.now() - started) / 1000,
      frames: done,
    };
  } finally {
    reader?.close();
    await runtime.release();
  }
}
