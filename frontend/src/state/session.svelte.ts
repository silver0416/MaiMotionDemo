import { analyzeChart, appVersion, isDesktop, nextRequestId } from '../lib/api';
import { STORE_ANALYSES, dbGet, dbPut, hashText } from '../lib/db';
import {
  DEFAULT_DRAFT,
  STATUS_HINT,
  STATUS_LABEL,
  cloneDraft,
  configEquals,
  projectConfig,
  requestModelOf,
  responseMismatch,
  scoringModelOf,
  validateConfig,
} from '../lib/contract';
import { buildTrack, type Track } from '../lib/motion';
import { palmByNote } from '../lib/palm';
import { isFireworkTouch, isTouchNote, sensorKey } from '../lib/touch';
import type {
  AnalyzeRequest,
  AnalyzeStatus,
  AnalyzeResponse,
  Assignment,
  Chart,
  ConfigDraft,
  Handover,
  Note,
  PalmPlacement,
  ScoringModel,
  SlidePath,
  Solution,
  SolverConfig,
  TouchSensor,
} from '../lib/types';
import { playback } from './playback.svelte';
import { toasts } from './toasts.svelte';

export type Phase = 'empty' | 'analyzing' | 'ready';

/** 分析狀態通知共用同一個 id，新的狀態直接取代舊的。 */
export const ANALYSIS_TOAST = 'analysis';

export interface ResultBundle {
  response: AnalyzeResponse;
  /** 產生這份結果的原文；紀錄清單另外保存原文，重新生成不會覆寫紀錄。 */
  source: string;
  /** 實際送出的版本化設定（已投影），用來判斷結果是否過期。 */
  config: SolverConfig;
  firstSeconds: number;
  receivedAt: number;
  /** 從資料庫載入的既有分析，沒有重新呼叫 Rust。 */
  fromCache?: boolean;
}

/** 呼叫核心失敗（IPC 錯誤、版本不符等）時保留的請求，供複製除錯資訊。 */
export interface AnalyzeFailure {
  request: AnalyzeRequest;
  message: string;
  at: number;
}

interface CachedAnalysis {
  key: string;
  sourceHash: string;
  createdAt: number;
  response: AnalyzeResponse;
  config: SolverConfig;
  firstSeconds: number;
}

function stableJson(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, item]) => item !== undefined)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return `{${entries.map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`).join(',')}}`;
}

/**
 * Rust 求解規則的修訂號。改了演算法但 App 版本號沒變時，舊快取（包含 no_solution）
 * 仍會被採用；每次求解行為改變就遞增這個值，讓舊結果自然失效。
 * judgment-1：依判定規則加入滑移最短一幀、Hold 結尾提早放手、Touch 晚接與順帶碰觸。
 * judgment-2：Touch Group 過半判定、Slide 進入最後判定區後可離手；s/z 方向與 pp/qq 形狀修正。
 * judgment-3：Touch 感應區代表點依盤面配置圖重新量測（A 0.80、B 0.465、D 0.87、E 0.645），順帶碰觸距離 0.2 → 0.28。
 * slide-wifi-4：WiFi 三線同時滑行由兩手 2+1 分擔；一般 Slide 除非別的音符需要，否則維持原手。
 */
export const SOLVER_REVISION = 'slide-wifi-4';

/**
 * 個別評分方式的修訂號，只併入該版的快取鍵，不影響其他版本已存在的快取。
 * V3 係數仍在校準；核心調整 V3 排序但 App 版本號沒變時遞增這裡。
 * v3-1：human-motion-v3 首次接入（schemaVersion 4）。
 */
export const SCORING_REVISION: Partial<Record<ScoringModel, string>> = {
  'human-motion-v3': 'v3-1',
};

/** 快取鍵：核心版本＋求解修訂＋原文雜湊＋起始秒數＋已投影的參數。任一項不同就重新分析。 */
async function cacheKey(
  source: string,
  firstSeconds: number,
  config: SolverConfig,
): Promise<{ key: string; sourceHash: string }> {
  const [version, sourceHash] = await Promise.all([appVersion(), hashText(source)]);
  const scoringRevision = SCORING_REVISION[requestModelOf(config)];
  const revision = scoringRevision ? `${SOLVER_REVISION}+${scoringRevision}` : SOLVER_REVISION;
  return {
    key: `${version}|${revision}|${sourceHash}|${firstSeconds}|${stableJson(config)}`,
    sourceHash,
  };
}

export interface Bounds {
  start: number;
  end: number;
}

export class Session {
  readonly desktop = isDesktop();

  /** 目前盤面對應的原文；由譜面紀錄或新增視窗寫入。 */
  source = $state('');
  firstSeconds = $state(0);
  /** 參數頁草稿，兩版欄位並存；送出前才依評分方式投影。 */
  config = $state<ConfigDraft>(cloneDraft(DEFAULT_DRAFT));

  result = $state<ResultBundle | null>(null);
  phase = $state<Phase>('empty');
  /** IPC 失敗或被拒絕（例如核心忙碌）的訊息；不會清掉上一份結果。 */
  errorMessage = $state<string | null>(null);
  lastRequestId = $state<string | null>(null);
  /** 最近一次呼叫核心失敗的請求；成功分析或清除後歸零。 */
  lastFailure = $state<AnalyzeFailure | null>(null);

  solutionIndex = $state(0);
  selectedNoteId = $state<string | null>(null);

  #pendingRequestId: string | null = null;
  /** 開啟紀錄時的快取查詢序號，只套用最後一次。 */
  #loadToken = 0;

  response = $derived<AnalyzeResponse | null>(this.result?.response ?? null);
  chart = $derived<Chart | null>(this.result?.response.chart ?? null);
  solutions = $derived<Solution[]>(this.result?.response.solutions ?? []);
  solution = $derived<Solution | null>(this.solutions[this.solutionIndex] ?? this.solutions[0] ?? null);

  configIssues = $derived(validateConfig(this.config, this.firstSeconds));

  /** 這次會送給 Rust 的設定。 */
  requestConfig = $derived<SolverConfig>(projectConfig(this.config));

  /** 目前結果使用的評分方式；沒有結果時為 null。 */
  resultScoringModel = $derived<ScoringModel | null>(
    this.result ? scoringModelOf(this.result.config) : null,
  );

  /** 參數與結果不一致時，畫面必須標示結果不是目前參數的分析。 */
  stale = $derived.by(() => {
    const result = this.result;
    if (!result) return false;
    return (
      result.source !== this.source ||
      result.firstSeconds !== this.firstSeconds ||
      !configEquals(result.config, this.requestConfig)
    );
  });

  notes = $derived<Note[]>(this.chart?.notes ?? []);

  noteById = $derived.by(() => {
    const map = new Map<string, Note>();
    for (const note of this.notes) map.set(note.id, note);
    return map;
  });

  /** Rust 輸出的 33 個 simai 可指名落點；舊格式沒有這個欄位時當成沒有資料。 */
  touchSensors = $derived<TouchSensor[]>(this.chart?.touchSensors ?? []);

  touchNotes = $derived<Note[]>(this.notes.filter(isTouchNote));

  hasTouchNotes = $derived(this.touchNotes.length > 0);

  /** 這份譜面實際用到的落點，用來加重盤面上對應的參考標記。 */
  usedSensorKeys = $derived.by(() => {
    const keys = new Set<string>();
    for (const note of this.touchNotes) keys.add(sensorKey(note.touchArea, note.button));
    return keys;
  });

  /** 先篩出有煙火的音符，播放時每幀只需檢查這一小組。 */
  fireworkNotes = $derived<Note[]>(this.notes.filter(isFireworkTouch));

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

  /** Rust 的手掌覆蓋結果；舊版核心沒有這個欄位時當成沒有手掌動作。 */
  palmPlacements = $derived<PalmPlacement[]>(this.solution?.palmPlacements ?? []);

  hasPalms = $derived(this.palmPlacements.length > 0);

  /** noteId → 覆蓋它的手掌；音符明細與清單用來標示「一掌覆蓋」。 */
  palmByNoteId = $derived.by(() => palmByNote(this.palmPlacements));

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

  /** 切換評分方式；兩版各自的數值都保留，不互相換算。 */
  setScoringModel(model: ScoringModel): void {
    if (this.config.scoringModel === model) return;
    this.config = { ...this.config, scoringModel: model };
  }

  /** 還原預設數值，但保留目前選擇的評分方式，方便 A/B 對照。 */
  resetConfig(): void {
    this.config = { ...cloneDraft(DEFAULT_DRAFT), scoringModel: this.config.scoringModel };
    this.firstSeconds = 0;
  }

  #applyResult(bundle: ResultBundle): void {
    // 同一份原文只是換參數重新生成：保留播放時間與循環範圍，方便在同一段對照前後差異。
    const sameChart = this.result !== null && this.result.source === bundle.source;
    const previousNote = this.selectedNoteId;
    this.result = bundle;
    this.solutionIndex = 0;
    this.selectedNoteId =
      sameChart && previousNote && bundle.response.chart?.notes.some((note) => note.id === previousNote)
        ? previousNote
        : null;
    this.phase = 'ready';
    const bounds = this.computeBounds();
    if (sameChart) playback.setRange(bounds.start, bounds.end);
    else playback.resetRange(bounds.start, bounds.end);
  }

  /**
   * 桌面版：呼叫 Rust 核心分析 `source`。
   * `accept` 回傳 false 時不套用結果（例如新增視窗裡語法錯誤，盤面保留原本的譜面），
   * 只把核心回應交回呼叫端顯示診斷。瀏覽器預覽沒有核心，直接回傳 null。
   */
  async analyze(
    source: string = this.source,
    accept: (response: AnalyzeResponse) => boolean = () => true,
  ): Promise<AnalyzeResponse | null> {
    if (!this.desktop) {
      this.#fail('瀏覽器預覽沒有 Rust 核心，無法分析譜面。請改用桌面版。');
      return null;
    }
    if (this.configIssues.length > 0) {
      this.#fail('參數超出核心允許範圍，請先修正「參數」分頁的紅字項目。');
      return null;
    }
    const requestId = nextRequestId();
    const request: AnalyzeRequest = {
      requestId,
      source,
      firstSeconds: this.firstSeconds,
      solverConfig: projectConfig(this.config),
    };
    const previousPhase: Phase = this.result ? 'ready' : 'empty';
    this.#pendingRequestId = requestId;
    this.lastRequestId = requestId;
    this.phase = 'analyzing';
    this.errorMessage = null;
    toasts.show({ id: ANALYSIS_TOAST, tone: 'busy', title: '分析中', sticky: true });
    try {
      const response = await analyzeChart(request);
      // 只採用最後一次送出的 requestId，其餘直接丟棄。
      if (this.#pendingRequestId !== requestId) return null;
      this.#pendingRequestId = null;
      if (response.requestId !== requestId) {
        this.phase = previousPhase;
        this.#fail(`核心回傳的 requestId（${response.requestId}）與這次請求不符，已忽略。`, request);
        return null;
      }
      // schemaVersion 與每個方案的 scoringModel、分數形狀都必須屬於這次請求的評分方式。
      const mismatch = responseMismatch(response, requestModelOf(request.solverConfig));
      if (mismatch) {
        this.phase = previousPhase;
        this.#fail(mismatch, request);
        return null;
      }
      if (!accept(response)) {
        this.phase = previousPhase;
        toasts.dismiss(ANALYSIS_TOAST);
        return response;
      }
      this.source = source;
      const bundle: ResultBundle = {
        response,
        source,
        config: request.solverConfig,
        firstSeconds: request.firstSeconds,
        receivedAt: Date.now(),
      };
      this.lastFailure = null;
      this.#applyResult(bundle);
      this.#announce(response);
      void this.#store(bundle);
      return response;
    } catch (error) {
      if (this.#pendingRequestId !== requestId) return null;
      this.#pendingRequestId = null;
      this.phase = previousPhase;
      this.#fail(typeof error === 'string' ? error : String(error), request);
      return null;
    }
  }

  /**
   * 開啟既有紀錄：同一份原文、同樣參數與核心版本分析過就直接從資料庫載入，
   * 否則交給 Rust 分析（結果會寫回資料庫）。
   */
  async load(source: string): Promise<AnalyzeResponse | null> {
    const token = ++this.#loadToken;
    if (this.configIssues.length === 0) {
      const config = projectConfig(this.config);
      const firstSeconds = this.firstSeconds;
      const { key } = await cacheKey(source, firstSeconds, config);
      const cached = await dbGet<CachedAnalysis>(STORE_ANALYSES, key);
      if (token !== this.#loadToken) return null;
      // 快取也要通過版本檢查；舊格式或別版的資料一律重新分析，不嘗試轉換。
      if (
        cached?.response &&
        scoringModelOf(cached.config ?? {}) === requestModelOf(config) &&
        responseMismatch(cached.response, requestModelOf(config)) === null
      ) {
        // 查詢期間不可有其他分析插隊。
        if (this.phase === 'analyzing') return null;
        this.#pendingRequestId = null;
        this.source = source;
        this.errorMessage = null;
        this.lastFailure = null;
        this.lastRequestId = cached.response.requestId;
        this.#applyResult({
          response: cached.response,
          source,
          config: cached.config,
          firstSeconds: cached.firstSeconds,
          receivedAt: cached.createdAt,
          fromCache: true,
        });
        this.#announce(cached.response, true);
        return cached.response;
      }
    }
    if (token !== this.#loadToken) return null;
    return this.analyze(source);
  }

  async #store(bundle: ResultBundle): Promise<void> {
    const { key, sourceHash } = await cacheKey(bundle.source, bundle.firstSeconds, bundle.config);
    const entry: CachedAnalysis = {
      key,
      sourceHash,
      createdAt: bundle.receivedAt,
      response: $state.snapshot(bundle.response) as AnalyzeResponse,
      config: $state.snapshot(bundle.config) as SolverConfig,
      firstSeconds: bundle.firstSeconds,
    };
    await dbPut(STORE_ANALYSES, entry);
  }

  #fail(message: string, request?: AnalyzeRequest): void {
    this.errorMessage = message;
    if (request) this.lastFailure = { request, message, at: Date.now() };
    toasts.show({ id: ANALYSIS_TOAST, tone: 'error', title: '分析未完成', body: message, sticky: true });
  }

  #announce(response: AnalyzeResponse, fromCache = false): void {
    const status = response.status as AnalyzeStatus;
    const label = STATUS_LABEL[status] ?? status;
    if (status === 'ok') {
      const notes = response.chart?.notes.length ?? 0;
      const candidates = response.solutions.length;
      toasts.show({
        id: ANALYSIS_TOAST,
        tone: 'ok',
        title: fromCache ? '已載入先前的分析' : label,
        body: `${notes} 個音符・${candidates} 個候選方案`,
      });
      return;
    }
    const errors = response.diagnostics.filter((item) => item.severity === 'error').length;
    const hint = STATUS_HINT[status] ?? '';
    toasts.show({
      id: ANALYSIS_TOAST,
      tone: 'warn',
      title: label,
      body: errors > 0 ? `${hint} 共 ${errors} 項診斷，詳見「方案」分頁。` : hint,
      sticky: true,
    });
  }

  clearResult(): void {
    this.result = null;
    this.source = '';
    toasts.dismiss(ANALYSIS_TOAST);
    this.phase = 'empty';
    this.selectedNoteId = null;
    this.solutionIndex = 0;
    this.errorMessage = null;
    this.lastFailure = null;
  }
}

export const session = new Session();
