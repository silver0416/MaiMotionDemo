// 真人手順標註：草稿、預填、檔案格式（maimotion-hand-annotation 第 1 版）與合併。
// 這裡只處理資料；比對與「照標註求解」由 Rust 核心負責。

import {
  ANNOTATION_FORMAT,
  ANNOTATION_VERSION,
  type AnnotationBranchFile,
  type Confidence,
  type Hand,
  type HandAnnotation,
  type HandoverMark,
  type Note,
  type NoteAnnotation,
  type NoteGroup,
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
  /** 分組（看譜時覺得是一組的範圍） */
  groups: NoteGroup[];
  /** 最後編輯時間（epoch 毫秒） */
  updatedAt: number;
  /** 對照用的真人影片與同步偏移 */
  video?: VideoLink;
  /** 其他打法的分支 */
  branches: AnnotationBranch[];
  /** 目前走哪條線（MAIN_LINE 或分支 id）；只存在本機，不寫進標註檔 */
  active: string;
  /** 主線在分支圖上的顏色；只存在本機 */
  mainColor: string;
}

/** 主線的 id；分支的 parent 指向它表示從主線分出來。 */
export const MAIN_LINE = 'main';
export const MAIN_NAME = '主線';

/**
 * 分支圖的線色。避開左右手的粉紅與藍色，在深色底上都清楚；第一個給主線。
 * 路線沒走到的線一律畫灰色，所以同時出現的顏色不多。
 */
export const LINE_COLORS = ['#f0b429', '#5fc98a', '#a78bfa', '#f08a4b', '#3fc1b0', '#c9d36a', '#8ea3ff', '#e8e3d3'];

export function isColor(value: unknown): value is string {
  return typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);
}

/** 還沒被用掉的第一個顏色；都用過就依數量輪替。 */
export function nextLineColor(used: string[]): string {
  const taken = new Set(used.map((color) => color.toLowerCase()));
  return LINE_COLORS.find((color) => !taken.has(color)) ?? LINE_COLORS[1 + (used.length % (LINE_COLORS.length - 1))];
}

/**
 * 打法分支（編輯用）：from～to 這段的步驟用自己的手順。
 * 分支前沿著 parent（分出的線）走，分支後沿著 merge（併入的線）走；兩者可以不同。
 */
export interface AnnotationBranch {
  id: string;
  name: string;
  parent: string;
  merge: string;
  from: number;
  to: number;
  memo: string;
  by?: string;
  /** 分支圖上的線色 */
  color: string;
  notes: Record<string, NoteAnnotation>;
}

export const CONFIDENCE_LABEL: Record<Confidence, string> = {
  sure: '確定',
  unsure: '不確定',
  either: '皆可',
};

export function emptyDraft(title = ''): AnnotationDraft {
  return {
    title,
    annotators: [],
    memo: '',
    notes: {},
    ranges: [],
    groups: [],
    updatedAt: 0,
    branches: [],
    active: MAIN_LINE,
    mainColor: LINE_COLORS[0],
  };
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

/** 標註的一步：接觸（含 Slide 起點）或 Slide 滑行。 */
export type StepPart = 'hand' | 'track';

/** 這一步的時間：接觸為判定時間，滑行為開始移動的時間。 */
export function stepTime(note: Note, part: StepPart): number {
  return part === 'hand' ? note.timeSeconds : (note.motionStart ?? note.timeSeconds);
}

/** 這顆音符的第一步（信心與備註記在這一步）。 */
export function firstPart(note: Note): StepPart {
  return neededParts(note).hand ? 'hand' : 'track';
}

/**
 * 一步負責的欄位。Slide 的起點與滑行可以分屬不同的打法分支，
 * 所以一顆音符的標註依步驟拆開：接觸的手、滑行的手與換手各自跟著那一步；信心與備註跟著第一步。
 */
export function partFields(note: Note, part: StepPart): (keyof NoteAnnotation)[] {
  const fields: (keyof NoteAnnotation)[] = part === 'hand' ? ['hand'] : ['track', 'handovers'];
  if (part === firstPart(note)) fields.push('confidence', 'memo');
  else fields.push('trackConfidence');
  return fields;
}

/** 有起點的 Slide：起點與滑行分兩步標，各有自己的信心。 */
function twoSteps(note: Note): boolean {
  return note.kind === 'slide' && note.hasHead;
}

/** 這一步的信心：有起點的 Slide 滑行那一步看 trackConfidence（沒有時沿用 confidence），其他看 confidence。 */
export function stepConfidence(note: Note, mark: NoteAnnotation | undefined, part: StepPart): Confidence {
  if (part === 'track' && twoSteps(note)) return mark?.trackConfidence ?? mark?.confidence ?? 'sure';
  return mark?.confidence ?? 'sure';
}

/** 設定滑行那一步的信心；和起點相同就省略（省略時沿用 confidence）。 */
export function setTrackConfidence(note: Note, mark: NoteAnnotation, value: Confidence | undefined): void {
  if (!twoSteps(note) || value === undefined || value === (mark.confidence ?? 'sure')) delete mark.trackConfidence;
  else mark.trackConfidence = value;
}

/** 標註沒有任何內容（手順、信心、備註都沒有）。 */
export function isEmptyMark(mark: NoteAnnotation): boolean {
  return (
    mark.hand === undefined &&
    mark.track === undefined &&
    !mark.memo &&
    mark.confidence === undefined &&
    mark.trackConfidence === undefined
  );
}

/** 手順欄位（不含信心與備註），比較兩條線是否打法不同時用。 */
export function partHands(mark: NoteAnnotation | undefined, part: StepPart): string {
  if (!mark) return '';
  return part === 'hand' ? (mark.hand ?? '') : JSON.stringify([mark.track ?? '', mark.handovers ?? []]);
}

/**
 * 把一顆音符各步的標註組成一份：source 給出負責那一步的線上的標註。
 * 兩步由同一條線負責時直接回傳那一筆；任一部分還是預填就整顆視為預填。
 */
export function composeMark(
  note: Note,
  source: (part: StepPart) => NoteAnnotation | undefined,
): NoteAnnotation | undefined {
  const need = neededParts(note);
  if (!need.hand || !need.track) return source(need.hand ? 'hand' : 'track');
  const hand = source('hand');
  const track = source('track');
  if (hand === track) return hand;
  if (!hand && !track) return undefined;
  const out: NoteAnnotation = { key: (hand ?? track)!.key };
  if (hand?.hand !== undefined) out.hand = hand.hand;
  if (hand?.confidence !== undefined) out.confidence = hand.confidence;
  if (hand?.memo) out.memo = hand.memo;
  if (track?.track !== undefined) out.track = track.track;
  if (track?.handovers) out.handovers = track.handovers;
  // 滑行的信心以那一條線自己的標註計算，和起點不同時才寫出來。
  if (track) {
    const trackConfidence = stepConfidence(note, track, 'track');
    if (trackConfidence !== (out.confidence ?? 'sure')) out.trackConfidence = trackConfidence;
  }
  const by = hand?.by ?? track?.by;
  if (by) out.by = by;
  if ((hand?.prefilled && out.hand !== undefined) || (track?.prefilled && out.track !== undefined)) out.prefilled = true;
  return isEmptyMark(out) ? undefined : out;
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
  // 滑行的信心和起點不同才有意義；「確定」也要保留（起點不確定、滑行確定）。
  if (isConfidence(raw.trackConfidence) && raw.trackConfidence !== (mark.confidence ?? 'sure')) {
    mark.trackConfidence = raw.trackConfidence;
  }
  if (raw.prefilled === true) mark.prefilled = true;
  if (text(raw.memo)) mark.memo = text(raw.memo);
  if (text(raw.by)) mark.by = text(raw.by);
  return isEmptyMark(mark) ? null : mark;
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

const GROUP_LABEL_MAX = 40;

export function cleanGroups(value: unknown): NoteGroup[] {
  if (!Array.isArray(value)) return [];
  const out: NoteGroup[] = [];
  for (const item of value) {
    if (typeof item !== 'object' || item === null) continue;
    const raw = item as Record<string, unknown>;
    const keys = Array.isArray(raw.keys)
      ? [...new Set(raw.keys.filter((key): key is string => typeof key === 'string' && key !== ''))]
      : [];
    let group: NoteGroup;
    if (keys.length >= 2) group = { keys };
    // v0.4.11 的時間範圍分組，開啟譜面後才換得成音符。
    else if (finite(raw.from) && finite(raw.to)) group = { keys: [], from: Math.min(raw.from, raw.to), to: Math.max(raw.from, raw.to) };
    else continue;
    if (text(raw.label)) group.label = text(raw.label).slice(0, GROUP_LABEL_MAX);
    if (text(raw.by)) group.by = text(raw.by);
    if (!out.some((other) => sameGroup(other, group))) out.push(group);
  }
  return out;
}

function cleanNotes(value: unknown): Record<string, NoteAnnotation> {
  const notes: Record<string, NoteAnnotation> = {};
  if (typeof value !== 'object' || value === null) return notes;
  // 草稿以鍵為索引，標註檔是陣列；兩種都收。
  for (const item of Object.values(value as Record<string, unknown>)) {
    const mark = cleanMark(item);
    if (mark) notes[mark.key] = mark;
  }
  return notes;
}

/**
 * 整理分支清單：缺欄位的丟掉，分出／併入的線找不到或互相繞圈的改接主線，
 * 並依被參照的線在前的順序排好。
 */
function cleanBranches(value: unknown): AnnotationBranch[] {
  if (!Array.isArray(value)) return [];
  const list: AnnotationBranch[] = [];
  const ids = new Set<string>();
  for (const item of value) {
    if (typeof item !== 'object' || item === null) continue;
    const raw = item as Record<string, unknown>;
    const id = text(raw.id);
    if (!id || id === MAIN_LINE || ids.has(id) || !finite(raw.from) || !finite(raw.to)) continue;
    ids.add(id);
    list.push({
      id,
      name: text(raw.name) || '未命名打法',
      parent: text(raw.parent) || MAIN_LINE,
      merge: text(raw.merge) || text(raw.parent) || MAIN_LINE,
      from: Math.min(raw.from, raw.to),
      to: Math.max(raw.from, raw.to),
      memo: text(raw.memo),
      ...(text(raw.by) ? { by: text(raw.by) } : {}),
      color: isColor(raw.color) ? raw.color : '',
      notes: cleanNotes(raw.notes),
    });
  }
  // 舊資料或別人的檔案沒有顏色：依序補上沒用過的顏色。
  for (const branch of list) {
    if (!branch.color) branch.color = nextLineColor([LINE_COLORS[0], ...list.map((item) => item.color).filter(Boolean)]);
  }
  const byId = new Map(list.map((branch) => [branch.id, branch]));
  for (const branch of list) {
    if (branch.parent !== MAIN_LINE && !byId.has(branch.parent)) branch.parent = MAIN_LINE;
    if (branch.merge !== MAIN_LINE && !byId.has(branch.merge)) branch.merge = branch.parent;
  }
  // 分出與併入的線都能接回主線才算有效；接不回去的（互相繞圈）改接主線。
  const ok = new Set([MAIN_LINE]);
  let progress = true;
  while (progress) {
    progress = false;
    for (const branch of list) {
      if (ok.has(branch.id) || !ok.has(branch.parent) || !ok.has(branch.merge)) continue;
      ok.add(branch.id);
      progress = true;
    }
  }
  for (const branch of list) {
    if (ok.has(branch.id)) continue;
    branch.parent = MAIN_LINE;
    branch.merge = MAIN_LINE;
  }
  return sortBranches(list);
}

/** 每條分支的層級：主線為 0，分支比它分出與併入的線都深一層。 */
export function branchDepths(list: AnnotationBranch[]): Map<string, number> {
  const byId = new Map(list.map((branch) => [branch.id, branch]));
  const depths = new Map<string, number>([[MAIN_LINE, 0]]);
  const visit = (id: string, seen: Set<string>): number => {
    const known = depths.get(id);
    if (known !== undefined) return known;
    const branch = byId.get(id);
    if (!branch || seen.has(id)) return 0;
    seen.add(id);
    const depth = 1 + Math.max(visit(branch.parent, seen), visit(branch.merge, seen));
    depths.set(id, depth);
    return depth;
  };
  for (const branch of list) visit(branch.id, new Set());
  return depths;
}

/** 被參照的線在前（層級淺的先）、同層依開始時間排序。 */
export function sortBranches(list: AnnotationBranch[]): AnnotationBranch[] {
  const depths = branchDepths(list);
  const depth = (branch: AnnotationBranch) => depths.get(branch.id) ?? 0;
  return [...list].sort((a, b) => depth(a) - depth(b) || a.from - b.from || a.to - b.to);
}

/** 從資料庫讀回的草稿；格式不對時回傳 null。 */
export function cleanDraft(value: unknown): AnnotationDraft | null {
  if (typeof value !== 'object' || value === null) return null;
  const raw = value as Record<string, unknown>;
  const notes = cleanNotes(raw.notes);
  const video = cleanLink(raw.video);
  const branches = cleanBranches(raw.branches);
  const active = text(raw.active);
  return {
    title: text(raw.title),
    annotators: Array.isArray(raw.annotators) ? raw.annotators.filter((a): a is string => typeof a === 'string' && a !== '') : [],
    memo: text(raw.memo),
    notes,
    ranges: cleanRanges(raw.ranges),
    groups: cleanGroups(raw.groups),
    updatedAt: finite(raw.updatedAt) ? raw.updatedAt : 0,
    ...(video ? { video } : {}),
    branches,
    active: branches.some((branch) => branch.id === active) ? active : MAIN_LINE,
    mainColor: isColor(raw.mainColor) ? raw.mainColor : LINE_COLORS[0],
  };
}

// ---- 打法分支與路線 ----
//
// 每條分支只管 from～to 這段；選定一條線就等於選了一條路線：
// 在它的範圍內用它的手順，範圍之前沿著分出的線、之後沿著併入的線繼續找，最後回到主線。

const TIME_EPS = 1e-4;

export function inBranch(branch: { from: number; to: number }, time: number): boolean {
  return time >= branch.from - TIME_EPS && time <= branch.to + TIME_EPS;
}

/**
 * 走 line 這條路線時，這個時間的步驟歸哪條線：在它的範圍內就是它，
 * 範圍之前沿著分出的線、之後沿著併入的線繼續找，最後回到主線。
 */
export function lineAt(branches: AnnotationBranch[], line: string, time: number): string {
  const seen = new Set<string>();
  let cursor = line;
  while (cursor !== MAIN_LINE && !seen.has(cursor)) {
    seen.add(cursor);
    const branch = branches.find((item) => item.id === cursor);
    if (!branch) return MAIN_LINE;
    if (inBranch(branch, time)) return cursor;
    cursor = time < branch.from ? branch.parent : branch.merge;
  }
  return MAIN_LINE;
}

/** 這條線（或它往下接的線）是否依賴 id：分出或併入的路上會經過 id。用來避免設定成繞圈。 */
export function dependsOn(branches: AnnotationBranch[], line: string, id: string): boolean {
  const seen = new Set<string>();
  const stack = [line];
  while (stack.length > 0) {
    const cursor = stack.pop()!;
    if (cursor === id) return true;
    if (cursor === MAIN_LINE || seen.has(cursor)) continue;
    seen.add(cursor);
    const branch = branches.find((item) => item.id === cursor);
    if (branch) stack.push(branch.parent, branch.merge);
  }
  return false;
}

/** 某條線自己的手順表。 */
export function lineMarks(draft: AnnotationDraft, id: string): Record<string, NoteAnnotation> | undefined {
  if (id === MAIN_LINE) return draft.notes;
  return draft.branches.find((branch) => branch.id === id)?.notes;
}

/** 沿著某條路線，這顆音符這一步的標註（負責那一步的線上的那一筆）。 */
export function stepSource(
  draft: AnnotationDraft,
  line: string,
  note: Note,
  part: StepPart,
): NoteAnnotation | undefined {
  if (!note.key) return undefined;
  return lineMarks(draft, lineAt(draft.branches, line, stepTime(note, part)))?.[note.key];
}

/** 走某條路線時每顆音符的手順（依步驟組合成單一份，用於進度、比對與送給 Rust）。 */
export function routeMarks(draft: AnnotationDraft, notes: Note[], active = draft.active): Record<string, NoteAnnotation> {
  const out: Record<string, NoteAnnotation> = {};
  for (const note of notes) {
    if (!note.key) continue;
    const mark = composeMark(note, (part) => stepSource(draft, active, note, part));
    if (mark) out[note.key] = mark;
  }
  return out;
}

export function lineName(draft: AnnotationDraft, id: string): string {
  if (id === MAIN_LINE) return MAIN_NAME;
  return draft.branches.find((branch) => branch.id === id)?.name ?? MAIN_NAME;
}

/** 新分支的預設名稱：打法 B、C、D…（主線視為 A）。 */
export function nextBranchName(branches: AnnotationBranch[]): string {
  const used = new Set(branches.map((branch) => branch.name));
  for (let index = 1; index < 26; index += 1) {
    const name = `打法 ${String.fromCharCode(65 + index)}`;
    if (!used.has(name)) return name;
  }
  return `打法 ${branches.length + 2}`;
}

export function newBranchId(): string {
  return `b${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/**
 * 分支圖的軌道：主線在第 0 軌，分支放在分出與併入的線右邊第一條時間上不重疊的軌道。
 * 和 git 分支圖一樣，同時存在的分支並排，錯開的分支共用軌道。
 * extent 可給出分支在畫面上實際延伸的時間（標註清單要算進 Slide 較晚的滑行步驟）。
 */
export function assignLanes(
  branches: AnnotationBranch[],
  extent: (branch: AnnotationBranch) => [number, number] = (branch) => [branch.from, branch.to],
): Map<string, number> {
  const lanes = new Map<string, number>([[MAIN_LINE, 0]]);
  const placed: AnnotationBranch[] = [];
  for (const branch of sortBranches(branches)) {
    let lane = Math.max(lanes.get(branch.parent) ?? 0, lanes.get(branch.merge) ?? 0) + 1;
    const [from, to] = extent(branch);
    const clash = (value: number) =>
      placed.some((other) => {
        if (lanes.get(other.id) !== value) return false;
        const [otherFrom, otherTo] = extent(other);
        return otherFrom <= to + TIME_EPS && from <= otherTo + TIME_EPS;
      });
    while (clash(lane)) lane += 1;
    lanes.set(branch.id, lane);
    placed.push(branch);
  }
  return lanes;
}

/** 一格分支圖：這一列上某條軌道怎麼畫。 */
export interface LaneCell {
  lane: number;
  /** 這條軌道是哪條線 */
  line: string;
  /** line 直線穿過；start 從分出的線分出；end 併入併入的線；single 同一列分出又併入 */
  kind: 'line' | 'start' | 'end' | 'single';
  /** 分出的線的軌道 */
  from: number;
  /** 併入的線的軌道 */
  to: number;
  /** 路線在這一列走這條軌道 */
  on: boolean;
}

export interface LaneRow {
  cells: LaneCell[];
  /** 路線在這一列停靠的軌道與那條線 */
  stop: number;
  stopLine: string;
}

/** 一條分支在圖上連續出現的列：[第一列, 最後一列]（含）。 */
export type LaneRun = [number, number];

/**
 * 依各列音符的時間，找出每條分支從第幾列畫到第幾列：一條分支是一整條連續的線。
 * 標註清單依步驟時間排序，分支開始前就在滑的 Slide，滑行步驟會夾在分支這段裡；
 * 那幾列分支線照樣經過，停靠點則畫在實際負責的線上（見 laneRows 的 owners）。
 */
export function spansByTime(times: number[], branches: AnnotationBranch[]): Map<string, LaneRun[]> {
  const spans = new Map<string, LaneRun[]>();
  for (const branch of branches) {
    let first = -1;
    let last = -1;
    times.forEach((time, index) => {
      if (!inBranch(branch, time)) return;
      if (first < 0) first = index;
      last = index;
    });
    if (first >= 0) spans.set(branch.id, [[first, last]]);
  }
  return spans;
}

/**
 * 把分支範圍轉成每一列的圖。times 是每一列的時間，tracks 是每一列路線走哪條線（決定上色），
 * owners 可另外指定每一列的停靠點畫在哪條線（省略時同 tracks）。
 *
 * 分出與併入的曲線要接在「那一列交界處真的畫著的線」上：設定的線在那裡還沒開始、已經結束，
 * 或因為清單篩選沒有列可畫時，就沿著它的分出／併入方向往下找，最後一定接得到主線，不會畫出懸空的線。
 */
export function laneRows(
  times: number[],
  runs: Map<string, LaneRun[]>,
  branches: AnnotationBranch[],
  lanes: Map<string, number>,
  tracks: string[],
  owners?: string[],
): LaneRow[] {
  const byId = new Map(branches.map((branch) => [branch.id, branch]));
  const count = times.length;
  const spanOf = (id: string) => runs.get(id)?.[0];
  /** 沿著 line 找第 index 列上緣（edge='top'）或下緣（'bottom'）畫著的線。 */
  const present = (line: string, index: number, edge: 'top' | 'bottom'): string => {
    const seen = new Set<string>();
    let cursor = line;
    while (cursor !== MAIN_LINE && !seen.has(cursor)) {
      seen.add(cursor);
      const branch = byId.get(cursor);
      if (!branch) return MAIN_LINE;
      const span = spanOf(cursor);
      if (span && (edge === 'top' ? span[0] < index && index <= span[1] : span[0] <= index && index < span[1])) {
        return cursor;
      }
      // 還沒開始就往分出的線找，已經結束就往併入的線找；沒有列可畫時看時間。
      const before = span ? (edge === 'top' ? index <= span[0] : index < span[0]) : times[index] < branch.from;
      cursor = before ? branch.parent : branch.merge;
    }
    return MAIN_LINE;
  };
  /**
   * 這一列真的畫著的線：路線圖上同一時間可能有好幾站，路線要走的分支可能還沒畫到（分出站在後面）
   * 或已經畫完；這時沿分出／併入的線找，停靠點與上色才不會落在沒有線的地方。
   */
  const drawn = (line: string, index: number): string => {
    const seen = new Set<string>();
    let cursor = line;
    while (cursor !== MAIN_LINE && !seen.has(cursor)) {
      seen.add(cursor);
      const branch = byId.get(cursor);
      if (!branch) return MAIN_LINE;
      const span = spanOf(cursor);
      if (span && index >= span[0] && index <= span[1]) return cursor;
      cursor = (span ? index < span[0] : times[index] < branch.from) ? branch.parent : branch.merge;
    }
    return MAIN_LINE;
  };
  const rows: LaneRow[] = [];
  for (let index = 0; index < count; index += 1) {
    const runAt = (id: string) => runs.get(id)?.find((run) => index >= run[0] && index <= run[1]);
    const track = drawn(tracks[index] ?? MAIN_LINE, index);
    const owner = drawn(owners?.[index] ?? track, index);
    const cells: LaneCell[] = [{ lane: 0, line: MAIN_LINE, kind: 'line', from: 0, to: 0, on: track === MAIN_LINE }];
    for (const id of runs.keys()) {
      const span = runAt(id);
      if (!span) continue;
      const kind = span[0] === span[1] ? 'single' : index === span[0] ? 'start' : index === span[1] ? 'end' : 'line';
      const branch = byId.get(id);
      const starts = kind === 'start' || kind === 'single';
      const ends = kind === 'end' || kind === 'single';
      cells.push({
        lane: lanes.get(id) ?? 0,
        line: id,
        kind,
        from: starts ? (lanes.get(present(branch?.parent ?? MAIN_LINE, index, 'top')) ?? 0) : 0,
        to: ends ? (lanes.get(present(branch?.merge ?? MAIN_LINE, index, 'bottom')) ?? 0) : 0,
        on: track === id,
      });
    }
    rows.push({ cells, stop: lanes.get(owner) ?? 0, stopLine: owner });
  }
  return rows;
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

function stamp(value: number): string {
  return value > 0 ? new Date(value).toISOString() : '';
}

/** 音符穩定鍵開頭的秒數；沒有解析結果時用來依時間排列。 */
function keyTime(key: string): number {
  const time = Number.parseFloat(key);
  return Number.isFinite(time) ? time : Infinity;
}

/**
 * 草稿轉成標註檔；只輸出有內容的音符，依譜面順序排列。
 * keys 是依譜面順序的音符穩定鍵（Rust 解析結果）；拿不到時傳 null，改依鍵開頭的秒數排列、
 * noteCount 記 0。
 */
export function toFile(
  draft: AnnotationDraft,
  chart: { source: string; sha256: string; firstSeconds: number; keys: string[] | null },
): HandAnnotation {
  const order = new Map((chart.keys ?? []).map((key, index) => [key, index]));
  const rank = (key: string) => order.get(key) ?? (chart.keys ? Infinity : keyTime(key));
  const video = linkToFile(draft.video);
  const sorted = (marks: Record<string, NoteAnnotation>) =>
    Object.values(marks)
      .sort((a, b) => rank(a.key) - rank(b.key) || (a.key < b.key ? -1 : 1))
      .map((mark) => ({ ...mark }));
  const branches: AnnotationBranchFile[] = draft.branches.map((branch) => ({
    id: branch.id,
    name: branch.name,
    parent: branch.parent,
    ...(branch.merge !== branch.parent ? { merge: branch.merge } : {}),
    from: branch.from,
    to: branch.to,
    ...(branch.memo ? { memo: branch.memo } : {}),
    ...(branch.by ? { by: branch.by } : {}),
    color: branch.color,
    notes: sorted(branch.notes),
  }));
  return {
    format: ANNOTATION_FORMAT,
    version: ANNOTATION_VERSION,
    title: draft.title,
    annotators: [...draft.annotators],
    updatedAt: stamp(draft.updatedAt),
    chart: {
      sha256: chart.sha256,
      firstSeconds: chart.firstSeconds,
      noteCount: chart.keys?.length ?? 0,
      source: chart.source,
    },
    memo: draft.memo,
    notes: sorted(draft.notes),
    ranges: draft.ranges.map((range) => ({ ...range })),
    ...(draft.groups.length > 0 ? { groups: draft.groups.map((group) => ({ ...group, keys: [...group.keys] })) } : {}),
    ...(video ? { video } : {}),
    ...(branches.length > 0 ? { branches } : {}),
  };
}

/**
 * 標註檔文字：一般 JSON，但每顆音符與每段備註各佔一行，原譜放在最後，
 * 方便貼上、閱讀與用 diff 比對不同人的標註。
 */
export function serialize(file: HandAnnotation): string {
  const line = (value: unknown) => JSON.stringify(value);
  const list = (items: unknown[], indent = '  ') =>
    items.length === 0 ? '[]' : `[\n${items.map((item) => `${indent}  ${line(item)}`).join(',\n')}\n${indent}]`;
  // 分支的欄位放一行，手順同樣每顆一行。
  const branchList = (items: AnnotationBranchFile[]) =>
    `[\n${items
      .map(({ notes, ...head }) => `    ${line(head).slice(0, -1)}, "notes": ${list(notes, '    ')}}`)
      .join(',\n')}\n  ]`;
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
    ...(file.groups && file.groups.length > 0 ? [`  "groups": ${list(file.groups)},`] : []),
    `  "notes": ${list(file.notes)},`,
    ...(file.branches && file.branches.length > 0 ? [`  "branches": ${branchList(file.branches)},`] : []),
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
  if (isRecordObject(raw) && raw.format === 'maimotion-hand-annotation-bundle') {
    return { ok: false, error: '這是多份譜面的整包標記檔，請在「新增」視窗貼上或用「開啟標記檔」匯入。' };
  }
  return parseValue(raw);
}

function isRecordObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/** 已經 JSON.parse 過的標註檔（整包標註檔裡的每一份也用這個）。 */
export function parseValue(raw: unknown): ParseResult {
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
  const branches: AnnotationBranchFile[] = cleanBranches(data.branches).map(({ notes: marks, memo, merge: into, ...branch }) => ({
    ...branch,
    ...(into !== branch.parent ? { merge: into } : {}),
    ...(memo ? { memo } : {}),
    notes: Object.values(marks),
  }));
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
      ...(cleanGroups(data.groups).length > 0 ? { groups: cleanGroups(data.groups) } : {}),
      ...(video ? { video } : {}),
      ...(branches.length > 0 ? { branches } : {}),
    },
  };
}

/** 同樣的音符視為同一組；舊版時間範圍的分組比兩端（1 毫秒內）。 */
export function sameGroup(a: NoteGroup, b: NoteGroup): boolean {
  if (a.keys.length > 0 || b.keys.length > 0) {
    const set = new Set(a.keys);
    return a.keys.length === b.keys.length && b.keys.every((key) => set.has(key));
  }
  return Math.abs((a.from ?? 0) - (b.from ?? 0)) < 1e-3 && Math.abs((a.to ?? 0) - (b.to ?? 0)) < 1e-3;
}

export function sameHands(a: NoteAnnotation, b: NoteAnnotation): boolean {
  const handovers = (m: NoteAnnotation) => JSON.stringify(m.handovers ?? []);
  return a.hand === b.hand && a.track === b.track && handovers(a) === handovers(b);
}

export interface MergeResult {
  draft: AnnotationDraft;
  /** 新增的音符標註數 */
  added: number;
  /** 手順不同的音符鍵；fill 模式保留自己的，replace 模式採用匯入的 */
  conflicts: string[];
  /** 新加入的打法分支數 */
  branches: number;
}

/** 把對方一條線的手順合併進 marks（就地修改）。 */
function mergeMarks(
  marks: Record<string, NoteAnnotation>,
  theirs: NoteAnnotation[],
  mode: 'fill' | 'replace',
): { added: number; conflicts: string[] } {
  let added = 0;
  const conflicts: string[] = [];
  for (const item of theirs) {
    const current = marks[item.key];
    if (!current || !isHumanLabeled(current)) {
      if (!item.prefilled || !current) {
        marks[item.key] = { ...item };
        if (isHumanLabeled(item)) added += 1;
      }
      continue;
    }
    if (!isHumanLabeled(item)) continue;
    const different = !sameHands(current, item);
    if (different) conflicts.push(item.key);
    const memo = [current.memo, item.memo].filter((value, index, all) => value && all.indexOf(value) === index).join(' / ');
    if (mode === 'replace' && different) {
      marks[item.key] = { ...item, ...(memo ? { memo } : {}) };
    } else if (memo && memo !== current.memo) {
      marks[item.key] = { ...current, memo };
    }
  }
  return { added, conflicts };
}

/**
 * 合併匯入的標註。
 * - fill：只補自己沒標（或只是預填）的音符，手順不同的保留自己的並列為衝突。
 * - replace：匯入的為準，覆蓋同一顆音符。
 * 兩種模式的備註都會保留雙方內容，區段備註與標註者名單取聯集。
 * 打法分支以 id 對應：同一條分支照同樣規則合併手順，自己沒有的整條加入。
 */
export function merge(mine: AnnotationDraft, file: HandAnnotation, mode: 'fill' | 'replace'): MergeResult {
  const draft: AnnotationDraft = {
    ...mine,
    notes: { ...mine.notes },
    ranges: [...mine.ranges],
    groups: [...mine.groups],
    annotators: [...mine.annotators],
    branches: mine.branches.map((branch) => ({ ...branch, notes: { ...branch.notes } })),
  };
  const main = mergeMarks(draft.notes, file.notes, mode);
  let added = main.added;
  const conflicts = [...main.conflicts];
  let branches = 0;
  for (const theirs of file.branches ?? []) {
    const current = draft.branches.find((branch) => branch.id === theirs.id);
    if (!current) {
      const notes: Record<string, NoteAnnotation> = {};
      for (const mark of theirs.notes) notes[mark.key] = { ...mark };
      draft.branches.push({
        ...theirs,
        merge: theirs.merge ?? theirs.parent,
        memo: theirs.memo ?? '',
        color: theirs.color ?? '',
        notes,
      });
      branches += 1;
      continue;
    }
    const result = mergeMarks(current.notes, theirs.notes, mode);
    added += result.added;
    conflicts.push(...result.conflicts.map((key) => `${current.id}:${key}`));
    if (theirs.memo && !current.memo.includes(theirs.memo)) {
      current.memo = current.memo ? `${current.memo}\n\n${theirs.memo}` : theirs.memo;
    }
  }
  // parent 可能是對方檔案裡才有的分支；重新整理確保都接得回主線。
  draft.branches = cleanBranches(draft.branches);
  for (const range of file.ranges) {
    const exists = draft.ranges.some(
      (item) => item.from === range.from && item.to === range.to && item.memo === range.memo,
    );
    if (!exists) draft.ranges.push({ ...range });
  }
  draft.ranges.sort((a, b) => a.from - b.from);
  for (const group of file.groups ?? []) {
    const same = draft.groups.find((item) => sameGroup(item, group));
    if (!same) draft.groups.push({ ...group, keys: [...group.keys] });
    else if (!same.label && group.label) same.label = group.label;
  }
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
  return { draft, added, conflicts, branches };
}

/** 標註進度：需要標的音符中，已由人確認的數量；marks 通常是目前路線展開後的手順。 */
export function progress(
  marks: Record<string, NoteAnnotation>,
  notes: Note[],
): { done: number; total: number; prefilled: number } {
  let done = 0;
  let prefilled = 0;
  for (const note of notes) {
    const mark = note.key ? marks[note.key] : undefined;
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
