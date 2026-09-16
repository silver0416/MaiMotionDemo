// 手掌覆蓋多個同時 Touch 的顯示輔助。
// 掌心、半徑、覆蓋名單與起止時間一律取自 Rust 的 solution.palmPlacements；
// 前端不自行判斷哪些 Touch 能被一掌覆蓋，也不用畫面上的形狀改變判定。

import { noteTarget } from './notes';
import type { Note, PalmPlacement, Point } from './types';

/** 各面板共用的說明，避免不同位置寫出不一致的宣稱。 */
export const PALM_APPROX_HINT =
  '半徑 0.5 代表覆蓋面積約為盤面四分之一（面積比等於半徑比的平方），是 Demo 的圓形近似，不是官方判定或真實手形。';

export function palmSeconds(placement: PalmPlacement): number {
  return Math.max(placement.endSeconds - placement.startSeconds, 0);
}

/**
 * 覆蓋區間內（含端點）的手掌。
 * 只比較絕對播放時間，因此 seek 到同一秒得到同一組手掌。
 */
export function activePalms(placements: PalmPlacement[], time: number): PalmPlacement[] {
  return placements.filter(
    (placement) => time >= placement.startSeconds && time <= placement.endSeconds,
  );
}

/** noteId → 覆蓋它的手掌動作。 */
export function palmByNote(placements: PalmPlacement[]): Map<string, PalmPlacement> {
  const map = new Map<string, PalmPlacement>();
  for (const placement of placements) {
    for (const noteId of placement.coveredNoteIds) map.set(noteId, placement);
  }
  return map;
}

/** 覆蓋名單的落點名稱（例如 C、B1、E1）；查不到音符就退回 noteId。 */
export function coveredTargets(placement: PalmPlacement, noteById: Map<string, Note>): string[] {
  return placement.coveredNoteIds.map((noteId) => {
    const note = noteById.get(noteId);
    return note ? noteTarget(note) : noteId;
  });
}

export interface PalmCover {
  noteId: string;
  /** 被覆蓋音符的盤面座標，直接取 Rust 的 note.position */
  at: Point;
}

/** 被覆蓋音符的位置，用來在盤面上標出這一掌蓋住了哪幾顆 Touch。 */
export function palmCovers(placement: PalmPlacement, noteById: Map<string, Note>): PalmCover[] {
  const out: PalmCover[] = [];
  for (const noteId of placement.coveredNoteIds) {
    const note = noteById.get(noteId);
    if (!note) continue;
    out.push({ noteId, at: note.position });
  }
  return out;
}

function unitFromCenter(point: Point): Point {
  const length = Math.hypot(point.x, point.y);
  // 掌心正好在盤面中心時沒有「朝外」方向，固定朝上讓標籤位置穩定。
  if (length < 1e-6) return { x: 0, y: -1 };
  return { x: point.x / length, y: point.y / length };
}

/** 標籤放到盤面外緣的留白處，最近與最遠都夾住，避免壓在音符或落點標記上。 */
const LABEL_MIN = 1.1;
const LABEL_MAX = 1.18;

/**
 * 標籤錨點：沿盤面中心 → 掌心的方向放到盤面邊緣外的留白，
 * 避開覆蓋圓內的音符、手標記與 33 個落點參考標記，再夾住長度避免被畫面裁掉。
 */
export function palmLabelAnchor(placement: PalmPlacement): Point {
  const dir = unitFromCenter(placement.center);
  const reach = Math.hypot(placement.center.x, placement.center.y) + placement.radius + 0.06;
  const length = Math.min(Math.max(reach, LABEL_MIN), LABEL_MAX);
  return { x: dir.x * length, y: dir.y * length };
}

/** 標籤往上或往下展開：掌心在上半盤就往上，文字不會壓進覆蓋圓。 */
export function palmLabelUpward(placement: PalmPlacement): boolean {
  return placement.center.y <= 0;
}
