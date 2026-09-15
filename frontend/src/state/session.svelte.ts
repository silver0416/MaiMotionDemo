import { analyzeChart, isDesktop, nextRequestId } from '../lib/api';
import { DEFAULT_CONFIG, cloneConfig, configEquals, validateConfig } from '../lib/contract';
import { buildTrack, type Track } from '../lib/motion';
import type { Sample } from '../lib/samples';
import type {
  AnalyzeRequest,
  AnalyzeResponse,
  Assignment,
  Chart,
  Handover,
  Note,
  SlidePath,
  Solution,
  SolverConfig,
} from '../lib/types';
import { playback } from './playback.svelte';

export type Phase = 'empty' | 'analyzing' | 'ready';
export type Origin = 'live' | 'sample';

export interface ResultBundle {
  response: AnalyzeResponse;
  /** 產生這份結果的原文，與編輯區分開保存，重新生成不會覆寫編輯區。 */
  source: string;
  config: SolverConfig;
  firstSeconds: number;
  origin: Origin;
  sampleId: string | null;
  receivedAt: number;
}

export interface Bounds {
  start: number;
  end: number;
}

const STARTER_SOURCE = '(120){4}1,2,3,4,5,6,7,8,E';

export class Session {
  readonly desktop = isDesktop();

  source = $state(STARTER_SOURCE);
  firstSeconds = $state(0);
  config = $state<SolverConfig>(cloneConfig(DEFAULT_CONFIG));

  result = $state<ResultBundle | null>(null);
  phase = $state<Phase>('empty');
  /** IPC 失敗或被拒絕（例如核心忙碌）的訊息；不會清掉上一份結果。 */
  errorMessage = $state<string | null>(null);
  lastRequestId = $state<string | null>(null);

  solutionIndex = $state(0);
  selectedNoteId = $state<string | null>(null);

  #pendingRequestId: string | null = null;

  response = $derived<AnalyzeResponse | null>(this.result?.response ?? null);
  chart = $derived<Chart | null>(this.result?.response.chart ?? null);
  solutions = $derived<Solution[]>(this.result?.response.solutions ?? []);
  solution = $derived<Solution | null>(this.solutions[this.solutionIndex] ?? this.solutions[0] ?? null);

  configIssues = $derived(validateConfig(this.config, this.firstSeconds));

  /** 編輯區內容或參數與結果不一致時，畫面必須標示結果不是目前原文的分析。 */
  stale = $derived.by(() => {
    const result = this.result;
    if (!result) return false;
    return (
      result.source !== this.source ||
      result.firstSeconds !== this.firstSeconds ||
      !configEquals(result.config, this.config)
    );
  });

  notes = $derived<Note[]>(this.chart?.notes ?? []);

  noteById = $derived.by(() => {
    const map = new Map<string, Note>();
    for (const note of this.notes) map.set(note.id, note);
    return map;
  });

  pathById = $derived.by(() => {
    const map = new Map<string, SlidePath>();
    for (const path of this.chart?.paths ?? []) map.set(path.id, path);
    return map;
  });

  assignmentsByNote = $derived.by(() => {
    const map = new Map<string, Assignment[]>();
    for (const assignment of this.solution?.assignments ?? []) {
      const list = map.get(assignment.noteId);
      if (list) list.push(assignment);
      else map.set(assignment.noteId, [assignment]);
    }
    return map;
  });

  handoversByNote = $derived.by(() => {
    const map = new Map<string, Handover[]>();
    for (const handover of this.solution?.handovers ?? []) {
      const list = map.get(handover.noteId);
      if (list) list.push(handover);
      else map.set(handover.noteId, [handover]);
    }
    return map;
  });

  leftTrack = $derived<Track | null>(this.solution ? buildTrack(this.solution.leftSegments) : null);
  rightTrack = $derived<Track | null>(this.solution ? buildTrack(this.solution.rightSegments) : null);

  leftStarts = $derived<number[]>(
    this.solution?.leftSegments.map((segment) => segment.startSeconds) ?? [],
  );
  rightStarts = $derived<number[]>(
    this.solution?.rightSegments.map((segment) => segment.startSeconds) ?? [],
  );

  selectedNote = $derived<Note | null>(
    this.selectedNoteId ? (this.noteById.get(this.selectedNoteId) ?? null) : null,
  );

  bounds = $derived.by<Bounds>(() => this.computeBounds());

  hasHands = $derived(this.solution !== null);

  computeBounds(): Bounds {
    const chart = this.chart;
    let start = 0;
    let end = chart?.durationSeconds ?? 1;
    for (const note of chart?.notes ?? []) {
      start = Math.min(start, note.timeSeconds);
      end = Math.max(end, note.endSeconds);
    }
    const solution = this.solution;
    if (solution) {
      for (const segments of [solution.leftSegments, solution.rightSegments]) {
        if (segments.length === 0) continue;
        start = Math.min(start, segments[0].startSeconds);
        end = Math.max(end, segments[segments.length - 1].endSeconds);
      }
    }
    if (!(end > start)) end = start + 1;
    return { start, end };
  }

  setSource(value: string): void {
    this.source = value;
  }

  selectSolution(index: number): void {
    if (index < 0 || index >= this.solutions.length) return;
    this.solutionIndex = index;
    const bounds = this.computeBounds();
    // 候選切換只改軌跡，保留目前播放時間。
    playback.setRange(bounds.start, bounds.end);
  }

  selectNote(noteId: string | null): void {
    this.selectedNoteId = noteId;
  }

  resetConfig(): void {
    this.config = cloneConfig(DEFAULT_CONFIG);
    this.firstSeconds = 0;
  }

  #applyResult(bundle: ResultBundle): void {
    this.result = bundle;
    this.solutionIndex = 0;
    this.selectedNoteId = null;
    this.phase = 'ready';
    const bounds = this.computeBounds();
    playback.resetRange(bounds.start, bounds.end);
  }

  /** 桌面版：呼叫 Rust 核心。瀏覽器不會走到這裡。 */
  async analyze(): Promise<void> {
    if (!this.desktop) {
      this.errorMessage =
        '瀏覽器預覽沒有 Rust 核心，無法分析輸入的原文。請改用桌面版，或載入下方範例檢視預先產生的核心輸出。';
      return;
    }
    if (this.configIssues.length > 0) {
      this.errorMessage = '參數超出核心允許範圍，請先修正「參數」分頁的紅字項目。';
      return;
    }
    const requestId = nextRequestId();
    const request: AnalyzeRequest = {
      requestId,
      source: this.source,
      firstSeconds: this.firstSeconds,
      solverConfig: cloneConfig(this.config),
    };
    this.#pendingRequestId = requestId;
    this.lastRequestId = requestId;
    this.phase = 'analyzing';
    this.errorMessage = null;
    try {
      const response = await analyzeChart(request);
      // 只採用最後一次送出的 requestId，其餘直接丟棄。
      if (this.#pendingRequestId !== requestId) return;
      if (response.requestId !== requestId) {
        this.errorMessage = `核心回傳的 requestId（${response.requestId}）與這次請求不符，已忽略。`;
        this.phase = this.result ? 'ready' : 'empty';
        this.#pendingRequestId = null;
        return;
      }
      this.#applyResult({
        response,
        source: request.source,
        config: request.solverConfig,
        firstSeconds: request.firstSeconds,
        origin: 'live',
        sampleId: null,
        receivedAt: Date.now(),
      });
      this.#pendingRequestId = null;
    } catch (error) {
      if (this.#pendingRequestId !== requestId) return;
      this.#pendingRequestId = null;
      this.errorMessage = typeof error === 'string' ? error : String(error);
      this.phase = this.result ? 'ready' : 'empty';
    }
  }

  /**
   * 載入範例：一律把原文與參數填回編輯區。
   * 桌面版直接送交 Rust 重新分析；瀏覽器只顯示 fixtures 內附的核心輸出，並標示為範例模式。
   */
  async loadSample(sample: Sample): Promise<void> {
    this.source = sample.source;
    this.firstSeconds = sample.firstSeconds;
    this.config = cloneConfig(sample.config);
    this.errorMessage = null;
    if (this.desktop) {
      await this.analyze();
      return;
    }
    this.#applyResult({
      response: sample.response,
      source: sample.source,
      config: cloneConfig(sample.config),
      firstSeconds: sample.firstSeconds,
      origin: 'sample',
      sampleId: sample.id,
      receivedAt: Date.now(),
    });
  }

  clearResult(): void {
    this.result = null;
    this.phase = 'empty';
    this.selectedNoteId = null;
    this.solutionIndex = 0;
    this.errorMessage = null;
  }
}

export const session = new Session();
