// 真人手順標註：草稿、預填、檔案格式（maimotion-hand-annotation 第 1 版）與合併。
// 這裡只處理資料；比對與「照標註求解」由 Rust 核心負責。

import {
  ANNOTATION_FORMAT,
  ANNOTATION_VERSION,
  type Confidence,
  type Hand,
  type HandAnnotation,
  type HandoverMark,
  type Note,
  type NoteAnnotation,
  type RangeMemo,
  type Solution,
  type TrackHand,
} from './types';
import { cleanLink, linkFromFile, linkToFile, type VideoLink } from './video';

/** 存在譜面紀錄裡的編輯狀態；notes 以音符穩定鍵為索引。 */
export interface AnnotationDraft {
  title: string;
  annotators: string[];
  memo: string;
  notes: Record<string, NoteAnnotation>;
  ranges: RangeMemo[];
  /** 最後編輯時間（epoch 毫秒） */
  updatedAt: number;
  /** 對照用的真人影片與同步偏移 */
  video?: VideoLink;
}

export const CONFIDENCE_LABEL: Record<Confidence, string> = {
  sure: '確定',
  unsure: '不確定',
  either: '皆可',
};

export function emptyDraft(title = ''): AnnotationDraft {
  return { title, annotators: [], memo: '', notes: {}, ranges: [], updatedAt: 0 };
}

function isHand(value: unknown): value is Hand {
  return value === 'L' || value === 'R';
}

function isTrack(value: unknown): value is TrackHand {
  return value === 'L' || value === 'R' || value === 'LR';
}

function isConfidence(value: unknown): value is Confidence {
  return value === 'sure' || value === 'unsure' || value === 'either';
}

function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function finite(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

/** 這顆音符需要標哪些部分：Slide 要標滑行手，有起點觸碰的還要標起點。 */
export function neededParts(note: Note): { hand: boolean; track: boolean } {
  const slide = note.kind === 'slide';
  return { hand: !slide || note.hasHead, track: slide };
}

/** 音符是否為 WiFi（可以兩手一起滑）。 */
export function isWifi(note: Note, shape: string | undefined): boolean {
  return note.kind === 'slide' && shape === 'w';
}

/** 有人確認過的標註：至少有一個部分，而且不是模型預填。 */
export function isHumanLabeled(mark: NoteAnnotation | undefined): boolean {
  return !!mark && !mark.prefilled && (mark.hand !== undefined || mark.track !== undefined);
}

/** 需要的部分都填了（不論是否為預填）。 */
export function isFilled(note: Note, mark: NoteAnnotation | undefined): boolean {
  if (!mark) return false;
  const need = neededParts(note);
  return (!need.hand || mark.hand !== undefined) && (!need.track || mark.track !== undefined);
}

function cleanHandovers(value: unknown): HandoverMark[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(
      (item): item is HandoverMark =>
        typeof item === 'object' && item !== null && finite((item as HandoverMark).at) && isHand((item as HandoverMark).to),
    )
    .map((item) => ({ at: item.at, to: item.to }))
    .sort((a, b) => a.at - b.at);
}

/** 把任意輸入整理成合法的單顆標註；沒有 key 或沒有任何內容時回傳 null。 */
export function cleanMark(value: unknown): NoteAnnotation | null {
  if (typeof value !== 'object' || value === null) return null;
  const raw = value as Record<string, unknown>;
  if (typeof raw.key !== 'string' || raw.key === '') return null;
  const mark: NoteAnnotation = { key: raw.key };
  if (isHand(raw.hand)) mark.hand = raw.hand;
  if (isTrack(raw.track)) mark.track = raw.track;
  const handovers = cleanHandovers(raw.handovers);
  if (handovers.length > 0) mark.handovers = handovers;
  if (isConfidence(raw.confidence) && raw.confidence !== 'sure') mark.confidence = raw.confidence;
  if (raw.prefilled === true) mark.prefilled = true;
  if (text(raw.memo)) mark.memo = text(raw.memo);
  if (text(raw.by)) mark.by = text(raw.by);
  const empty = mark.hand === undefined && mark.track === undefined && !mark.memo && mark.confidence === undefined;
  return empty ? null : mark;
}

function cleanRanges(value: unknown): RangeMemo[] {
  if (!Array.isArray(value)) return [];
  const out: RangeMemo[] = [];
  for (const item of value) {
    if (typeof item !== 'object' || item === null) continue;
    const raw = item as Record<string, unknown>;
    if (!finite(raw.from) || !finite(raw.to) || !text(raw.memo)) continue;
    const range: RangeMemo = { from: Math.min(raw.from, raw.to), to: Math.max(raw.from, raw.to), memo: text(raw.memo) };
    if (text(raw.by)) range.by = text(raw.by);
    out.push(range);
  }
  return out.sort((a, b) => a.from - b.from);
}

/** 從資料庫讀回的草稿；格式不對時回傳 null。 */
export function cleanDraft(value: unknown): AnnotationDraft | null {
  if (typeof value !== 'object' || value === null) return null;
  const raw = value as Record<string, unknown>;
  const notes: Record<string, NoteAnnotation> = {};
  if (typeof raw.notes === 'object' && raw.notes !== null) {
    for (const item of Object.values(raw.notes as Record<string, unknown>)) {
      const mark = cleanMark(item);
      if (mark) notes[mark.key] = mark;
    }
  }
  const video = cleanLink(raw.video);
  return {
    title: text(raw.title),
    annotators: Array.isArray(raw.annotators) ? raw.annotators.filter((a): a is string => typeof a === 'string' && a !== '') : [],
    memo: text(raw.memo),
    notes,
    ranges: cleanRanges(raw.ranges),
    updatedAt: finite(raw.updatedAt) ? raw.updatedAt : 0,
    ...(video ? { video } : {}),
  };
}

/** 模型方案在某顆音符上的手部分工，用來預填與比對顯示。 */
export function modelMark(solution: Solution, note: Note): Omit<NoteAnnotation, 'key'> {
  const list = solution.assignments.filter((item) => item.noteId === note.id);
  const out: Omit<NoteAnnotation, 'key'> = {};
  const contact = list
    .filter((item) => item.part !== 'slide')
    .sort((a, b) => a.startSeconds - b.startSeconds)[0];
  if (contact) out.hand = contact.hand;
  const slides = list.filter((item) => item.part === 'slide').sort((a, b) => a.startSeconds - b.startSeconds);
  const handovers = solution.handovers
    .filter((item) => item.noteId === note.id)
    .sort((a, b) => a.startSeconds - b.startSeconds);
  if (slides.length > 0) {
    const first = slides[0];
    // 交接期間兩手也會重疊；只有不是交接造成的重疊才算兩手一起滑（WiFi 2+1）。
    const both = slides.some(
      (item) =>
        item.hand !== first.hand &&
        item.startSeconds < first.endSeconds &&
        first.startSeconds < item.endSeconds &&
        !handovers.some((h) => Math.abs(h.startSeconds - item.startSeconds) < 1e-9),
    );
    out.track = both ? 'LR' : first.hand;
    if (!both && handovers.length > 0) {
      out.handovers = handovers.map((item) => ({ at: item.startSeconds, to: item.to }));
    }
  }
  return out;
}

/** 以模型方案填入還沒標的音符；回傳填入的數量。已有標註（含預填）的不動。 */
export function prefill(draft: AnnotationDraft, notes: Note[], solution: Solution): number {
  let count = 0;
  for (const note of notes) {
    if (!note.key || draft.notes[note.key]) continue;
    const mark = modelMark(solution, note);
    if (mark.hand === undefined && mark.track === undefined) continue;
    draft.notes[note.key] = { key: note.key, ...mark, prefilled: true };
    count += 1;
  }
  return count;
}

function stamp(value: number): string {
  return value > 0 ? new Date(value).toISOString() : '';
}

/** 草稿轉成標註檔；只輸出有內容的音符，依譜面順序排列。 */
export function toFile(
  draft: AnnotationDraft,
  chart: { source: string; sha256: string; firstSeconds: number; notes: Note[] },
): HandAnnotation {
  const order = new Map(chart.notes.map((note, index) => [note.key ?? '', index]));
  const video = linkToFile(draft.video);
  const notes = Object.values(draft.notes).sort(
    (a, b) => (order.get(a.key) ?? Infinity) - (order.get(b.key) ?? Infinity) || (a.key < b.key ? -1 : 1),
  );
  return {
    format: ANNOTATION_FORMAT,
    version: ANNOTATION_VERSION,
    title: draft.title,
    annotators: [...draft.annotators],
    updatedAt: stamp(draft.updatedAt),
    chart: {
      sha256: chart.sha256,
      firstSeconds: chart.firstSeconds,
      noteCount: chart.notes.length,
      source: chart.source,
    },
    memo: draft.memo,
    notes: notes.map((mark) => ({ ...mark })),
    ranges: draft.ranges.map((range) => ({ ...range })),
    ...(video ? { video } : {}),
  };
}

/**
 * 標註檔文字：一般 JSON，但每顆音符與每段備註各佔一行，原譜放在最後，
 * 方便貼上、閱讀與用 diff 比對不同人的標註。
 */
export function serialize(file: HandAnnotation): string {
  const line = (value: unknown) => JSON.stringify(value);
  const list = (items: unknown[]) =>
    items.length === 0 ? '[]' : `[\n${items.map((item) => `    ${line(item)}`).join(',\n')}\n  ]`;
  return [
    '{',
    `  "format": ${line(file.format)},`,
    `  "version": ${file.version},`,
    `  "title": ${line(file.title)},`,
    `  "annotators": ${line(file.annotators)},`,
    `  "updatedAt": ${line(file.updatedAt)},`,
    `  "memo": ${line(file.memo)},`,
    ...(file.video ? [`  "video": ${line(file.video)},`] : []),
    `  "ranges": ${list(file.ranges)},`,
    `  "notes": ${list(file.notes)},`,
    `  "chart": ${line(file.chart)}`,
    '}',
    '',
  ].join('\n');
}

export type ParseResult = { ok: true; file: HandAnnotation; skipped: number } | { ok: false; error: string };

/** 讀取貼上或開啟的標註檔文字；不合規的單顆標註略過並回報數量。 */
export function parseFile(input: string): ParseResult {
  let raw: unknown;
  try {
    raw = JSON.parse(input);
  } catch {
    return { ok: false, error: '不是有效的 JSON。請貼上完整的標註檔內容。' };
  }
  if (typeof raw !== 'object' || raw === null) return { ok: false, error: '標註檔內容是空的。' };
  const data = raw as Record<string, unknown>;
  if (data.format !== ANNOTATION_FORMAT) {
    return { ok: false, error: `不是真人手順標註檔（format 應為 ${ANNOTATION_FORMAT}）。` };
  }
  if (data.version !== ANNOTATION_VERSION) {
    return { ok: false, error: `標註檔版本 ${String(data.version)} 不支援，本版只讀第 ${ANNOTATION_VERSION} 版。` };
  }
  const chart = data.chart as Record<string, unknown> | undefined;
  if (!chart || typeof chart.source !== 'string' || chart.source.trim() === '') {
    return { ok: false, error: '標註檔沒有附原譜，無法對應音符。' };
  }
  const input_notes = Array.isArray(data.notes) ? data.notes : [];
  const video = linkToFile(linkFromFile(data.video));
  const notes = input_notes.map(cleanMark).filter((mark): mark is NoteAnnotation => mark !== null);
  return {
    ok: true,
    skipped: input_notes.length - notes.length,
    file: {
      format: ANNOTATION_FORMAT,
      version: ANNOTATION_VERSION,
      title: text(data.title),
      annotators: Array.isArray(data.annotators)
        ? data.annotators.filter((a): a is string => typeof a === 'string' && a !== '')
        : [],
      updatedAt: text(data.updatedAt),
      chart: {
        sha256: text(chart.sha256),
        firstSeconds: finite(chart.firstSeconds) ? chart.firstSeconds : 0,
        noteCount: finite(chart.noteCount) ? chart.noteCount : 0,
        source: chart.source,
      },
      memo: text(data.memo),
      notes,
      ranges: cleanRanges(data.ranges),
      ...(video ? { video } : {}),
    },
  };
}

function sameHands(a: NoteAnnotation, b: NoteAnnotation): boolean {
  const handovers = (m: NoteAnnotation) => JSON.stringify(m.handovers ?? []);
  return a.hand === b.hand && a.track === b.track && handovers(a) === handovers(b);
}

export interface MergeResult {
  draft: AnnotationDraft;
  /** 新增的音符標註數 */
  added: number;
  /** 手順不同的音符鍵；fill 模式保留自己的，replace 模式採用匯入的 */
  conflicts: string[];
}

/**
 * 合併匯入的標註。
 * - fill：只補自己沒標（或只是預填）的音符，手順不同的保留自己的並列為衝突。
 * - replace：匯入的為準，覆蓋同一顆音符。
 * 兩種模式的備註都會保留雙方內容，區段備註與標註者名單取聯集。
 */
export function merge(mine: AnnotationDraft, file: HandAnnotation, mode: 'fill' | 'replace'): MergeResult {
  const draft: AnnotationDraft = {
    ...mine,
    notes: { ...mine.notes },
    ranges: [...mine.ranges],
    annotators: [...mine.annotators],
  };
  let added = 0;
  const conflicts: string[] = [];
  for (const theirs of file.notes) {
    const current = draft.notes[theirs.key];
    if (!current || !isHumanLabeled(current)) {
      if (!theirs.prefilled || !current) {
        draft.notes[theirs.key] = { ...theirs };
        if (isHumanLabeled(theirs)) added += 1;
      }
      continue;
    }
    if (!isHumanLabeled(theirs)) continue;
    const different = !sameHands(current, theirs);
    if (different) conflicts.push(theirs.key);
    const memo = [current.memo, theirs.memo].filter((item, index, all) => item && all.indexOf(item) === index).join(' / ');
    if (mode === 'replace' && different) {
      draft.notes[theirs.key] = { ...theirs, ...(memo ? { memo } : {}) };
    } else if (memo && memo !== current.memo) {
      draft.notes[theirs.key] = { ...current, memo };
    }
  }
  for (const range of file.ranges) {
    const exists = draft.ranges.some(
      (item) => item.from === range.from && item.to === range.to && item.memo === range.memo,
    );
    if (!exists) draft.ranges.push({ ...range });
  }
  draft.ranges.sort((a, b) => a.from - b.from);
  for (const name of file.annotators) {
    if (!draft.annotators.includes(name)) draft.annotators.push(name);
  }
  if (file.memo && !draft.memo.includes(file.memo)) {
    draft.memo = draft.memo ? `${draft.memo}\n\n${file.memo}` : file.memo;
  }
  if (!draft.title) draft.title = file.title;
  // 自己還沒挑影片時，沿用對方的影片與同步偏移。
  const video = linkFromFile(file.video);
  if (!draft.video && video) draft.video = video;
  return { draft, added, conflicts };
}

/** 標註進度：需要標的音符中，已由人確認的數量。 */
export function progress(draft: AnnotationDraft, notes: Note[]): { done: number; total: number; prefilled: number } {
  let done = 0;
  let prefilled = 0;
  for (const note of notes) {
    const mark = note.key ? draft.notes[note.key] : undefined;
    if (!mark || !isFilled(note, mark)) continue;
    if (mark.prefilled) prefilled += 1;
    else done += 1;
  }
  return { done, total: notes.length, prefilled };
}

/** 檔名：標題轉成安全字元，附日期。 */
export function fileName(title: string, now = new Date()): string {
  const base = (title || 'chart').replace(/[\\/:*?"<>|\s]+/g, '_').slice(0, 60);
  const day = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, '0')}${String(now.getDate()).padStart(2, '0')}`;
  return `${base}.${day}.maimotion-hands.json`;
}
