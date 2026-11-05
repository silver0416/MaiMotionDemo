// 部分解：模型本身走不下去時略過、不指派手的音符部分（Rust 的 solution.skipped）。
import { noteLabel, stepLabel } from './notes';
import type { Note, SkippedPart, SlidePath } from './types';

/** 略過的一處：同一刻略過的音符部分算同一處。 */
export interface SkippedSpot {
  timeSeconds: number;
  parts: SkippedPart[];
}

/** 依時刻分成幾處（Rust 已依時間排好；同一刻差在 1 微秒內）。 */
export function skippedSpots(parts: readonly SkippedPart[] | undefined): SkippedSpot[] {
  const spots: SkippedSpot[] = [];
  for (const part of parts ?? []) {
    const last = spots.at(-1);
    if (last && Math.abs(part.timeSeconds - last.timeSeconds) < 1e-6) last.parts.push(part);
    else spots.push({ timeSeconds: part.timeSeconds, parts: [part] });
  }
  return spots;
}

/** 略過部分的說明：有起點的 Slide 分成「起點」與「滑行」，其餘同音符說明。 */
export function skippedPartLabel(part: SkippedPart, note: Note | undefined, pathById: Map<string, SlidePath>): string {
  if (!note) return part.noteId;
  if (note.kind === 'slide' && note.hasHead) {
    return stepLabel({ note, part: part.part === 'slide' ? 'track' : 'hand' }, pathById);
  }
  return noteLabel(note, pathById);
}
