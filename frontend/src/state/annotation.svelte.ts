// 真人手順標註的編輯狀態：跟著目前開啟的譜面紀錄，改動後自動寫回紀錄。

import { CANCELLED, appVersion, evaluateAnnotation, nextRequestId } from '../lib/api';
import {
  type AnnotationBranch,
  type AnnotationDraft,
  CONFIDENCE_LABEL,
  cleanDraft,
  cleanRoute,
  composeMark,
  emptyDraft,
  firstPart,
  humanMark,
  inBranch,
  isEmptyMark,
  isPrefilledPart,
  dependsOn,
  lineAt,
  lineMarks,
  lineName,
  lineRoute,
  MAIN_LINE,
  MAIN_NAME,
  merge,
  modelMarks,
  sameGroup,
  nextLineColor,
  neededParts,
  newBranchId,
  nextBranchName,
  partFields,
  partHands,
  pickRoute,
  prefilledParts,
  routeBranches,
  routeLineAt,
  routeMarks,
  routeSections,
  sameRoute,
  serialize,
  setPrefilled,
  setTrackConfidence,
  sortBranches,
  stepConfidence,
  stepSource,
  stepTime,
  toFile,
  type StepPart,
  type MergeResult,
} from '../lib/annotation';
import { configEquals, projectConfig, withScoring, type ScoringChoice } from '../lib/contract';
import { STORE_EVALUATIONS, dbDeleteKeys, dbGetAllByIndex, dbPut, hashText, quickHash, stableJson } from '../lib/db';
import { recallAlignment, rememberAlignment, type HandcamMark, type VideoLink } from '../lib/video';
import type {
  Confidence,
  EvaluateResponse,
  Hand,
  HandAnnotation,
  HandoverMark,
  Note,
  NoteAnnotation,
  NoteGroup,
  RouteAnnotation,
  SolverConfig,
  TrackHand,
} from '../lib/types';
import { playback } from './playback.svelte';
import { recordTitle, records, type ChartRecord } from './records.svelte';
import { SCORING_REVISION, SOLVER_REVISION, session } from './session.svelte';
import { toasts } from './toasts.svelte';

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
  if (!mark || isPrefilledPart(mark, step.part)) return false;
  return step.part === 'hand' ? mark.hand !== undefined : mark.track !== undefined;
}

/** 每筆紀錄最多留幾份比對暫存（參數 × 模型）；超過時刪掉最舊的。 */
const EVALUATION_LIMIT = 32;

/**
 * 比對有打法分支時的暫存格名稱：每一段的走法都比、每段挑一種，採用整首吻合率最高的組合，和目前路線無關。
 * 2026-11-09 起改成每段各挑，換了格子名稱：之前的 'best' 格子是整首只挑一條，不再沿用。
 * 沒有分支時仍用 MAIN_LINE，以前的暫存照樣有效。
 */
const BEST_ROUTE_SLOT = 'sections';

/**
 * 比對暫存（2026-11-08 使用者要求）：某筆紀錄、某組參數（含模型）最後一次比對的結果。
 * 比對頁換模型時先顯示這份；標註之後改過（marks 不同）就標成舊的；
 * App 版本、求解或評分修訂、原文任一不同就作廢（和分析快取同一套條件）。
 */
interface StoredEvaluation {
  /** 資料庫鍵：`${recordId}|${slot}`。 */
  key: string;
  recordId: string;
  /** 路線（沒有分支為 main，有分支為 sections）＋起始秒數＋已投影的參數（含模型）。 */
  slot: string;
  /** App 版本＋求解與評分修訂。 */
  version: string;
  sourceHash: string;
  /** 比對時送出的標註指紋（routesPrint）。 */
  marks: string;
  createdAt: number;
  response: EvaluateResponse;
}

/** 模型選單上顯示的暫存吻合率。 */
export interface AgreementPreview {
  /** 吻合百分比（一位小數）；沒有可比對的步驟時為 null。 */
  percent: number | null;
  /** 標註在那次比對之後改過。 */
  outdated: boolean;
}

async function evaluationVersion(): Promise<string> {
  return `${await appVersion()}|${SOLVER_REVISION}+${SCORING_REVISION}`;
}

/** 標註指紋：只看會影響比對的欄位，改備註或換標註者不算改過。 */
function markPrint(marks: NoteAnnotation[]): string {
  return quickHash(
    JSON.stringify(
      marks.map((mark) => [
        mark.key,
        mark.hand ?? null,
        mark.track ?? null,
        mark.handovers ?? null,
        mark.confidence ?? null,
        mark.trackConfidence ?? null,
      ]),
    ),
  );
}

/** 比對送出的所有路線的指紋：沒有分支時和以前一樣只看主線（以前的暫存照樣有效）。 */
function routesPrint([main, ...others]: RouteAnnotation[]): string {
  const head = markPrint(main?.notes ?? []);
  if (others.length === 0) return head;
  return [head, ...others.map((route) => `${route.id}:${markPrint(route.notes)}`)].join('|');
}

/** 只留最新的 EVALUATION_LIMIT 份，回傳留下的與要刪的鍵。 */
function pruneEvaluations(entries: StoredEvaluation[]): { kept: StoredEvaluation[]; dropped: string[] } {
  if (entries.length <= EVALUATION_LIMIT) return { kept: entries, dropped: [] };
  const sorted = [...entries].sort((a, b) => b.createdAt - a.createdAt);
  return {
    kept: sorted.slice(0, EVALUATION_LIMIT),
    dropped: sorted.slice(EVALUATION_LIMIT).map((entry) => entry.key),
  };
}

function bySlot(entries: StoredEvaluation[]): Record<string, StoredEvaluation> {
  return Object.fromEntries(entries.map((entry) => [entry.slot, entry]));
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

  /** 目前紀錄的比對暫存，以 slot 為鍵（見 StoredEvaluation）；比對結果、進度與錯誤的 getter 在檔尾。 */
  #evaluations = $state.raw<Record<string, StoredEvaluation>>({});
  /** 正在比對的 slot；null 表示沒有比對在跑。 */
  #running = $state<string | null>(null);
  #runToken = 0;
  #error = $state.raw<{ slot: string; message: string } | null>(null);
  /** 比對頁換了模型，正在等盤面的分析（載入快取或重新分析）與接著的比對。 */
  switching = $state(false);
  #switchToken = 0;

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
    this.groupStart = null;
    this.endPicking();
    this.draft = (record && cleanDraft(record.annotation)) ?? emptyDraft(record ? recordTitle(record) : '');
    if (record && !this.draft.title) this.draft.title = recordTitle(record);
    if (this.#syncAlignment()) this.#scheduleSave();
    // 比對暫存跟著紀錄走：先清掉上一筆的，再從資料庫讀這一筆的。
    // 上一筆還在跑的比對照樣寫進資料庫，只是不顯示在這一筆。
    this.#evaluations = {};
    this.#error = null;
    this.#running = null;
    this.#runToken += 1;
    this.#switchToken += 1;
    this.switching = false;
    if (record) void this.#loadEvaluations(record.id);
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

  // 目前路線是一組分支選擇（draft.route）：不同段落可以各走各的分支，沒選到的段落走主線。

  /** 目前路線展開後的手順（進度、篩選都看這份；比對則主線與每條路線都送）。 */
  routed = $derived<Record<string, NoteAnnotation>>(routeMarks(this.draft, this.ordered));

  /** 目前路線有走到的分支。 */
  routeLines = $derived(routeBranches(this.draft.branches, this.draft.route));

  /** 目前路線在這個時間走哪條線。 */
  routeAt(time: number): string {
    return routeLineAt(this.draft.branches, this.draft.route, time);
  }

  /** 這一步在目前路線上歸哪條線管；編輯會寫到那條線。 */
  lineOfStep(note: Note, part: StepPart): string {
    return this.routeAt(stepTime(note, part));
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

  /** 在這一步的各種打法之間輪流切換（快捷鍵 B／Shift+B）；只換這一段，其他段落選的分支不變。 */
  cycleLine(step: Step, direction: 1 | -1): void {
    const lines = this.linesAt(step);
    if (lines.length < 2) return;
    const index = lines.indexOf(this.lineOfStep(step.note, step.part));
    this.pickLine(lines[(index + direction + lines.length) % lines.length], step.time);
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

  /** 直接以兩步為範圍建立分支（多選時用）：從目前路線在第一步走的線分出，併入在最後一步走的線。 */
  branchFromSteps(first: Step, last: Step): AnnotationBranch | null {
    this.pendingStart = first.key;
    this.pendingFrom = this.lineOfStep(first.note, first.part);
    const branch = this.finishMark(last);
    if (!branch) this.cancelMark();
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
    return stepSource(this.draft, this.draft.route, note, part);
  }

  /** 這一步自己的標註（是否預填、是否已確認看這一筆）。 */
  stepMark(step: { note: Note; part: StepPart }): NoteAnnotation | undefined {
    return this.#source(step.note, step.part);
  }

  /** 這顆音符在目前路線上的標註（各步組合而成）。 */
  mark(note: Note | null): NoteAnnotation | undefined {
    return note ? composeMark(note, (part) => this.#source(note, part)) : undefined;
  }

  /** 這顆音符沿某條路線的標註（比對結果用：比對採用的路線不一定是目前選的那條）。 */
  markOn(note: Note, line: string): NoteAnnotation | undefined {
    return composeMark(note, (part) => this.#sourceOnLine(line, note, part));
  }

  /** 這顆音符照某組選擇（例如比對採用的組合）走時的標註。 */
  markOnRoute(note: Note, picks: readonly string[]): NoteAnnotation | undefined {
    return composeMark(note, (part) => stepSource(this.draft, picks, note, part));
  }

  /** 這顆音符需要的步驟。 */
  #parts(note: Note): StepPart[] {
    const need = neededParts(note);
    return [...(need.hand ? (['hand'] as const) : []), ...(need.track ? (['track'] as const) : [])];
  }

  /**
   * 把 next 中這幾步負責的欄位寫進某條線的那一筆。寫到的步驟算有人確認過、不再是預填；
   * 沒寫到的步驟維持原樣（預填仍是預填），沒看過的預填不會因為這次編輯就變成真人資料。
   * 滑行的信心在省略時沿用起點的信心，所以寫完後重新算一次，讓它維持原本（或 next）的值。
   */
  #writeParts(line: string, note: Note, next: NoteAnnotation, parts: StepPart[]): void {
    const marks = lineMarks(this.draft, line);
    const key = note.key;
    if (!marks || !key) return;
    const current = marks[key];
    const out: NoteAnnotation = current ? { ...current } : { key };
    const fields = parts.flatMap((part) => partFields(note, part));
    const trackConfidence = fields.includes('track')
      ? stepConfidence(note, next, 'track')
      : current
        ? stepConfidence(note, current, 'track')
        : undefined;
    const prefilled = prefilledParts(current);
    const record = out as unknown as Record<string, unknown>;
    for (const field of fields) {
      const value = next[field];
      if (value === undefined) delete record[field];
      else record[field] = value;
    }
    setTrackConfidence(note, out, trackConfidence);
    setPrefilled(out, { hand: prefilled.hand && !parts.includes('hand'), track: prefilled.track && !parts.includes('track') });
    if (this.annotator) out.by = this.annotator;
    if (isEmptyMark(out)) delete marks[key];
    else marks[key] = out;
  }

  /**
   * 修改一顆的標註：parts 是這次動到的步驟，只有它們寫回去並算有人確認過（其他步驟的預填照舊）。
   * 內容清空時移除整筆。起點與滑行分屬不同的線時，各步寫回自己的線。
   */
  #edit(note: Note, parts: StepPart[], change: (mark: NoteAnnotation) => void): void {
    if (!note.key) return;
    const current = this.mark(note);
    const next: NoteAnnotation = current ? { ...current } : { key: note.key };
    change(next);
    const byLine = new Map<string, StepPart[]>();
    for (const part of this.#parts(note)) {
      if (!parts.includes(part)) continue;
      const line = this.lineOfStep(note, part);
      byLine.set(line, [...(byLine.get(line) ?? []), part]);
    }
    for (const [line, list] of byLine) this.#writeParts(line, note, next, list);
    this.#touch();
  }

  setHand(note: Note, hand: Hand | undefined): void {
    this.#edit(note, ['hand'], (mark) => {
      if (hand) mark.hand = hand;
      else delete mark.hand;
    });
  }

  setTrack(note: Note, track: TrackHand | undefined): void {
    this.#edit(note, ['track'], (mark) => {
      if (track) mark.track = track;
      else delete mark.track;
      if (track === 'LR') delete mark.handovers;
    });
  }

  /** 快捷鍵 Shift+A／D：這顆需要的部分都設成同一隻手。 */
  setAll(note: Note, hand: Hand): void {
    const need = neededParts(note);
    this.#edit(note, this.#parts(note), (mark) => {
      if (need.hand) mark.hand = hand;
      if (need.track) mark.track = hand;
    });
  }

  /** 設定某一步的信心（預設第一步）；有起點的 Slide 起點與滑行分開設定，互不影響。 */
  setConfidence(note: Note, confidence: Confidence, part: StepPart = firstPart(note)): void {
    this.#edit(note, [part], (mark) => {
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
    this.#edit(note, [firstPart(note)], (mark) => {
      if (memo.trim()) mark.memo = memo;
      else delete mark.memo;
    });
  }

  /** 在 Slide 滑行期間加一次換手；換到目前滑行手的另一隻。 */
  addHandover(note: Note, at: number): void {
    this.#edit(note, ['track'], (mark) => {
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
    this.#edit(note, ['track'], (mark) => {
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
    this.#edit(note, this.#parts(note), (mark) => {
      delete mark.hand;
      delete mark.track;
      delete mark.handovers;
      delete mark.confidence;
      delete mark.trackConfidence;
      delete mark.memo;
    });
  }

  /** 把這顆這幾步（預設全部）的預填確認為真人資料（手順不變）；有確認到時回傳 true。 */
  #confirmParts(note: Note, parts: StepPart[] = this.#parts(note)): boolean {
    let changed = false;
    for (const part of parts) {
      const marks = lineMarks(this.draft, this.lineOfStep(note, part));
      const mark = note.key ? marks?.[note.key] : undefined;
      if (!marks || !mark || !isPrefilledPart(mark, part)) continue;
      const next = { ...mark };
      const prefilled = prefilledParts(mark);
      prefilled[part] = false;
      setPrefilled(next, prefilled);
      if (this.annotator) next.by = this.annotator;
      marks[mark.key] = next;
      changed = true;
    }
    return changed;
  }

  confirm(note: Note): void {
    if (this.#confirmParts(note)) this.#touch();
  }

  confirmAll(): number {
    let count = 0;
    for (const note of this.ordered) if (this.#confirmParts(note)) count += 1;
    if (count > 0) this.#touch();
    return count;
  }

  /**
   * 以目前盤面的模型方案預填目前路線上還沒有手的步驟（含 Slide 滑行），標成預填；
   * 已經有手的步驟（真人標的或之前的預填）不動，同一顆其他步驟的真人標註也照舊。回傳填入的音符數。
   */
  prefillFromModel(): number {
    const solution = session.solutions[session.solutionIndex] ?? session.solutions[0];
    if (!solution) return 0;
    let count = 0;
    const models = modelMarks(solution, this.ordered);
    for (const note of this.ordered) {
      if (!note.key) continue;
      const model = models.get(note.id) ?? {};
      let filled = false;
      for (const part of this.#parts(note)) {
        const marks = lineMarks(this.draft, this.lineOfStep(note, part));
        if (!marks) continue;
        const value = part === 'hand' ? model.hand : model.track;
        if (value === undefined) continue;
        const current = marks[note.key];
        if (current && (part === 'hand' ? current.hand : current.track) !== undefined) continue;
        const out: NoteAnnotation = current ? { ...current } : { key: note.key };
        const prefilled = prefilledParts(current);
        if (part === 'hand') out.hand = model.hand;
        else {
          out.track = model.track;
          if (model.handovers) out.handovers = model.handovers;
          else delete out.handovers;
        }
        setPrefilled(out, { ...prefilled, [part]: true });
        marks[note.key] = out;
        filled = true;
      }
      if (filled) count += 1;
    }
    if (count > 0) this.#touch();
    return count;
  }

  /**
   * 以手元影片推估的結果預填目前路線上的步驟。真人標的步驟一律不動；空白或原本是預填（含模型預填）的步驟
   * 改用影片結果，因為影片是實際打法的觀察。中信心的步驟標「不確定」，提醒照影片確認。回傳填入的音符數。
   */
  prefillFromVideo(marks: HandcamMark[]): number {
    const byKey = new Map(marks.map((mark) => [mark.key, mark]));
    let count = 0;
    for (const note of this.ordered) {
      if (!note.key) continue;
      const video = byKey.get(note.key);
      if (!video) continue;
      let filled = false;
      for (const part of this.#parts(note)) {
        const value = part === 'hand' ? video.hand : video.track;
        if (value === undefined) continue;
        const lineMarksOf = lineMarks(this.draft, this.lineOfStep(note, part));
        if (!lineMarksOf) continue;
        const current = lineMarksOf[note.key];
        const prefilled = prefilledParts(current);
        if (current && (part === 'hand' ? current.hand : current.track) !== undefined && !prefilled[part]) continue;
        const out: NoteAnnotation = current ? { ...current } : { key: note.key };
        const unsure = part === 'hand' ? video.handUnsure : video.trackUnsure;
        // 有起點的 Slide：滑行那一步的信心記在 trackConfidence；其餘第一步記在 confidence。
        const first = part === firstPart(note);
        const own = first ? current?.confidence : current?.trackConfidence;
        // 原本是預填就換成影片的信心；空白的步驟有人設過信心就照舊，否則中信心標「不確定」。
        const confidence: Confidence = prefilled[part] || own === undefined ? (unsure ? 'unsure' : 'sure') : own;
        const track = stepConfidence(note, current, 'track');
        if (part === 'hand') out.hand = value as Hand;
        else {
          out.track = value;
          delete out.handovers;
        }
        if (first) {
          if (confidence === 'sure') delete out.confidence;
          else out.confidence = confidence;
          // 滑行沿用起點的信心；改了起點的信心，已經有手（或自己設過信心）的滑行要維持原本的值。
          if (current?.track !== undefined || current?.trackConfidence !== undefined) setTrackConfidence(note, out, track);
        } else {
          setTrackConfidence(note, out, confidence);
        }
        setPrefilled(out, { ...prefilled, [part]: true });
        lineMarksOf[note.key] = out;
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

  /**
   * 移除所有線上尚未確認的預填：拿掉預填那一步的手和它的信心（預填時帶的），同一顆真人標的步驟與備註保留。
   * 回傳拿掉的步數。
   */
  clearPrefilled(): number {
    let count = 0;
    for (const marks of this.#allMarks()) {
      for (const [key, mark] of Object.entries(marks)) {
        const prefilled = prefilledParts(mark);
        if (!prefilled.hand && !prefilled.track) continue;
        const note = this.#byKey.get(key);
        // 對不到音符時（不是這份譜面的鍵）有接觸的手就當作有起點。
        const twoSteps = note ? neededParts(note).hand && neededParts(note).track : mark.hand !== undefined;
        const out: NoteAnnotation = { ...mark };
        if (prefilled.hand) {
          // 滑行沿用起點的信心；拿掉起點的信心後，真人標的滑行要維持原本的值。
          const track = out.trackConfidence ?? out.confidence ?? 'sure';
          delete out.hand;
          delete out.confidence;
          if (twoSteps && out.track !== undefined && !prefilled.track) {
            if (track === 'sure') delete out.trackConfidence;
            else out.trackConfidence = track;
          }
        }
        if (prefilled.track) {
          delete out.track;
          delete out.handovers;
          delete out.trackConfidence;
          if (!twoSteps) delete out.confidence;
        }
        setPrefilled(out, { hand: false, track: false });
        if (isEmptyMark(out)) delete marks[key];
        else marks[key] = out;
        count += Number(prefilled.hand) + Number(prefilled.track);
      }
    }
    if (count > 0) this.#touch();
    return count;
  }

  // ---- 多選（逐顆清單）：以步驟為單位一次標很多步 ----
  //
  // Ctrl／Shift 點選或右鍵「選取」進入選取模式；這時點清單只切換選取、不換目前這一步，
  // A／D 套用到全部選取。批次改動做完只跳一次提示，可以復原。

  picking = $state(false);
  /** 選取的步驟 key（點選順序）。 */
  picked = $state<string[]>([]);

  /** 選取的步驟，依時間排列；已經不存在的略過。 */
  get pickedSteps(): Step[] {
    return this.picked
      .map((key) => this.#stepIndex.get(key))
      .filter((index): index is number => index !== undefined)
      .sort((a, b) => a - b)
      .map((index) => this.steps[index]);
  }

  /** 設定選取；清空就結束選取模式，不會留下「已選 0 步」。 */
  setPicked(keys: string[]): void {
    this.picked = keys;
    this.picking = keys.length > 0;
  }

  endPicking(): void {
    this.picking = false;
    this.picked = [];
  }

  #partsByNote(steps: Step[]): Map<Note, Set<StepPart>> {
    const map = new Map<Note, Set<StepPart>>();
    for (const step of steps) map.set(step.note, (map.get(step.note) ?? new Set<StepPart>()).add(step.part));
    return map;
  }

  /** 這些步驟都標成同一隻手（起點／接觸寫手，滑行寫滑行的手）；沒選到的步驟維持原樣。 */
  setHandMany(steps: Step[], hand: Hand): number {
    for (const [note, parts] of this.#partsByNote(steps)) {
      this.#edit(note, [...parts], (mark) => {
        if (parts.has('hand')) mark.hand = hand;
        if (parts.has('track')) mark.track = hand;
      });
    }
    return steps.length;
  }

  setConfidenceMany(steps: Step[], confidence: Confidence): number {
    for (const step of steps) this.setConfidence(step.note, confidence, step.part);
    return steps.length;
  }

  /** 清掉這些步驟的手（滑行連同換手）；信心與備註保留。回傳實際清掉的步數。 */
  clearHandsMany(steps: Step[]): number {
    let count = 0;
    for (const [note, parts] of this.#partsByNote(steps)) {
      const mark = this.mark(note);
      const filled = [...parts].filter((part) => (part === 'hand' ? mark?.hand : mark?.track) !== undefined).length;
      if (filled === 0) continue;
      count += filled;
      this.#edit(note, [...parts], (next) => {
        if (parts.has('hand')) delete next.hand;
        if (parts.has('track')) {
          delete next.track;
          delete next.handovers;
        }
      });
    }
    return count;
  }

  /** 把這些步驟的預填確認為真人資料（沒選到的步驟維持預填）；回傳確認的步數。 */
  confirmMany(steps: Step[]): number {
    let count = 0;
    for (const step of steps) if (this.#confirmParts(step.note, [step.part])) count += 1;
    if (count > 0) this.#touch();
    return count;
  }

  /** 批次改動：記下改動前的草稿，做完跳一次可復原的提示。 */
  #batch(run: () => number, done: (count: number) => string, none: string): void {
    const before = $state.snapshot(this.draft) as AnnotationDraft;
    const count = run();
    if (count === 0) {
      toasts.show({ id: 'annotation-batch', tone: 'info', title: none });
      return;
    }
    toasts.show({
      id: 'annotation-batch',
      tone: 'ok',
      title: done(count),
      body: '按「復原」可以還原。',
      action: { label: '復原', icon: 'rotate-ccw', run: () => this.restore(before) },
    });
  }

  /** 選取的步驟標同一隻手；wholeNote 時連同這些音符的其他步驟一起標（Shift+A／D）。 */
  pickHand(hand: Hand, wholeNote = false): void {
    const picked = this.pickedSteps;
    const ids = new Set(picked.map((step) => step.note.id));
    const steps = wholeNote ? this.steps.filter((step) => ids.has(step.note.id)) : picked;
    this.#batch(
      () => this.setHandMany(steps, hand),
      (count) => `已把 ${count} 步標成${hand === 'L' ? '左手' : '右手'}`,
      '沒有選取的步驟',
    );
  }

  pickConfidence(confidence: Confidence): void {
    const steps = this.pickedSteps;
    this.#batch(
      () => this.setConfidenceMany(steps, confidence),
      (count) => `已把 ${count} 步的信心設成「${CONFIDENCE_LABEL[confidence]}」`,
      '沒有選取的步驟',
    );
  }

  pickConfirm(): void {
    const steps = this.pickedSteps;
    this.#batch(() => this.confirmMany(steps), (count) => `已確認 ${count} 步的預填`, '選取的步驟沒有預填');
  }

  pickClear(): void {
    const steps = this.pickedSteps;
    this.#batch(() => this.clearHandsMany(steps), (count) => `已清除 ${count} 步的手`, '選取的步驟還沒標手');
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
      this.draft.groups = [];
      this.draft.memo = '';
      this.draft.branches = [];
      this.draft.route = [];
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

  // 切換路線都不影響任何手順。

  /** 換路線並記下正在設定的線；有變才存。 */
  #setRoute(route: string[], active: string): void {
    const same = route.length === this.draft.route.length && route.every((line, index) => line === this.draft.route[index]);
    if (same && active === this.draft.active) return;
    this.draft.route = route;
    this.draft.active = active;
    this.#scheduleSave();
  }

  /**
   * 在 time 這一步改走 line（主線也可以）：只換這一段，其他段落選的分支照舊；
   * 這一段原本走的分支、和 line 同一段衝突的分支才換掉。分支圖、這一步的打法與 B 鍵用這個。
   */
  pickLine(line: string, time: number): void {
    if (line !== MAIN_LINE && !this.branch(line)) return;
    this.#setRoute(pickRoute(this.draft.branches, this.draft.route, line, time), line);
  }

  /** 「打法」分頁點一條線：分支就是走它（它自己那一段）；主線是所有分支都不走。 */
  focusLine(line: string): void {
    const branch = this.branch(line);
    if (branch) this.pickLine(line, branch.from);
    else this.useRoute(MAIN_LINE);
  }

  /** 路線改成只走這一條線（主線：所有分支都不走），例如「打法」分頁路線圖的開頭與結尾。 */
  useRoute(line: string): void {
    const next = line === MAIN_LINE || this.branch(line) ? line : MAIN_LINE;
    this.#setRoute(lineRoute(next), next);
  }

  /** 路線改成這組選擇（比對結果的「切到這條／這組」）；「打法」分頁設定最後一個選的分支。 */
  usePicks(picks: readonly string[]): void {
    const route = cleanRoute(this.draft.branches, picks);
    this.#setRoute(route, route.at(-1) ?? MAIN_LINE);
  }

  /** 目前路線和這組選擇每個時刻走的線都一樣。 */
  isRoute(picks: readonly string[]): boolean {
    return sameRoute(this.draft.branches, this.draft.route, picks);
  }

  /** 只走某一條線時這一步的標註。 */
  #sourceOnLine(line: string, note: Note, part: StepPart): NoteAnnotation | undefined {
    return stepSource(this.draft, lineRoute(line), note, part);
  }

  /** 把 source 這一步負責的欄位複製進 marks 裡這顆的那一筆（深拷貝換手）；這一步是否預填跟著 source。 */
  #copyPart(marks: Record<string, NoteAnnotation>, note: Note, part: StepPart, source: NoteAnnotation | undefined): void {
    if (!note.key || !source) return;
    const out: NoteAnnotation = marks[note.key] ? { ...marks[note.key] } : { key: note.key };
    // 滑行的信心：複製滑行時取來源那一步的；複製起點時保持這一筆原本滑行的。
    const existing = marks[note.key];
    const trackConfidence =
      part === 'track' ? stepConfidence(note, source, 'track') : existing ? stepConfidence(note, existing, 'track') : undefined;
    const prefilled = prefilledParts(existing);
    const record = out as unknown as Record<string, unknown>;
    for (const field of partFields(note, part)) {
      const value = source[field];
      if (value === undefined) delete record[field];
      else record[field] = field === 'handovers' ? (value as HandoverMark[]).map((item) => ({ ...item })) : value;
    }
    setTrackConfidence(note, out, trackConfidence);
    setPrefilled(out, { ...prefilled, [part]: isPrefilledPart(source, part) });
    if (source.by && !out.by) out.by = source.by;
    marks[note.key] = out;
  }

  /** 刪掉這一步負責的欄位；那一筆變空時整筆移除。 */
  #dropPart(marks: Record<string, NoteAnnotation>, note: Note, part: StepPart): void {
    const mark = note.key ? marks[note.key] : undefined;
    if (!mark) return;
    const trackConfidence = part === 'hand' ? stepConfidence(note, mark, 'track') : undefined;
    const prefilled = prefilledParts(mark);
    const out = { ...mark } as unknown as Record<string, unknown>;
    for (const field of partFields(note, part)) delete out[field];
    const next = out as unknown as NoteAnnotation;
    // 拿掉起點的信心後，滑行的信心要保持原本的值。
    if (part === 'hand') setTrackConfidence(note, next, trackConfidence);
    setPrefilled(next, { ...prefilled, [part]: false });
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
    const parent = links.parent && this.#covers(links.parent, lo) ? links.parent : this.routeAt(lo);
    const merge = links.merge && this.#covers(links.merge, hi) ? links.merge : this.routeAt(hi);
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
    // 這一段改走新分支，其他段落選的分支不變。
    this.draft.route = pickRoute(this.draft.branches, this.draft.route, branch.id, lo);
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
    // 範圍變了可能和路線上另一段的分支重疊：衝突時留後選的。
    this.draft.route = cleanRoute(this.draft.branches, this.draft.route);
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
    this.draft.route = cleanRoute(this.draft.branches, this.draft.route);
    this.#touch();
    return true;
  }

  /**
   * 刪除分支；從它分出或併入它的分支改接它原本的路線（那個時間實際走的線）。
   * 目前路線有選它時，那一段改走它分出的線（其他段落選的分支不變）。回傳刪除前的草稿供復原。
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
    this.draft.route = cleanRoute(
      this.draft.branches,
      this.draft.route.map((line) => (line === id ? branch.parent : line)),
    );
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

  // ---- 分組（以顆為單位） ----
  //
  // 在逐顆清單上標：先標這組的第一顆（右鍵或 G），再選最後一顆，兩顆之間（含兩端）的
  // 音符就是一組。之後可以單顆移出或加入。

  /** 正在標的分組第一顆（音符 id）；null 表示沒有在標。 */
  groupStart = $state<string | null>(null);

  /** 盤面上的結果就是這筆紀錄的原文，才可以標分組。 */
  get groupsAvailable(): boolean {
    const record = records.get(this.recordId);
    return this.keysReady && !!record && session.result?.source === record.source;
  }

  get groupStartNote(): Note | null {
    return this.groupStart ? (this.ordered.find((note) => note.id === this.groupStart) ?? null) : null;
  }

  /** 分組的音符（依時間）；舊版時間範圍的分組取範圍內的音符。 */
  groupNotes(group: NoteGroup): Note[] {
    if (group.keys.length === 0) {
      const from = group.from ?? 0;
      const to = group.to ?? -1;
      return this.ordered.filter((note) => note.timeSeconds >= from - 1e-3 && note.timeSeconds <= to + 1e-3);
    }
    return group.keys
      .map((key) => this.#byKey.get(key))
      .filter((note): note is Note => !!note)
      .sort((a, b) => this.noteNumber(a) - this.noteNumber(b));
  }

  /** 依第一顆排序的分組；index 是在 draft.groups 裡的位置。 */
  groupList = $derived.by(() =>
    this.draft.groups
      .map((group, index) => ({ index, group, notes: this.groupNotes(group) }))
      .sort(
        (a, b) =>
          (a.notes[0] ? this.noteNumber(a.notes[0]) : Infinity) - (b.notes[0] ? this.noteNumber(b.notes[0]) : Infinity) ||
          b.notes.length - a.notes.length,
      ),
  );

  /** 音符 id → 包含它的分組（draft.groups 的位置）。 */
  #groupsByNote = $derived.by(() => {
    const map = new Map<string, number[]>();
    for (const { index, notes } of this.groupList) {
      for (const note of notes) map.set(note.id, [...(map.get(note.id) ?? []), index]);
    }
    return map;
  });

  groupsOf(note: Note): number[] {
    return this.#groupsByNote.get(note.id) ?? [];
  }

  /** 分組的名稱：有取名用名稱，否則用顆數範圍。 */
  groupName(index: number): string {
    const group = this.draft.groups[index];
    if (!group) return '';
    if (group.label) return group.label;
    const notes = this.groupNotes(group);
    if (notes.length === 0) return '分組（找不到音符）';
    return `第 ${this.noteNumber(notes[0])}–${this.noteNumber(notes[notes.length - 1])} 顆`;
  }

  /** 兩顆之間（含兩端，依時間）的音符。 */
  notesBetween(a: Note, b: Note): Note[] {
    const [from, to] = [this.noteNumber(a), this.noteNumber(b)].sort((x, y) => x - y);
    return this.ordered.slice(from - 1, to);
  }

  /** 第一次記下第一顆，第二次以兩顆之間（含兩端）建立分組。回傳結果給呼叫端顯示提示。 */
  markGroup(note: Note): { kind: 'start' | 'created' | 'exists' | 'too-short'; index?: number } | null {
    if (!this.groupsAvailable || !note.key) return null;
    const start = this.groupStartNote;
    if (!start) {
      this.groupStart = note.id;
      return { kind: 'start' };
    }
    const notes = this.notesBetween(start, note);
    if (notes.length < 2) return { kind: 'too-short' };
    this.groupStart = null;
    return this.createGroup(notes);
  }

  /** 以這些音符（不必相連）建立一組；同樣的分組已經存在時不重複建立。 */
  createGroup(notes: Note[]): { kind: 'created' | 'exists' | 'too-short'; index?: number } | null {
    if (!this.groupsAvailable) return null;
    const keys = [...new Set(notes)]
      .filter((item) => !!item.key)
      .sort((a, b) => this.noteNumber(a) - this.noteNumber(b))
      .map((item) => item.key ?? '');
    if (keys.length < 2) return { kind: 'too-short' };
    const group: NoteGroup = { keys, ...(this.annotator ? { by: this.annotator } : {}) };
    const existing = this.draft.groups.findIndex((item) => sameGroup(this.#resolved(item), group));
    if (existing >= 0) return { kind: 'exists', index: existing };
    this.draft.groups.push(group);
    this.#touch();
    return { kind: 'created', index: this.draft.groups.length - 1 };
  }

  cancelGroup(): void {
    this.groupStart = null;
  }

  /** 舊版時間範圍的分組換成音符後的樣子（不改動草稿）。 */
  #resolved(group: NoteGroup): NoteGroup {
    if (group.keys.length > 0 || !this.keysReady) return group;
    const { from: _from, to: _to, ...rest } = group;
    return { ...rest, keys: this.groupNotes(group).map((note) => note.key ?? '').filter((key) => key !== '') };
  }

  /** 改動前把舊版時間範圍的分組換成音符。 */
  #materialize(group: NoteGroup): void {
    if (group.keys.length > 0 || !this.keysReady) return;
    group.keys = this.groupNotes(group).map((note) => note.key ?? '').filter((key) => key !== '');
    delete group.from;
    delete group.to;
  }

  /** 匯出前把舊版時間範圍的分組都換成音符。 */
  #materializeAll(): void {
    if (!this.keysReady || this.draft.groups.every((group) => group.keys.length > 0)) return;
    for (const group of this.draft.groups) this.#materialize(group);
    this.#touch();
  }

  /** 目前分組的複本，給復原用。 */
  snapshotGroups(): NoteGroup[] {
    return $state.snapshot(this.draft.groups) as NoteGroup[];
  }

  restoreGroups(groups: NoteGroup[]): void {
    this.draft.groups = groups.map((group) => ({ ...group, keys: [...group.keys] }));
    this.#touch();
  }

  /** 還沒確認的預填分組數。 */
  get prefilledGroups(): number {
    return this.draft.groups.filter((group) => group.prefilled).length;
  }

  /**
   * 以 Rust 的自動分組預填：只加和現有分組（含預填）沒有共用音符的組，標成預填。
   * 回傳加了幾組。
   */
  prefillGroups(groups: string[][]): number {
    if (!this.groupsAvailable) return 0;
    const taken = new Set(this.draft.groups.flatMap((group) => this.#resolved(group).keys));
    let added = 0;
    for (const keys of groups) {
      const notes = keys.map((key) => this.#byKey.get(key)).filter((note): note is Note => !!note);
      if (notes.length < 2 || notes.length !== keys.length || keys.some((key) => taken.has(key))) continue;
      const sorted = notes.sort((a, b) => this.noteNumber(a) - this.noteNumber(b)).map((note) => note.key ?? '');
      this.draft.groups.push({ keys: sorted, prefilled: true });
      for (const key of sorted) taken.add(key);
      added += 1;
    }
    if (added > 0) this.#touch();
    return added;
  }

  /** 確認預填的分組：之後算真人分組，記上標註者。 */
  #own(group: NoteGroup): void {
    if (!group.prefilled) return;
    delete group.prefilled;
    if (this.annotator && !group.by) group.by = this.annotator;
  }

  confirmGroup(index: number): void {
    const group = this.draft.groups[index];
    if (!group?.prefilled) return;
    this.#own(group);
    this.#touch();
  }

  /** 確認全部預填分組；回傳確認了幾組。 */
  confirmAllGroups(): number {
    let count = 0;
    for (const group of this.draft.groups) {
      if (!group.prefilled) continue;
      this.#own(group);
      count += 1;
    }
    if (count > 0) this.#touch();
    return count;
  }

  /** 刪除還沒確認的預填分組；回傳刪了幾組。 */
  clearPrefilledGroups(): number {
    const before = this.draft.groups.length;
    this.draft.groups = this.draft.groups.filter((group) => !group.prefilled);
    const count = before - this.draft.groups.length;
    if (count > 0) this.#touch();
    return count;
  }

  /** 改名或增減音符都算確認了這一組。 */
  renameGroup(index: number, label: string): void {
    const group = this.draft.groups[index];
    if (!group) return;
    const text = label.trim().slice(0, 40);
    if ((group.label ?? '') === text) return;
    this.#materialize(group);
    this.#own(group);
    if (text) group.label = text;
    else delete group.label;
    this.#touch();
  }

  removeGroup(index: number): void {
    if (index < 0 || index >= this.draft.groups.length) return;
    this.draft.groups.splice(index, 1);
    this.#touch();
  }

  /** 把這顆移出分組；剩不到兩顆就整組刪除。回傳是否整組刪除。 */
  removeFromGroup(index: number, note: Note): boolean {
    const group = this.draft.groups[index];
    if (!group || !note.key) return false;
    this.#materialize(group);
    this.#own(group);
    group.keys = group.keys.filter((key) => key !== note.key);
    if (group.keys.length < 2) {
      this.draft.groups.splice(index, 1);
      this.#touch();
      return true;
    }
    this.#touch();
    return false;
  }

  addToGroup(index: number, note: Note): void {
    const group = this.draft.groups[index];
    if (!group || !note.key) return;
    this.#materialize(group);
    if (group.keys.includes(note.key)) return;
    this.#own(group);
    group.keys = [...group.keys, note.key]
      .map((key) => this.#byKey.get(key))
      .filter((item): item is Note => !!item)
      .sort((a, b) => this.noteNumber(a) - this.noteNumber(b))
      .map((item) => item.key ?? '');
    this.#touch();
  }

  /** 可以把這顆加入的分組：不含這顆、但範圍就在這顆前後一顆以內。 */
  nearbyGroups(note: Note): number[] {
    const number = this.noteNumber(note);
    return this.groupList
      .filter(({ notes }) => {
        if (notes.length === 0 || notes.some((item) => item.id === note.id)) return false;
        return number >= this.noteNumber(notes[0]) - 1 && number <= this.noteNumber(notes[notes.length - 1]) + 1;
      })
      .map(({ index }) => index);
  }

  /** 分組的播放範圍：第一顆判定到最後一顆結束（Slide 到滑完）。 */
  groupSpan(index: number): { from: number; to: number } | null {
    const group = this.draft.groups[index];
    const notes = group ? this.groupNotes(group) : [];
    if (notes.length === 0) return null;
    return {
      from: notes[0].timeSeconds,
      to: Math.max(...notes.map((note) => Math.max(note.timeSeconds, note.endSeconds, note.motionEnd ?? 0))),
    };
  }

  /** 目前草稿連同原譜轉成標註檔。 */
  async toFile(): Promise<HandAnnotation | null> {
    const source = session.result?.source ?? session.source;
    if (!source || !this.keysReady) return null;
    if (this.groupsAvailable) this.#materializeAll();
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
   * A／D 標目前這一步的左右手（Shift 整顆同一隻手）、S 切換信心、Enter 確認這一步的預填、
   * Delete 清除、N 下一步未標。
   */
  handleKey(event: KeyboardEvent): boolean {
    const key = event.key.toLowerCase();
    if (event.key === 'Escape' && this.pendingStart) {
      this.cancelMark();
      return true;
    }
    if (this.picking) {
      if (event.key === 'Escape') {
        this.endPicking();
        return true;
      }
      if (key === 'a' || key === 'd') {
        this.pickHand(key === 'a' ? 'L' : 'R', event.shiftKey);
        return true;
      }
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
      // 只確認看著的這一步；Slide 的另一步要到那一步自己確認。
      if (this.#confirmParts(note, [step.part])) this.#touch();
      this.#advance(step);
      return true;
    }
    if (event.key === 'Delete' || event.key === 'Backspace') {
      this.clear(note);
      return true;
    }
    return false;
  }

  // ---- 比對與比對暫存 ----

  /** 照某組選擇走時展開後有人確認過的步驟（預填的步驟拿掉）。 */
  #confirmedFor(picks: readonly string[]): NoteAnnotation[] {
    const marks = routeMarks(this.draft, this.ordered, picks);
    return this.ordered
      .map((note) => humanMark(note.key ? marks[note.key] : undefined))
      .filter((mark): mark is NoteAnnotation => mark !== undefined);
  }

  /** 比對的走法（2026-11-09 起）：每段主線以外的組合；id 是選的分支以 + 連接（只選一條時就是分支 id）。 */
  #compareOptions = $derived(
    routeSections(this.draft.branches).map((option) => ({ ...option, id: option.picks.join('+') })),
  );

  /**
   * 比對送出的手順：主線在最前，接著每段的每種走法各自展開（只換那一段、其他照主線）並帶段號；
   * Rust 每段挑一種，採用整首吻合率最高的組合，所以和目前路線無關。
   */
  #compareRoutes = $derived<RouteAnnotation[]>([
    { id: MAIN_LINE, name: MAIN_NAME, notes: this.#confirmedFor([]) },
    ...this.#compareOptions.map((option) => ({
      id: option.id,
      name: option.picks.map((id) => this.lineName(id)).join('＋'),
      notes: this.#confirmedFor(option.picks),
      group: option.section,
    })),
  ]);

  /** 比對結果裡某種走法選的分支：照這次送出的對照；之後刪掉或改過的，照 id 拆開、只留還在的分支。 */
  optionPicks(id: string): string[] {
    if (id === MAIN_LINE) return [];
    const option = this.#compareOptions.find((item) => item.id === id);
    return option ? option.picks : id.split('+').filter((pick) => this.branch(pick) !== undefined);
  }

  /** 這種走法選的分支都還在。 */
  optionExists(id: string): boolean {
    return id === MAIN_LINE || this.#compareOptions.some((item) => item.id === id) || id.split('+').every((pick) => this.branch(pick) !== undefined);
  }

  /** 走法名稱：選的分支名稱以「＋」連接；有分支已經刪掉時用比對當時的名稱。 */
  optionName(id: string, saved = ''): string {
    if (id === MAIN_LINE) return MAIN_NAME;
    const picks = id.split('+');
    return picks.every((pick) => this.branch(pick)) ? picks.map((pick) => this.lineName(pick)).join('＋') : saved || id;
  }

  /** 走法的顏色：最後選的那條分支（同段裡最裡層的）。 */
  optionColor(id: string): string {
    return this.lineColor(this.optionPicks(id).at(-1) ?? MAIN_LINE);
  }

  /** 比對採用的幾種走法合成一組路線選擇。 */
  chosenPicks(ids: readonly string[]): string[] {
    return cleanRoute(
      this.draft.branches,
      ids.flatMap((id) => this.optionPicks(id)),
    );
  }

  #marksPrint = $derived(routesPrint(this.#compareRoutes));

  #sourceHash = $derived(records.get(this.recordId)?.sourceHash ?? null);

  /** 比對暫存的格子：路線（沒有分支為主線，有分支為全部比過取最高）＋起始秒數（和 toFile 取同一個）＋已投影的參數。 */
  #slotFor(config: SolverConfig): string {
    const firstSeconds = session.result?.firstSeconds ?? session.firstSeconds;
    const route = this.draft.branches.length > 0 ? BEST_ROUTE_SLOT : MAIN_LINE;
    return `${route}|${firstSeconds}|${stableJson(config)}`;
  }

  /** 目前參數（含模型）的格子。 */
  #slot = $derived(this.#slotFor(session.requestConfig));

  #entry = $derived.by<StoredEvaluation | null>(() => {
    const entry = this.#evaluations[this.#slot];
    return entry && entry.sourceHash === this.#sourceHash ? entry : null;
  });

  /** 目前參數與模型的比對結果；可能是先前暫存的。 */
  get evaluation(): EvaluateResponse | null {
    return this.#entry?.response ?? null;
  }

  /** 顯示的比對結果是標註改過之前做的。 */
  get evaluationOutdated(): boolean {
    return this.#entry !== null && this.#entry.marks !== this.#marksPrint;
  }

  /** 目前這一格正在比對。 */
  get evaluating(): boolean {
    return this.#running !== null && this.#running === this.#slot;
  }

  get evaluationError(): string | null {
    return this.#error?.slot === this.#slot ? this.#error.message : null;
  }

  /** 模型選單用：在目前參數下換成這個評分方式時暫存的吻合率（有分支時是採用的那條）；沒比對過回傳 null。 */
  agreementFor(choice: ScoringChoice): AgreementPreview | null {
    const entry = this.#evaluations[this.#slotFor(projectConfig(withScoring(session.config, choice)))];
    if (!entry || entry.sourceHash !== this.#sourceHash) return null;
    const { agreed, compared } = entry.response;
    return {
      percent: compared > 0 ? Math.round((1000 * agreed) / compared) / 10 : null,
      outdated: entry.marks !== this.#marksPrint,
    };
  }

  /** 設定裡清除暫存之後呼叫：記憶體裡的比對暫存一起丟掉。 */
  forgetEvaluations(): void {
    this.#evaluations = {};
    this.#error = null;
  }

  /** 讀進這筆紀錄的比對暫存；版本或原文對不上的直接刪掉。 */
  async #loadEvaluations(recordId: string): Promise<void> {
    const [version, stored] = await Promise.all([
      evaluationVersion(),
      dbGetAllByIndex<StoredEvaluation>(STORE_EVALUATIONS, 'recordId', recordId),
    ]);
    const sourceHash = records.get(recordId)?.sourceHash;
    if (this.recordId !== recordId || !sourceHash) return;
    const valid: StoredEvaluation[] = [];
    const invalid: string[] = [];
    for (const entry of stored) {
      if (entry.version === version && entry.sourceHash === sourceHash && typeof entry.slot === 'string' && entry.response) {
        valid.push(entry);
      } else {
        invalid.push(entry.key);
      }
    }
    // 載入期間剛比對完的比資料庫裡的新。
    const { kept, dropped } = pruneEvaluations(Object.values({ ...bySlot(valid), ...this.#evaluations }));
    this.#evaluations = bySlot(kept);
    void dbDeleteKeys(STORE_EVALUATIONS, [...invalid, ...dropped]);
  }

  /** 把一次比對存成暫存：資料庫一定寫；還是同一筆紀錄才放進記憶體。 */
  async #remember(entry: StoredEvaluation): Promise<void> {
    let dropped: string[] = [];
    if (this.recordId === entry.recordId) {
      const pruned = pruneEvaluations(Object.values({ ...this.#evaluations, [entry.slot]: entry }));
      this.#evaluations = bySlot(pruned.kept);
      dropped = pruned.dropped;
    }
    if (await dbPut(STORE_EVALUATIONS, entry)) await dbDeleteKeys(STORE_EVALUATIONS, dropped);
  }

  /**
   * 比對頁的模型選單：換評分方式（和參數頁是同一個設定），畫面立刻顯示這個模型暫存的比對；
   * 盤面跟著換成這個模型的分析（有分析快取直接載入，沒有才重新分析），
   * 沒有暫存或標註之後改過時再比對一次。瀏覽器預覽沒有核心，只換設定。
   */
  async compareWith(choice: ScoringChoice): Promise<void> {
    session.setScoring(choice);
    const token = ++this.#switchToken;
    const source = session.result?.source ?? session.source;
    if (!session.desktop || !source || session.configIssues.length > 0) {
      this.switching = false;
      return;
    }
    this.switching = true;
    try {
      if (session.stale || !session.result) {
        await session.load(source);
        if (token !== this.#switchToken) return;
      }
      // 分析失敗或被別的動作取代：不自動比對，留給「開始比對」。
      if (session.stale || !session.result || session.result.response.status !== 'ok') return;
      if (this.#compareRoutes.every((route) => route.notes.length === 0)) return;
      if (this.#entry && !this.evaluationOutdated) return;
      await this.evaluate();
    } finally {
      if (token === this.#switchToken) this.switching = false;
    }
  }

  /**
   * 比對：Rust 求模型最佳解，主線與每條打法路線各自和它比，採用吻合率最高的那條求照標註的最佳解。
   * 只送有人確認過的步驟（預填的步驟拿掉）。
   */
  async evaluate(): Promise<void> {
    const file = await this.toFile();
    const recordId = this.recordId;
    if (!file || !recordId) return;
    // Rust 不認分支結構：主線放在 notes，其他路線各自展開後放在 routes。
    const sent = $state.snapshot(this.#compareRoutes) as RouteAnnotation[];
    const [main, ...routes] = sent;
    file.notes = main.notes;
    delete file.branches;
    delete file.groups;
    const solverConfig = projectConfig(session.config);
    const slot = this.#slotFor(solverConfig);
    const marks = routesPrint(sent);
    // 目前的分析就是同一份原文、同一組設定時，直接用它的第一名當模型解，省掉重算
    // （Temoto 重算一次要十秒以上）。動作軌跡比對用不到，不送。
    const result = session.result;
    const top = result?.response.status === 'ok' ? result.response.solutions[0] : undefined;
    const modelSolution =
      top &&
      result &&
      result.source === file.chart.source &&
      result.firstSeconds === file.chart.firstSeconds &&
      configEquals(result.config, solverConfig)
        ? { ...$state.snapshot(top), leftSegments: [], rightSegments: [], palmPlacements: [] }
        : undefined;
    const token = ++this.#runToken;
    this.#running = slot;
    if (this.#error?.slot === slot) this.#error = null;
    try {
      const [response, version] = await Promise.all([
        evaluateAnnotation({
          requestId: nextRequestId(),
          source: file.chart.source,
          firstSeconds: file.chart.firstSeconds,
          solverConfig,
          annotation: file,
          modelSolution,
          routes,
        }),
        evaluationVersion(),
      ]);
      // 比對期間換了譜面：結果照樣存成那一筆的暫存，不顯示在現在這一筆。
      // 記憶體裡的暫存立刻更新，寫資料庫不必等。
      void this.#remember({
        key: `${recordId}|${slot}`,
        recordId,
        slot,
        version,
        sourceHash: file.chart.sha256,
        marks,
        createdAt: Date.now(),
        response,
      });
    } catch (error) {
      // 換譜、換模型或重新分析取代了這次比對，不算錯誤。
      if (error !== CANCELLED && this.recordId === recordId) {
        this.#error = { slot, message: typeof error === 'string' ? error : String(error) };
      }
    } finally {
      if (token === this.#runToken) this.#running = null;
    }
  }
}

export const annotation = new AnnotationStore();
