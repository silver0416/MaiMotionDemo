import type { Note, PathSample, Point } from './types';

export type NotePhase = 'upcoming' | 'active' | 'done';

export interface NoteVisual {
  note: Note;
  visible: boolean;
  phase: NotePhase;
  /** 飛入進度 0–1，純視覺，不影響判定時間 */
  travel: number;
  opacity: number;
  /** Hold／Slide 的完成比例 0–1 */
  progress: number;
  /** Slide 軌道上的移動比例 0–1（依 motionStart／motionEnd 線性映射） */
  slideU: number;
}

const FADE_OUT = 0.4;

export function noteVisual(
  note: Note,
  time: number,
  mode: 'window' | 'all',
  approach: boolean,
  approachSeconds: number,
): NoteVisual {
  const start = note.timeSeconds;
  const end = Math.max(note.endSeconds, note.timeSeconds);
  const appear = start - approachSeconds;
  const phase: NotePhase = time < start ? 'upcoming' : time <= end ? 'active' : 'done';
  const span = end - start;
  const progress = span > 0 ? Math.min(1, Math.max(0, (time - start) / span)) : phase === 'upcoming' ? 0 : 1;

  const motionStart = note.motionStart ?? start;
  const motionEnd = note.motionEnd ?? end;
  const motionSpan = motionEnd - motionStart;
  const slideU =
    motionSpan > 0 ? Math.min(1, Math.max(0, (time - motionStart) / motionSpan)) : time >= motionEnd ? 1 : 0;

  if (mode === 'all') {
    return {
      note,
      visible: true,
      phase,
      travel: 1,
      opacity: phase === 'active' ? 1 : phase === 'upcoming' ? 0.45 : 0.28,
      progress,
      slideU,
    };
  }

  const travel = approach
    ? Math.min(1, Math.max(0, (time - appear) / Math.max(approachSeconds, 1e-6)))
    : time >= appear
      ? 1
      : 0;

  let opacity = 1;
  if (phase === 'upcoming') opacity = 0.3 + 0.7 * travel;
  else if (phase === 'done') opacity = Math.max(0, 1 - (time - end) / FADE_OUT);

  return {
    note,
    visible: time >= appear && time <= end + FADE_OUT,
    phase,
    travel,
    opacity,
    progress,
    slideU,
  };
}

/** 飛入動畫時的半徑倍率；判定時間之後固定為 1（Rust 座標原位）。 */
export function approachScale(visual: NoteVisual): number {
  if (visual.phase !== 'upcoming') return 1;
  return 0.2 + 0.8 * visual.travel;
}

export function scalePoint(point: Point, factor: number): Point {
  return { x: point.x * factor, y: point.y * factor };
}

export function pathLength(samples: PathSample[]): number {
  let total = 0;
  for (let i = 1; i < samples.length; i += 1) {
    total += Math.hypot(samples[i].x - samples[i - 1].x, samples[i].y - samples[i - 1].y);
  }
  return total;
}
