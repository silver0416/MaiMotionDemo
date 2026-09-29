// 真人手順標註的編輯狀態：跟著目前開啟的譜面紀錄，改動後自動寫回紀錄。

import { evaluateAnnotation, nextRequestId } from '../lib/api';
import {
  type AnnotationBranch,
  type AnnotationDraft,
  cleanDraft,
  composeMark,
  emptyDraft,
  firstPart,
  inBranch,
  isEmptyMark,
  isHumanLabeled,
  dependsOn,
  lineAt,
  lineMarks,
  lineName,
  MAIN_LINE,
  merge,
  modelMark,
  nextLineColor,
  neededParts,
  newBranchId,
  nextBranchName,
  partFields,
  partHands,
  routeMarks,
  serialize,
  setTrackConfidence,
  sortBranches,
  stepConfidence,
  stepSource,
  stepTime,
  toFile,
  type StepPart,
  type MergeResult,
} from '../lib/annotation';
import { projectConfig } from '../lib/contract';
import { hashText } from '../lib/db';
import { recallAlignment, rememberAlignment, type VideoLink } from '../lib/video';
import type {
  Confidence,
  EvaluateResponse,
  Hand,
  HandAnnotation,
  HandoverMark,
  Note,
  NoteAnnotation,
  TrackHand,
} from '../lib/types';
import { playback } from './playback.svelte';
import { recordTitle, records, type ChartRecord } from './records.svelte';
import { session } from './session.svelte';

const ANNOTATOR_KEY = 'maimotion.annotator.v1';
const ADVANCE_KEY = 'maimotion.annotate-advance.v1';
const SAVE_DELAY = 400;

function readLocal(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeLocal(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 無法保存只影響下次啟動的預設值。
  }
}

export type ListFilter = 'all' | 'todo' | 'prefilled' | 'diff' | 'memo' | 'unsure';

/** 標註的一步：一般音符只有一步；有起點的 Slide 分成起點觸碰與開始滑行兩步。 */
export type { StepPart };

export interface Step {
  key: string;
  note: Note;
  part: StepPart;
  /** 這一步的時間：起點為判定時間，滑行為開始移動的時間 */
  time: number;
}

/** 標註檔附的原譜和它記錄的雜湊不符時回傳錯誤訊息；沒問題回傳 null。 */
export async function checkSource(file: HandAnnotation): Promise<string | null> {
  if (!file.chart.sha256 || file.chart.sha256 === (await hashText(file.chart.source))) return null;
  return '標註檔裡的原譜被改過（雜湊不符），為避免對錯音符已停止匯入。';
}

/** 這一步是否已有人確認（不是預填）。 */
export function isStepDone(step: Step, mark: NoteAnnotation | undefined): boolean {
  if (!mark || mark.prefilled) return false;
  return step.part === 'hand' ? mark.hand !== undefined : mark.track !== undefined;
}

export class AnnotationStore {
  /** 目前紀錄的草稿。 */
  draft = $state<AnnotationDraft>(emptyDraft());
  /** 草稿屬於哪一筆紀錄；null 表示沒有開啟的紀錄。 */
  recordId = $state<string | null>(null);
  annotator = $state(readLocal(ANNOTATOR_KEY) ?? '');
  /** 標完一顆後自動跳到下一顆。 */
  autoAdvance = $state(readLocal(ADVANCE_KEY) !== 'off');
  filter = $state<ListFilter>('all');

  evaluation = $state<EvaluateResponse | null>(null);
  evaluating = $state(false);
  evaluationError = $state<string | null>(null);

  #saveTimer: ReturnType<typeof setTimeout> | undefined;

  /** 依時間排序的音符（同時音維持譜面順序）。 */
  ordered = $derived<Note[]>(
    [...session.notes].sort((a, b) => a.timeSeconds - b.timeSeconds || Number(a.id.slice(1)) - Number(b.id.slice(1))),
  );

  /** 依時間排序的標註步驟。 */
  steps = $derived.by<Step[]>(() => {
    const out: (Step & { order: number })[] = [];
    this.ordered.forEach((note, order) => {
      const need = neededParts(note);
      if (need.hand) out.push({ key: `${note.id}:hand`, note, part: 'hand', time: note.timeSeconds, order });
      if (need.track) {
        const time = stepTime(note, 'track');
        out.push({ key: `${note.id}:track`, note, part: 'track', time, order });
      }
    });
    out.sort((a, b) => a.time - b.time || a.order - b.order || (a.part === 'hand' ? -1 : 1));
    return out;
  });

  #stepIndex = $derived(new Map(this.steps.map((step, index) => [step.key, index])));

  /** 目前選到的是這顆音符的哪一步；選到別顆音符時回到它的第一步。 */
  #part = $state<{ noteId: string; part: StepPart } | null>(null);

  /** 目前的標註步驟：選取的音符＋步驟。 */
  get currentStep(): Step | null {
    const note = session.selectedNote;
    if (!note) return null;
    const index = this.#indexOf(note, this.#part?.noteId === note.id ? this.#part.part : undefined);
    return index === undefined ? null : this.steps[index];
  }

  #indexOf(note: Note, part?: StepPart): number | undefined {
    if (part) {
      const index = this.#stepIndex.get(`${note.id}:${part}`);
      if (index !== undefined) return index;
    }
    return this.#stepIndex.get(`${note.id}:hand`) ?? this.#stepIndex.get(`${note.id}:track`);
  }

  /** 分析結果缺少穩定鍵（舊快取）時無法標註。 */
  keysReady = $derived(session.notes.length > 0 && session.notes.every((note) => !!note.key));

  /** 開啟的紀錄換了：載入它的草稿。同一筆紀錄不重載，編輯中的內容不會被蓋掉。 */
  bind(record: ChartRecord | null): void {
    if ((record?.id ?? null) === this.recordId) return;
    this.flush();
    this.recordId = record?.id ?? null;
    this.pendingStart = null;
    this.draft = (record && cleanDraft(record.annotation)) ?? emptyDraft(record ? recordTitle(record) : '');
    if (record && !this.draft.title) this.draft.title = recordTitle(record);
    if (this.#syncAlignment()) this.#scheduleSave();
    this.evaluation = null;
    this.evaluationError = null;
  }

  setAnnotator(name: string): void {
    this.annotator = name.trim();
    writeLocal(ANNOTATOR_KEY, this.annotator);
  }

  setAutoAdvance(on: boolean): void {
    this.autoAdvance = on;
    writeLocal(ADVANCE_KEY, on ? 'on' : 'off');
  }

  #touch(): void {
    this.draft.updatedAt = Date.now();
    if (this.annotator && !this.draft.annotators.includes(this.annotator)) {
      this.draft.annotators.push(this.annotator);
    }
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => this.flush(), SAVE_DELAY);
  }

  /** 立即寫回紀錄（切換紀錄或匯出前呼叫）。 */
  flush(): void {
    clearTimeout(this.#saveTimer);
    this.#saveTimer = undefined;
    if (!this.recordId || this.draft.updatedAt === 0) return;
    records.setAnnotation(this.recordId, $state.snapshot(this.draft) as AnnotationDraft);
  }

  // ---- 打法路線 ----
  //
  // 分支以「步驟」為單位：有起點的 Slide，起點與滑行可以由不同的線負責。
  // 讀取時把各步的標註組成一顆（composeMark），寫入時依欄位拆回負責那一步的線。

  /** 目前路線：選定的線一路往上到主線。 */

  /** 目前路線展開後的手順（進度、篩選、比對都看這份）。 */
  routed = $derived<Record<string, NoteAnnotation>>(routeMarks(this.draft, this.ordered));

  /** 這一步在目前路線上歸哪條線管；編輯會寫到那條線。 */
  lineOfStep(note: Note, part: StepPart): string {
    return lineAt(this.draft.branches, this.draft.active, stepTime(note, part));
  }

  /** 這顆音符第一步歸哪條線。 */
  lineOf(note: Note): string {
    return this.lineOfStep(note, firstPart(note));
  }

  lineName(id: string): string {
    return lineName(this.draft, id);
  }

  branch(id: string): AnnotationBranch | undefined {
    return this.draft.branches.find((item) => item.id === id);
  }

  lineColor(id: string): string {
    if (id === MAIN_LINE) return this.draft.mainColor;
    return this.branch(id)?.color ?? this.draft.mainColor;
  }

  setLineColor(id: string, color: string): void {
    if (id === MAIN_LINE) this.draft.mainColor = color;
    else {
      const branch = this.branch(id);
      if (!branch) return;
      branch.color = color;
    }
    this.#scheduleSave();
  }

  /** 分支圖上滑鼠停著的線；所有列一起用那條線的顏色預覽。 */
  hoverLine = $state<string | null>(null);

  /** 音符的顆數編號（依時間排序，從 1 開始）。 */
  #numbers = $derived(new Map(this.ordered.map((note, index) => [note.id, index + 1])));
  #byKey = $derived(new Map(this.ordered.map((note) => [note.key ?? '', note])));

  noteNumber(note: Note): number {
    return this.#numbers.get(note.id) ?? 0;
  }

  /** 步驟的編號：一般為顆數，有起點的 Slide 滑行那一步前面加「↳」。 */
  stepRef(step: { note: Note; part: StepPart }): string {
    const number = this.noteNumber(step.note);
    return step.part === 'track' && step.note.hasHead ? `↳${number}` : String(number);
  }

  /**
   * 讀使用者輸入的步驟編號：「64」是第 64 顆（當起點取它的第一步、當終點取最後一步），
   * 「↳64」或「64滑」指定 Slide 的滑行那一步。
   */
  parseStepRef(text: string, boundary: 'start' | 'end'): Step | null {
    const match = /^\s*(↳|>)?\s*(\d+)\s*(滑行|滑)?\s*$/.exec(text);
    if (!match) return null;
    const number = Number(match[2]);
    const note = this.ordered[number - 1];
    if (!note) return null;
    const mine = this.steps.filter((step) => step.note.id === note.id);
    if (mine.length === 0) return null;
    if (match[1] || match[3]) return mine.find((step) => step.part === 'track') ?? null;
    return boundary === 'start' ? mine[0] : mine[mine.length - 1];
  }

  /** 範圍內的步驟（依清單順序）。 */
  stepsIn(from: number, to: number): Step[] {
    return this.steps.filter((step) => inBranch({ from, to }, step.time));
  }

  /** 範圍的文字，例如「第 ↳64–67 顆」。 */
  rangeText(from: number, to: number): string {
    const steps = this.stepsIn(from, to);
    if (steps.length === 0) return '沒有音符';
    const first = this.stepRef(steps[0]);
    const last = this.stepRef(steps[steps.length - 1]);
    return first === last ? `第 ${first} 顆` : `第 ${first}–${last} 顆`;
  }

  /** 這一步有哪些打法可以選：主線加上範圍涵蓋它的分支（parent 在前）。 */
  linesAt(step: Step): string[] {
    return [MAIN_LINE, ...this.draft.branches.filter((branch) => inBranch(branch, step.time)).map((b) => b.id)];
  }

  /** 在這一步的各種打法之間輪流切換（快捷鍵 B／Shift+B）。 */
  cycleLine(step: Step, direction: 1 | -1): void {
    const lines = this.linesAt(step);
    if (lines.length < 2) return;
    const index = lines.indexOf(this.lineOfStep(step.note, step.part));
    this.setActive(lines[(index + direction + lines.length) % lines.length]);
  }

  // ---- 標記新分支：先標起點，再選終點（以步驟為單位） ----

  /** 已標為新分支起點的步驟 key；null 表示不在標記模式。 */
  pendingStart = $state<string | null>(null);

  get pendingStep(): Step | null {
    if (!this.pendingStart) return null;
    const index = this.#stepIndex.get(this.pendingStart);
    return index === undefined ? null : this.steps[index];
  }

  /** 新分支要從哪條線分出：標起點那一刻，目前路線在起點走的線。 */
  pendingFrom = $state<string | null>(null);

  /**
   * 標起點時記下目前路線在這一步走的線（之後切換路線也不變），並選取它，
   * 之後點選的另一步才算終點候選。
   */
  markStart(step: Step): void {
    this.pendingStart = step.key;
    this.pendingFrom = this.lineOfStep(step.note, step.part);
    if (this.currentStep?.key !== step.key) this.selectStep(step);
  }

  cancelMark(): void {
    this.pendingStart = null;
    this.pendingFrom = null;
  }

  /** 標記中的新分支會從哪條線分出。 */
  get pendingFromLine(): string {
    const start = this.pendingStep;
    if (!start) return MAIN_LINE;
    return this.pendingFrom ?? this.lineOfStep(start.note, start.part);
  }

  /**
   * 以標好的起點和這一步終點建立分支（前後順序不拘）：
   * 從標起點時的路線分出，併入現在路線在終點走的線（標完起點後切換路線，就能併入別條線）。
   */
  finishMark(end: Step, name?: string): AnnotationBranch | null {
    const start = this.pendingStep;
    if (!start) return null;
    const [first, last] = start.time <= end.time ? [start, end] : [end, start];
    const branch = this.createBranch(first.time, last.time, name, {
      parent: start === first ? this.pendingFromLine : undefined,
      merge: this.lineOfStep(last.note, last.part),
    });
    if (branch) this.cancelMark();
    return branch;
  }

  /** 在這一步可以接的線：這一步所在範圍內的線（主線永遠可以）；exclude 的分支與依賴它的線不列入。 */
  linesAtTime(time: number, exclude?: string): string[] {
    return [
      MAIN_LINE,
      ...this.draft.branches
        .filter((branch) => inBranch(branch, time))
        .filter((branch) => !exclude || !dependsOn(this.draft.branches, branch.id, exclude))
        .map((branch) => branch.id),
    ];
  }

  // ---- 讀寫標註 ----

  /** 目前路線上負責這一步的線，它對這顆音符的那一筆標註。 */
  #source(note: Note, part: StepPart): NoteAnnotation | undefined {
    return stepSource(this.draft, this.draft.active, note, part);
  }

  /** 這一步自己的標註（是否預填、是否已確認看這一筆）。 */
  stepMark(step: { note: Note; part: StepPart }): NoteAnnotation | undefined {
    return this.#source(step.note, step.part);
  }

  /** 這顆音符在目前路線上的標註（各步組合而成）。 */
  mark(note: Note | null): NoteAnnotation | undefined {
    return note ? composeMark(note, (part) => this.#source(note, part)) : undefined;
  }

  /** 這顆音符需要的步驟。 */
  #parts(note: Note): StepPart[] {
    const need = neededParts(note);
    return [...(need.hand ? (['hand'] as const) : []), ...(need.track ? (['track'] as const) : [])];
  }

  /**
   * 把 next 中這些欄位寫進某條線的那一筆；原本是預填時整筆轉為已確認，其他欄位的預填一併清掉。
   * 滑行的信心在省略時沿用起點的信心，所以寫完後重新算一次，讓它維持原本（或 next）的值。
   */
  #writeFields(line: string, note: Note, next: NoteAnnotation, fields: (keyof NoteAnnotation)[]): void {
    const marks = lineMarks(this.draft, line);
    const key = note.key;
    if (!marks || !key) return;
    const current = marks[key];
    const out: NoteAnnotation = current ? { ...current } : { key };
    const trackConfidence = fields.includes('track')
      ? stepConfidence(note, next, 'track')
      : current
        ? stepConfidence(note, current, 'track')
        : undefined;
    if (current?.prefilled) {
      // 沒看過的預填不能因為這次編輯就變成真人資料。
      for (const field of ['hand', 'track', 'handovers'] as const) if (!fields.includes(field)) delete out[field];
    }
    const record = out as unknown as Record<string, unknown>;
    for (const field of fields) {
      const value = next[field];
      if (value === undefined) delete record[field];
      else record[field] = value;
    }
    setTrackConfidence(note, out, trackConfidence);
    delete out.prefilled;
    if (this.annotator) out.by = this.annotator;
    if (isEmptyMark(out)) delete marks[key];
    else marks[key] = out;
  }

  /**
   * 修改一顆的標註；只要有人動過就不再是預填。內容清空時移除整筆。
   * 起點與滑行分屬不同的線時，只把有變動的那一步寫回它自己的線。
   */
  #edit(note: Note, change: (mark: NoteAnnotation) => void): void {
    if (!note.key) return;
    const parts = this.#parts(note);
    const lines = parts.map((part) => this.lineOfStep(note, part));
    const current = this.mark(note);
    const next: NoteAnnotation = current ? { ...current } : { key: note.key };
    change(next);
    if (lines.every((line) => line === lines[0])) {
      const fields = parts.flatMap((part) => partFields(note, part));
      this.#writeFields(lines[0], note, next, fields);
    } else {
      parts.forEach((part, index) => {
        const fields = partFields(note, part);
        const changed = fields.some((field) => JSON.stringify(next[field] ?? null) !== JSON.stringify(current?.[field] ?? null));
        if (changed) this.#writeFields(lines[index], note, next, fields);
      });
    }
    this.#touch();
  }

  /**
   * 分步標註時只改一個部分；如果原本是模型預填，另一部分也一起清掉，
   * 留給那一步自己標，免得沒看過的預填被當成真人資料。
   */
  #dropOtherPrefill(note: Note, mark: NoteAnnotation, part: StepPart): void {
    const need = neededParts(note);
    if (!need.hand || !need.track) return;
    const other = part === 'hand' ? 'track' : 'hand';
    if (!this.#source(note, other)?.prefilled) return;
    if (part === 'hand') {
      delete mark.track;
      delete mark.handovers;
    } else {
      delete mark.hand;
    }
  }

  setHand(note: Note, hand: Hand | undefined): void {
    this.#edit(note, (mark) => {
      this.#dropOtherPrefill(note, mark, 'hand');
      if (hand) mark.hand = hand;
      else delete mark.hand;
    });
  }

  setTrack(note: Note, track: TrackHand | undefined): void {
    this.#edit(note, (mark) => {
      this.#dropOtherPrefill(note, mark, 'track');
      if (track) mark.track = track;
      else delete mark.track;
      if (track === 'LR') delete mark.handovers;
    });
  }

  /** 快捷鍵 Shift+A／D：這顆需要的部分都設成同一隻手。 */
  setAll(note: Note, hand: Hand): void {
    const need = neededParts(note);
    this.#edit(note, (mark) => {
      if (need.hand) mark.hand = hand;
      if (need.track) mark.track = hand;
    });
  }

  /** 設定某一步的信心（預設第一步）；有起點的 Slide 起點與滑行分開設定，互不影響。 */
  setConfidence(note: Note, confidence: Confidence, part: StepPart = firstPart(note)): void {
    this.#edit(note, (mark) => {
      if (part === 'track' && part !== firstPart(note)) {
        setTrackConfidence(note, mark, confidence);
        return;
      }
      const track = stepConfidence(note, mark, 'track');
      if (confidence === 'sure') delete mark.confidence;
      else mark.confidence = confidence;
      setTrackConfidence(note, mark, track);
    });
  }

  confidenceOf(note: Note, part: StepPart = firstPart(note)): Confidence {
    return stepConfidence(note, this.mark(note), part);
  }

  cycleConfidence(note: Note, part: StepPart = firstPart(note)): void {
    const order: Confidence[] = ['sure', 'unsure', 'either'];
    const current = this.confidenceOf(note, part);
    this.setConfidence(note, order[(order.indexOf(current) + 1) % order.length], part);
  }

  setMemo(note: Note, memo: string): void {
    this.#edit(note, (mark) => {
      if (memo.trim()) mark.memo = memo;
      else delete mark.memo;
    });
  }

  /** 在 Slide 滑行期間加一次換手；換到目前滑行手的另一隻。 */
  addHandover(note: Note, at: number): void {
    this.#edit(note, (mark) => {
      const list = (mark.handovers ?? [])
        .map((item) => ({ ...item }))
        .filter((item) => Math.abs(item.at - at) > 1e-3);
      const before = [...list].filter((item) => item.at < at).pop();
      const holder: Hand = before?.to ?? (mark.track === 'R' ? 'R' : 'L');
      if (!mark.track || mark.track === 'LR') mark.track = holder;
      list.push({ at, to: holder === 'L' ? 'R' : 'L' });
      list.sort((a, b) => a.at - b.at);
      // 後面的換手依序交替，保持每次都是換到另一隻手。
      let hand: Hand = mark.track as Hand;
      for (const item of list) {
        item.to = hand === 'L' ? 'R' : 'L';
        hand = item.to;
      }
      mark.handovers = list;
    });
  }

  removeHandover(note: Note, index: number): void {
    this.#edit(note, (mark) => {
      const list = (mark.handovers ?? []).map((item) => ({ ...item }));
      list.splice(index, 1);
      let hand: Hand = mark.track === 'R' ? 'R' : 'L';
      for (const item of list) {
        item.to = hand === 'L' ? 'R' : 'L';
        hand = item.to;
      }
      if (list.length > 0) mark.handovers = list;
      else delete mark.handovers;
    });
  }

  clear(note: Note): void {
    if (!note.key || !this.mark(note)) return;
    this.#edit(note, (mark) => {
      delete mark.hand;
      delete mark.track;
      delete mark.handovers;
      delete mark.confidence;
      delete mark.trackConfidence;
      delete mark.memo;
    });
  }

  /** 把這顆各步的預填確認為真人資料（手順不變）；有確認到時回傳 true。 */
  #confirmNote(note: Note): boolean {
    let changed = false;
    for (const part of this.#parts(note)) {
      const line = this.lineOfStep(note, part);
      const marks = lineMarks(this.draft, line);
      const mark = note.key ? marks?.[note.key] : undefined;
      if (!marks || !mark?.prefilled) continue;
      const next = { ...mark };
      delete next.prefilled;
      if (this.annotator) next.by = this.annotator;
      marks[mark.key] = next;
      changed = true;
    }
    return changed;
  }

  confirm(note: Note): void {
    if (this.#confirmNote(note)) this.#touch();
  }

  confirmAll(): number {
    let count = 0;
    for (const note of this.ordered) if (this.#confirmNote(note)) count += 1;
    if (count > 0) this.#touch();
    return count;
  }

  /** 以目前盤面的模型方案預填目前路線上還沒標的步驟；已有標註（含預填）的不動。 */
  prefillFromModel(): number {
    const solution = session.solutions[session.solutionIndex] ?? session.solutions[0];
    if (!solution) return 0;
    let count = 0;
    for (const note of this.ordered) {
      if (!note.key) continue;
      const model = modelMark(solution, note);
      let filled = false;
      for (const part of this.#parts(note)) {
        const marks = lineMarks(this.draft, this.lineOfStep(note, part));
        if (!marks) continue;
        const value = part === 'hand' ? model.hand : model.track;
        if (value === undefined) continue;
        const current = marks[note.key];
        // 那一條線已經有這顆的標註就不動（包含其他步的真人資料）。
        if (current && !current.prefilled) continue;
        const out: NoteAnnotation = current ? { ...current } : { key: note.key };
        if (part === 'hand') out.hand = model.hand;
        else {
          out.track = model.track;
          if (model.handovers) out.handovers = model.handovers;
        }
        if (current && (part === 'hand' ? current.hand : current.track) !== undefined) continue;
        out.prefilled = true;
        marks[note.key] = out;
        filled = true;
      }
      if (filled) count += 1;
    }
    if (count > 0) this.#touch();
    return count;
  }

  /** 所有線（主線與每條分支）的手順表。 */
  #allMarks(): Record<string, NoteAnnotation>[] {
    return [this.draft.notes, ...this.draft.branches.map((branch) => branch.notes)];
  }

  /** 移除所有線上尚未確認的預填。 */
  clearPrefilled(): number {
    let count = 0;
    for (const marks of this.#allMarks()) {
      for (const [key, mark] of Object.entries(marks)) {
        if (!mark.prefilled) continue;
        delete marks[key];
        count += 1;
      }
    }
    if (count > 0) this.#touch();
    return count;
  }

  /** 各種清除範圍會影響的數量（含所有打法分支），用在按鈕與確認文字。 */
  counts(): { hands: number; memos: number; branches: number } {
    let hands = 0;
    let memos = 0;
    for (const marks of this.#allMarks()) {
      for (const mark of Object.values(marks)) {
        if (mark.hand !== undefined || mark.track !== undefined) hands += 1;
        if (mark.memo) memos += 1;
      }
    }
    memos += this.draft.ranges.length + (this.draft.memo.trim() ? 1 : 0);
    memos += this.draft.branches.filter((branch) => branch.memo.trim()).length;
    return { hands, memos, branches: this.draft.branches.length };
  }

  /**
   * 一次清除：hands 只清手順（含預填、換手與信心，備註保留）、memos 只清所有備註、
   * all 全部清空（連同打法分支）。主線與分支一起處理。回傳清除前的草稿，供復原。
   */
  clearAll(scope: 'hands' | 'memos' | 'all'): AnnotationDraft {
    const before = $state.snapshot(this.draft) as AnnotationDraft;
    const keep = (marks: Record<string, NoteAnnotation>) => {
      const out: Record<string, NoteAnnotation> = {};
      for (const [key, mark] of Object.entries(marks)) {
        if (scope === 'hands' && mark.memo) {
          out[key] = { key, memo: mark.memo, ...(mark.by ? { by: mark.by } : {}) };
        } else if (scope === 'memos' && (mark.hand !== undefined || mark.track !== undefined)) {
          const next = { ...mark };
          delete next.memo;
          out[key] = next;
        }
      }
      return out;
    };
    if (scope === 'all') {
      this.draft.notes = {};
      this.draft.ranges = [];
      this.draft.memo = '';
      this.draft.branches = [];
      this.draft.active = MAIN_LINE;
    } else {
      this.draft.notes = keep(before.notes);
      for (const branch of this.draft.branches) {
        branch.notes = keep($state.snapshot(branch.notes) as Record<string, NoteAnnotation>);
        if (scope === 'memos') branch.memo = '';
      }
      if (scope === 'memos') {
        this.draft.ranges = [];
        this.draft.memo = '';
      }
    }
    this.#touch();
    return before;
  }

  // ---- 打法分支操作 ----

  /** 切換目前路線；不影響任何手順。 */
  setActive(id: string): void {
    const next = id === MAIN_LINE || this.branch(id) ? id : MAIN_LINE;
    if (next === this.draft.active) return;
    this.draft.active = next;
    this.#scheduleSave();
  }

  /** 沿某條線的路線看這一步的標註。 */
  #sourceOnLine(line: string, note: Note, part: StepPart): NoteAnnotation | undefined {
    return stepSource(this.draft, line, note, part);
  }

  /** 把 source 這一步負責的欄位複製進 marks 裡這顆的那一筆（深拷貝換手）。 */
  #copyPart(marks: Record<string, NoteAnnotation>, note: Note, part: StepPart, source: NoteAnnotation | undefined): void {
    if (!note.key || !source) return;
    const out: NoteAnnotation = marks[note.key] ? { ...marks[note.key] } : { key: note.key };
    // 滑行的信心：複製滑行時取來源那一步的；複製起點時保持這一筆原本滑行的。
    const existing = marks[note.key];
    const trackConfidence =
      part === 'track' ? stepConfidence(note, source, 'track') : existing ? stepConfidence(note, existing, 'track') : undefined;
    const record = out as unknown as Record<string, unknown>;
    for (const field of partFields(note, part)) {
      const value = source[field];
      if (value === undefined) delete record[field];
      else record[field] = field === 'handovers' ? (value as HandoverMark[]).map((item) => ({ ...item })) : value;
    }
    setTrackConfidence(note, out, trackConfidence);
    if (source.prefilled) out.prefilled = true;
    if (source.by && !out.by) out.by = source.by;
    marks[note.key] = out;
  }

  /** 刪掉這一步負責的欄位；那一筆變空時整筆移除。 */
  #dropPart(marks: Record<string, NoteAnnotation>, note: Note, part: StepPart): void {
    const mark = note.key ? marks[note.key] : undefined;
    if (!mark) return;
    const trackConfidence = part === 'hand' ? stepConfidence(note, mark, 'track') : undefined;
    const out = { ...mark } as unknown as Record<string, unknown>;
    for (const field of partFields(note, part)) delete out[field];
    const next = out as unknown as NoteAnnotation;
    // 拿掉起點的信心後，滑行的信心要保持原本的值。
    if (part === 'hand') setTrackConfidence(note, next, trackConfidence);
    if (isEmptyMark(next)) delete marks[mark.key];
    else marks[mark.key] = next;
  }

  /** 這條線在這個時間是否存在（主線永遠存在）。 */
  #covers(line: string, time: number): boolean {
    const branch = this.branch(line);
    return line === MAIN_LINE || (!!branch && inBranch(branch, time));
  }

  /**
   * 從目前路線拉出一條分支，範圍對齊到 from～to 內的第一步與最後一步（Slide 的起點與滑行可以分開）。
   * 預設從目前路線在起點走的線分出、併入目前路線在終點走的線；links 可以另外指定，兩者可以不同。
   * 這段目前看到的手順複製進去當起點。成功時切到新分支並回傳它；範圍內沒有步驟時回傳 null。
   */
  createBranch(
    from: number,
    to: number,
    name?: string,
    links: { parent?: string; merge?: string } = {},
  ): AnnotationBranch | null {
    const steps = this.stepsIn(Math.min(from, to), Math.max(from, to));
    if (steps.length === 0) return null;
    const lo = steps[0].time;
    const hi = steps[steps.length - 1].time;
    const active = this.draft.active;
    const parent = links.parent && this.#covers(links.parent, lo) ? links.parent : lineAt(this.draft.branches, active, lo);
    const merge = links.merge && this.#covers(links.merge, hi) ? links.merge : lineAt(this.draft.branches, active, hi);
    const marks: Record<string, NoteAnnotation> = {};
    for (const step of steps) this.#copyPart(marks, step.note, step.part, this.#source(step.note, step.part));
    const branch: AnnotationBranch = {
      id: newBranchId(),
      name: name?.trim() || nextBranchName(this.draft.branches),
      parent,
      merge,
      from: lo,
      to: hi,
      memo: '',
      ...(this.annotator ? { by: this.annotator } : {}),
      color: nextLineColor([this.draft.mainColor, ...this.draft.branches.map((item) => item.color)]),
      notes: marks,
    };
    this.draft.branches = sortBranches([...this.draft.branches, branch]);
    this.draft.active = branch.id;
    this.#touch();
    return this.branch(branch.id) ?? null;
  }

  renameBranch(id: string, name: string): void {
    const branch = this.branch(id);
    if (!branch || !name.trim()) return;
    branch.name = name.trim();
    this.#touch();
  }

  setBranchMemo(id: string, memo: string): void {
    const branch = this.branch(id);
    if (!branch) return;
    branch.memo = memo;
    this.#touch();
  }

  /**
   * 調整分支範圍（對齊步驟）。新涵蓋的步驟複製原本路線上的手順（起點之前的沿分出的線、終點之後的沿併入的線），
   * 移出範圍的步驟刪掉。分出／併入的線在新的起點／終點不存在時，改接那條線在那個時間實際走的線；
   * 從這條分出或併入這條的分支也一樣重新接好。回傳修改前的草稿供復原；範圍無效時回傳 null。
   */
  setBranchRange(id: string, from: number, to: number): AnnotationDraft | null {
    const branch = this.branch(id);
    if (!branch) return null;
    const steps = this.stepsIn(Math.min(from, to), Math.max(from, to));
    if (steps.length === 0) return null;
    const before = $state.snapshot(this.draft) as AnnotationDraft;
    for (const step of steps) {
      if (inBranch(branch, step.time)) continue;
      const line = step.time < branch.from ? branch.parent : branch.merge;
      this.#copyPart(branch.notes, step.note, step.part, this.#sourceOnLine(line, step.note, step.part));
    }
    branch.from = steps[0].time;
    branch.to = steps[steps.length - 1].time;
    if (!this.#covers(branch.parent, branch.from)) branch.parent = lineAt(this.draft.branches, branch.parent, branch.from);
    if (!this.#covers(branch.merge, branch.to)) branch.merge = lineAt(this.draft.branches, branch.merge, branch.to);
    this.#fitInside(branch);
    this.draft.branches = sortBranches(this.draft.branches);
    this.#touch();
    return before;
  }

  /** 刪掉範圍外那幾步的手順；接在這條上、但接點已不在範圍內的分支改接那個時間實際走的線。 */
  #fitInside(branch: AnnotationBranch): void {
    for (const key of Object.keys(branch.notes)) {
      const note = this.#byKey.get(key);
      if (!note) continue;
      for (const part of this.#parts(note)) {
        if (!inBranch(branch, stepTime(note, part))) this.#dropPart(branch.notes, note, part);
      }
    }
    for (const other of this.draft.branches) {
      if (other.parent === branch.id && !inBranch(branch, other.from)) {
        other.parent = lineAt(this.draft.branches, branch.id, other.from);
      }
      if (other.merge === branch.id && !inBranch(branch, other.to)) {
        other.merge = lineAt(this.draft.branches, branch.id, other.to);
      }
    }
  }

  /** 改分出／併入的線；必須在起點／終點存在，且不能造成繞圈。成功時回傳 true。 */
  setBranchLinks(id: string, links: { parent?: string; merge?: string }): boolean {
    const branch = this.branch(id);
    if (!branch) return false;
    const valid = (line: string, time: number) =>
      line !== id && this.#covers(line, time) && !dependsOn(this.draft.branches, line, id);
    if (links.parent !== undefined && !valid(links.parent, branch.from)) return false;
    if (links.merge !== undefined && !valid(links.merge, branch.to)) return false;
    if (links.parent !== undefined) branch.parent = links.parent;
    if (links.merge !== undefined) branch.merge = links.merge;
    this.draft.branches = sortBranches(this.draft.branches);
    this.#touch();
    return true;
  }

  /**
   * 刪除分支；從它分出或併入它的分支改接它原本的路線（那個時間實際走的線）。
   * 目前路線是它時改走它分出的線。回傳刪除前的草稿供復原。
   */
  removeBranch(id: string): AnnotationDraft | null {
    const branch = this.branch(id);
    if (!branch) return null;
    const before = $state.snapshot(this.draft) as AnnotationDraft;
    const others = this.draft.branches.filter((item) => item.id !== id);
    for (const other of others) {
      if (other.parent === id) other.parent = lineAt(others, branch.parent, other.from);
      if (other.merge === id) other.merge = lineAt(others, branch.merge, other.to);
    }
    if (this.draft.active === id) this.draft.active = branch.parent;
    this.draft.branches = sortBranches(others);
    this.#touch();
    return before;
  }

  /**
   * 和分出的那條線交換這段的手順（逐步交換，每一步換給那條線的路線上負責那一步的線）：
   * 分支的打法變成那條路線的，原本的打法留在這條分支，所以不會遺失任何一種。回傳交換前的草稿供復原。
   */
  swapWithParent(id: string): AnnotationDraft | null {
    const branch = this.branch(id);
    if (!branch) return null;
    const before = $state.snapshot(this.draft) as AnnotationDraft;
    for (const step of this.stepsIn(branch.from, branch.to)) {
      const key = step.note.key;
      const parentMarks = lineMarks(this.draft, lineAt(this.draft.branches, branch.parent, step.time));
      if (!key || !parentMarks) continue;
      const mine = branch.notes[key] ? { ...branch.notes[key] } : undefined;
      const theirs = parentMarks[key] ? { ...parentMarks[key] } : undefined;
      this.#dropPart(branch.notes, step.note, step.part);
      this.#dropPart(parentMarks, step.note, step.part);
      this.#copyPart(branch.notes, step.note, step.part, theirs);
      this.#copyPart(parentMarks, step.note, step.part, mine);
    }
    this.#touch();
    return before;
  }

  /** 分支和上一層手順不同的步數（不看備註與信心）。 */
  branchDiff(id: string): number {
    const branch = this.branch(id);
    if (!branch) return 0;
    let count = 0;
    for (const step of this.stepsIn(branch.from, branch.to)) {
      const mine = step.note.key ? branch.notes[step.note.key] : undefined;
      const theirs = this.#sourceOnLine(branch.parent, step.note, step.part);
      if (partHands(mine, step.part) !== partHands(theirs, step.part)) count += 1;
    }
    return count;
  }

  /** 復原到指定的草稿（清除後的「復原」）。 */
  restore(draft: AnnotationDraft): void {
    this.draft = draft;
    this.#touch();
  }

  /** 設定或解除對照影片（含同步偏移）。 */
  setVideo(link: VideoLink | null): void {
    if (!this.recordId) return;
    if (link) this.draft.video = { ...link };
    else delete this.draft.video;
    this.#syncAlignment();
    this.#scheduleSave();
  }

  #scheduleSave(): void {
    this.draft.updatedAt = Date.now();
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => this.flush(), SAVE_DELAY);
  }

  /** 影片還沒對齊時補上這份譜面配這部影片上次的對齊；已對齊就記下來。有補上時回傳 true。 */
  #syncAlignment(): boolean {
    const hash = records.get(this.recordId)?.sourceHash;
    const video = this.draft.video;
    if (!hash || !video) return false;
    if (video.offset !== null) {
      rememberAlignment(hash, $state.snapshot(video) as VideoLink);
      return false;
    }
    const recalled = recallAlignment(hash, $state.snapshot(video) as VideoLink);
    if (recalled.offset === null) return false;
    this.draft.video = recalled;
    return true;
  }

  setOverallMemo(memo: string): void {
    this.draft.memo = memo;
    this.#touch();
  }

  addRange(from: number, to: number, memo: string): void {
    const text = memo.trim();
    if (!text) return;
    this.draft.ranges.push({
      from: Math.min(from, to),
      to: Math.max(from, to),
      memo: text,
      ...(this.annotator ? { by: this.annotator } : {}),
    });
    this.draft.ranges.sort((a, b) => a.from - b.from);
    this.#touch();
  }

  removeRange(index: number): void {
    this.draft.ranges.splice(index, 1);
    this.#touch();
  }

  /** 目前草稿連同原譜轉成標註檔。 */
  async toFile(): Promise<HandAnnotation | null> {
    const source = session.result?.source ?? session.source;
    if (!source || !this.keysReady) return null;
    this.flush();
    return toFile($state.snapshot(this.draft) as AnnotationDraft, {
      source,
      sha256: await hashText(source),
      firstSeconds: session.result?.firstSeconds ?? session.firstSeconds,
      keys: this.ordered.map((note) => note.key ?? ''),
    });
  }

  async exportText(): Promise<string | null> {
    const file = await this.toFile();
    return file ? serialize(file) : null;
  }

  /** 合併匯入的標註到目前草稿。 */
  applyMerge(file: HandAnnotation, mode: 'fill' | 'replace'): MergeResult {
    const result = merge($state.snapshot(this.draft) as AnnotationDraft, file, mode);
    this.draft = result.draft;
    this.#syncAlignment();
    this.#touch();
    return result;
  }

  /**
   * 匯入標註檔：找同一份原譜的紀錄（沒有就連同原譜新增一筆），切過去後合併標註。
   * 回傳給使用者看的結果摘要；原譜被改過時不匯入。
   */
  async importFile(
    file: HandAnnotation,
    options: { mode: 'fill' | 'replace'; name?: string; skipped?: number },
  ): Promise<{ ok: boolean; text: string }> {
    const tampered = await checkSource(file);
    if (tampered) return { ok: false, text: tampered };
    const hash = await hashText(file.chart.source);
    let record = records.findByHash(hash);
    let created = false;
    if (!record) {
      record = await records.add(file.chart.source, { name: options.name?.trim() || file.title || undefined });
      created = true;
    }
    const switching = records.activeId !== record.id;
    records.activeId = record.id;
    this.bind(record);
    const result = this.applyMerge(file, options.mode);
    if (switching || !session.result || session.result.source !== record.source) {
      void session.load(record.source);
    }
    const parts = [
      created ? '已連同原譜新增譜面紀錄' : switching ? '已切換到同一份譜面的紀錄' : '已合併到目前的譜面',
      `新增 ${result.added} 顆`,
    ];
    if (result.branches > 0) parts.push(`加入 ${result.branches} 條打法分支`);
    if (result.conflicts.length > 0) {
      parts.push(
        options.mode === 'fill'
          ? `${result.conflicts.length} 顆手順不同，保留你的`
          : `${result.conflicts.length} 顆手順不同，已改用匯入的`,
      );
    }
    if (options.skipped) parts.push(`略過 ${options.skipped} 筆格式不對的項目`);
    return { ok: true, text: `${parts.join('，')}。` };
  }

  /** 選取一顆音符（預設它的第一步）並把播放時間移到那一步。 */
  select(note: Note, part?: StepPart, seek = true): void {
    const index = this.#indexOf(note, part);
    const step = index === undefined ? null : this.steps[index];
    this.#part = { noteId: note.id, part: step?.part ?? part ?? 'hand' };
    session.selectNote(note.id);
    if (seek) playback.seek(step?.time ?? note.timeSeconds);
  }

  selectStep(step: Step): void {
    this.select(step.note, step.part);
  }

  #currentIndex(): number {
    const step = this.currentStep;
    return step ? (this.#stepIndex.get(step.key) ?? -1) : -1;
  }

  /** 依時間順序的上一步／下一步。 */
  step(direction: 1 | -1): void {
    const list = this.steps;
    if (list.length === 0) return;
    const index = this.#currentIndex();
    const target = list[Math.min(list.length - 1, Math.max(0, index + direction))];
    if (target) this.selectStep(target);
  }

  /** 從目前這一步之後找下一步還沒有人確認的；到結尾就從頭找。 */
  nextTodo(): boolean {
    const list = this.steps;
    const start = this.#currentIndex() + 1;
    for (let offset = 0; offset < list.length; offset += 1) {
      const step = list[(start + offset) % list.length];
      if (!isStepDone(step, this.stepMark(step))) {
        this.selectStep(step);
        return true;
      }
    }
    return false;
  }

  /** 標完跳到下一步；wholeNote 時跳過這顆音符剩下的步驟。 */
  #advance(from: Step, wholeNote = false): void {
    if (!this.autoAdvance) return;
    const list = this.steps;
    let index = (this.#stepIndex.get(from.key) ?? -1) + 1;
    while (wholeNote && index < list.length && list[index].note.id === from.note.id) index += 1;
    const target = list[index];
    if (target) this.selectStep(target);
  }

  /**
   * 標註分頁開著時的快捷鍵；處理了就回傳 true。
   * A／D 標目前這一步的左右手（Shift 整顆同一隻手）、S 切換信心、Enter 確認預填、
   * Delete 清除、N 下一步未標。
   */
  handleKey(event: KeyboardEvent): boolean {
    const key = event.key.toLowerCase();
    if (event.key === 'Escape' && this.pendingStart) {
      this.cancelMark();
      return true;
    }
    if (key === 'n') {
      this.nextTodo();
      return true;
    }
    const step = this.currentStep;
    const note = step?.note;
    if (!step || !note || !note.key) return false;
    if (key === 'a' || key === 'd') {
      const hand: Hand = key === 'a' ? 'L' : 'R';
      if (event.shiftKey) this.setAll(note, hand);
      else if (step.part === 'hand') this.setHand(note, hand);
      else this.setTrack(note, hand);
      this.#advance(step, event.shiftKey);
      return true;
    }
    if (key === 's') {
      this.cycleConfidence(note, step.part);
      return true;
    }
    if (key === 'b') {
      this.cycleLine(step, event.shiftKey ? -1 : 1);
      return true;
    }
    if (event.key === 'Enter') {
      if (this.mark(note)?.prefilled) this.confirm(note);
      this.#advance(step);
      return true;
    }
    if (event.key === 'Delete' || event.key === 'Backspace') {
      this.clear(note);
      return true;
    }
    return false;
  }

  /** 比對：Rust 求模型最佳解與照標註的最佳解。只送有人確認過的標註。 */
  async evaluate(): Promise<void> {
    const file = await this.toFile();
    if (!file) return;
    // Rust 只認單一份手順：送目前路線展開後的結果，不送分支。
    file.notes = this.ordered
      .map((note) => (note.key ? this.routed[note.key] : undefined))
      .filter((mark): mark is NoteAnnotation => isHumanLabeled(mark))
      .map((mark) => ({ ...mark }));
    delete file.branches;
    this.evaluating = true;
    this.evaluationError = null;
    try {
      this.evaluation = await evaluateAnnotation({
        requestId: nextRequestId(),
        source: file.chart.source,
        firstSeconds: file.chart.firstSeconds,
        solverConfig: projectConfig(session.config),
        annotation: file,
      });
    } catch (error) {
      this.evaluationError = typeof error === 'string' ? error : String(error);
    } finally {
      this.evaluating = false;
    }
  }
}

export const annotation = new AnnotationStore();
