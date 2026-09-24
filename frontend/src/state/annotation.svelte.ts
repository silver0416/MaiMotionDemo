// 真人手順標註的編輯狀態：跟著目前開啟的譜面紀錄，改動後自動寫回紀錄。

import { evaluateAnnotation, nextRequestId } from '../lib/api';
import {
  type AnnotationDraft,
  cleanDraft,
  emptyDraft,
  isHumanLabeled,
  merge,
  neededParts,
  prefill,
  serialize,
  toFile,
  type MergeResult,
} from '../lib/annotation';
import { projectConfig } from '../lib/contract';
import { hashText } from '../lib/db';
import type { VideoLink } from '../lib/video';
import type {
  Confidence,
  EvaluateResponse,
  Hand,
  HandAnnotation,
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
export type StepPart = 'hand' | 'track';

export interface Step {
  key: string;
  note: Note;
  part: StepPart;
  /** 這一步的時間：起點為判定時間，滑行為開始移動的時間 */
  time: number;
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
        const time = note.motionStart ?? note.timeSeconds;
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
    this.draft = (record && cleanDraft(record.annotation)) ?? emptyDraft(record ? recordTitle(record) : '');
    if (record && !this.draft.title) this.draft.title = recordTitle(record);
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

  mark(note: Note | null): NoteAnnotation | undefined {
    return note?.key ? this.draft.notes[note.key] : undefined;
  }

  /** 修改一顆的標註；只要有人動過就不再是預填。內容清空時移除整筆。 */
  #edit(note: Note, change: (mark: NoteAnnotation) => void): void {
    if (!note.key) return;
    const current = this.draft.notes[note.key];
    const mark: NoteAnnotation = current ? { ...current } : { key: note.key };
    change(mark);
    delete mark.prefilled;
    if (this.annotator) mark.by = this.annotator;
    const empty =
      mark.hand === undefined && mark.track === undefined && !mark.memo && mark.confidence === undefined;
    if (empty) delete this.draft.notes[note.key];
    else this.draft.notes[note.key] = mark;
    this.#touch();
  }

  /**
   * 分步標註時只改一個部分；如果原本是模型預填，另一部分也一起清掉，
   * 留給那一步自己標，免得沒看過的預填被當成真人資料。
   */
  #dropOtherPrefill(note: Note, mark: NoteAnnotation, part: StepPart): void {
    const need = neededParts(note);
    if (!this.mark(note)?.prefilled || !need.hand || !need.track) return;
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

  /** 快捷鍵 A／D：這顆需要的部分都設成同一隻手。 */
  setAll(note: Note, hand: Hand): void {
    const need = neededParts(note);
    this.#edit(note, (mark) => {
      if (need.hand) mark.hand = hand;
      if (need.track) mark.track = hand;
    });
  }

  setConfidence(note: Note, confidence: Confidence): void {
    this.#edit(note, (mark) => {
      if (confidence === 'sure') delete mark.confidence;
      else mark.confidence = confidence;
    });
  }

  cycleConfidence(note: Note): void {
    const order: Confidence[] = ['sure', 'unsure', 'either'];
    const current = this.mark(note)?.confidence ?? 'sure';
    this.setConfidence(note, order[(order.indexOf(current) + 1) % order.length]);
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
    if (!note.key || !this.draft.notes[note.key]) return;
    delete this.draft.notes[note.key];
    this.#touch();
  }

  /** 把預填確認為真人資料（手順不變）。 */
  confirm(note: Note): void {
    const mark = this.mark(note);
    if (!mark?.prefilled) return;
    this.#edit(note, () => {});
  }

  confirmAll(): number {
    let count = 0;
    for (const note of this.ordered) {
      const mark = this.mark(note);
      if (!mark?.prefilled) continue;
      const next = { ...mark };
      delete next.prefilled;
      if (this.annotator) next.by = this.annotator;
      this.draft.notes[mark.key] = next;
      count += 1;
    }
    if (count > 0) this.#touch();
    return count;
  }

  /** 以目前盤面的模型方案預填還沒標的音符。 */
  prefillFromModel(): number {
    const solution = session.solutions[session.solutionIndex] ?? session.solutions[0];
    if (!solution) return 0;
    const count = prefill(this.draft, this.ordered, solution);
    if (count > 0) this.#touch();
    return count;
  }

  /** 移除所有尚未確認的預填。 */
  clearPrefilled(): number {
    let count = 0;
    for (const [key, mark] of Object.entries(this.draft.notes)) {
      if (!mark.prefilled) continue;
      delete this.draft.notes[key];
      count += 1;
    }
    if (count > 0) this.#touch();
    return count;
  }

  /** 各種清除範圍會影響的數量，用在按鈕與確認文字。 */
  counts(): { hands: number; memos: number } {
    let hands = 0;
    let memos = 0;
    for (const mark of Object.values(this.draft.notes)) {
      if (mark.hand !== undefined || mark.track !== undefined) hands += 1;
      if (mark.memo) memos += 1;
    }
    memos += this.draft.ranges.length + (this.draft.memo.trim() ? 1 : 0);
    return { hands, memos };
  }

  /**
   * 一次清除：hands 只清手順（含預填、換手與信心，備註保留）、memos 只清所有備註、
   * all 全部清空。回傳清除前的草稿，供復原。
   */
  clearAll(scope: 'hands' | 'memos' | 'all'): AnnotationDraft {
    const before = $state.snapshot(this.draft) as AnnotationDraft;
    if (scope === 'all') {
      this.draft.notes = {};
      this.draft.ranges = [];
      this.draft.memo = '';
    } else {
      const notes: Record<string, NoteAnnotation> = {};
      for (const [key, mark] of Object.entries(before.notes)) {
        if (scope === 'hands' && mark.memo) {
          notes[key] = { key, memo: mark.memo, ...(mark.by ? { by: mark.by } : {}) };
        } else if (scope === 'memos' && (mark.hand !== undefined || mark.track !== undefined)) {
          const next = { ...mark };
          delete next.memo;
          notes[key] = next;
        }
      }
      this.draft.notes = notes;
      if (scope === 'memos') {
        this.draft.ranges = [];
        this.draft.memo = '';
      }
    }
    this.#touch();
    return before;
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
    this.draft.updatedAt = Date.now();
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => this.flush(), SAVE_DELAY);
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
      notes: this.ordered,
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
    this.#touch();
    return result;
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
      if (!isStepDone(step, this.mark(step.note))) {
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
      this.cycleConfidence(note);
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
    file.notes = file.notes.filter((mark) => isHumanLabeled(mark));
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
