// 與 docs/CONTRACT.md 及 src/model.rs（camelCase 序列化）對應的型別。
// 前端只讀取這些資料，不重做 simai 解析或左右手指派。

export interface Point {
  x: number;
  y: number;
}

export interface SourceSpan {
  /** 原文 UTF-8 byte 起點（半開區間） */
  start: number;
  /** 原文 UTF-8 byte 終點（半開區間） */
  end: number;
  /** 1-based 行號 */
  line: number;
  /** 1-based 欄號，以 Unicode scalar 計數（不是 UTF-16 offset） */
  column: number;
}

export type Severity = 'error' | 'warning' | 'info';

export interface Diagnostic {
  code: string;
  message: string;
  severity: Severity | string;
  sourceSpan: SourceSpan | null;
  noteIds: string[];
  timeSeconds: number | null;
}

export interface PathSample {
  u: number;
  x: number;
  y: number;
}

export interface SlidePath {
  id: string;
  /** 形狀符號，連續寫法會接起來（例如 `-^`） */
  shape: string;
  startButton: number;
  endButton: number;
  samples: PathSample[];
  /** Wifi 的兩條側線；其餘形狀為空陣列 */
  branches: PathSample[][];
}

export type NoteKind = 'tap' | 'hold' | 'slide' | 'touch' | 'touchHold';

export type TouchArea = 'A' | 'B' | 'C' | 'D' | 'E';

export interface Modifiers {
  breakNote: boolean;
  ex: boolean;
  star: boolean;
  spinStar: boolean;
  fireworks: boolean;
  breakSlide: boolean;
  exSlide: boolean;
}

export interface Note {
  id: string;
  kind: NoteKind | string;
  /** 1–8 外圈鍵位或 Touch 區編號；Touch C 區為 0 */
  button: number;
  /** Touch 區代號 A–E；按鍵音符為 null */
  touchArea: TouchArea | string | null;
  timeSeconds: number;
  endSeconds: number;
  position: Point;
  pathId: string | null;
  motionStart: number | null;
  motionEnd: number | null;
  /** Slide 是否有起點觸碰（`?` `!` 與 `*` 的第二條之後為 false） */
  hasHead: boolean;
  modifiers: Modifiers;
  sourceSpan: SourceSpan;
}

/**
 * simai 可指名的 Touch 落點，由 Rust 的盤面幾何輸出。
 * A／B／D／E 各 1–8，C 的 index 為 0，共 33 個；C1／C2 在核心合併為同一個 C。
 * 這是 Demo 的可辨識落點座標，不是實機感應區輪廓。
 */
export interface TouchSensor {
  area: TouchArea | string;
  /** 1–8；C 區為 0 */
  index: number;
  position: Point;
}

export interface Chart {
  durationSeconds: number;
  notes: Note[];
  paths: SlidePath[];
  touchSensors: TouchSensor[];
}

/**
 * 評分模型版本。未傳 scoringModel 的舊 request 由 Rust 當成 legacy-v1。
 * 兩版的設定欄位互斥：V2 不得帶六個權重與 speedReference，V1 不得帶四個行為控制。
 */
export type ScoringModel = 'legacy-v1' | 'hand-affinity-v2';

/** 兩版共用的搜尋、時間與手掌設定。 */
export interface BaseSolverConfig {
  beamWidth: number;
  topK: number;
  allowHandover: boolean;
  checkpointSeconds: number;
  contactSeconds: number;
  handoverSeconds: number;
  handoverCooldown: number;
  slidePickupSeconds: number;
  glideDistance: number;
  preparationSeconds: number;
  repetitionSeconds: number;
  /**
   * 一隻手掌同時覆蓋多個 Touch 的圓形近似半徑（盤面半徑為 1）。
   * 0–1，預設 0.5；0 表示關閉手掌覆蓋，每個 Touch 都要各自接觸。
   * 覆蓋是否成立一律由 Rust 判定，前端不自行計算覆蓋組合。
   */
  palmRadius: number;
}

export interface LegacyWeights {
  speedReference: number;
  distanceWeight: number;
  speedWeight: number;
  sideWeight: number;
  crossWeight: number;
  repetitionWeight: number;
  handoverWeight: number;
}

export interface PreferenceControls {
  /** 左右分工傾向 0–100；越高越偏好各手留在本側 */
  homePreference: number;
  /** 快速移動容忍 1–200 半徑/秒；超過才開始計速度負擔，不是速度上限 */
  travelComfort: number;
  /** 同手連打容忍 0–100；越高越接受同一隻手重新擊打 */
  repeatTolerance: number;
  /** Slide 換手意願 0–100；越高交接附加費越小 */
  handoverWillingness: number;
}

/** V1：schemaVersion 2。scoringModel 可省略。 */
export interface LegacySolverConfig extends BaseSolverConfig, LegacyWeights {
  scoringModel?: 'legacy-v1';
}

/** V2：schemaVersion 3。平面物件，scoringModel 必填。 */
export interface V2SolverConfig extends BaseSolverConfig, PreferenceControls {
  scoringModel: 'hand-affinity-v2';
}

/** 實際送給 Rust、也是 configSnapshot 的形狀。 */
export type SolverConfig = LegacySolverConfig | V2SolverConfig;

/**
 * 前端參數頁的草稿：兩版欄位同時保存，切換評分方式不互相換算。
 * 不可直接送給 Rust，必須經 contract.ts 的 projectConfig() 投影。
 */
export interface ConfigDraft extends BaseSolverConfig, LegacyWeights, PreferenceControls {
  scoringModel: ScoringModel;
}

export interface AnalyzeRequest {
  requestId: string;
  source: string;
  firstSeconds: number;
  solverConfig: SolverConfig;
}

export type AnalyzeStatus = 'ok' | 'invalid' | 'unsupported' | 'no_solution' | 'search_limit';

export type Hand = 'L' | 'R';

export interface MotionSample {
  timeSeconds: number;
  x: number;
  y: number;
}

export type MotionMode =
  | 'travel'
  | 'glide'
  | 'tap'
  | 'hold'
  | 'slide'
  | 'handover'
  /** 一隻手掌同時覆蓋多個 Touch；這一段的 samples 是掌心 */
  | 'palm'
  | 'idle';

export interface MotionSegment {
  mode: MotionMode | string;
  noteId: string | null;
  startSeconds: number;
  endSeconds: number;
  samples: MotionSample[];
}

export type AssignmentPart = 'head' | 'contact' | 'slide';

export interface Assignment {
  noteId: string;
  part: AssignmentPart | string;
  hand: Hand;
  startSeconds: number;
  endSeconds: number;
}

export interface Handover {
  noteId: string;
  from: Hand;
  to: Hand;
  startSeconds: number;
  endSeconds: number;
  /** 兩手在同一點碰頭並互換目的地；沒有重疊時間，兩條 Slide 會同時各有一筆 */
  swap: boolean;
}

/**
 * 一次手掌覆蓋動作，是手掌覆蓋的權威資料。
 * 只覆蓋同一判定時間的 Touch／Touch Hold；中心與半徑都是盤面座標，時間為秒。
 * 對應該手 mode=`palm` 的動作段，期間該手不得接別處。
 */
export interface PalmPlacement {
  hand: Hand;
  /** 掌心盤面座標，由 Rust 求解產生 */
  center: Point;
  /** 覆蓋半徑（盤面座標），等於這次求解使用的 palmRadius */
  radius: number;
  startSeconds: number;
  /** 其中最晚釋放的時刻（Touch Hold 會延長） */
  endSeconds: number;
  coveredNoteIds: string[];
}

export interface CostBreakdown {
  distance: number;
  speed: number;
  side: number;
  cross: number;
  repetition: number;
  handover: number;
}

/** V2 評分；分數是本 Demo 的相對比較值，不是機率或人體能力。 */
export interface Score {
  /** 左右分工與姿態：scoreBreakdown 前四項總和 */
  intuition: number;
  /** 動作負擔：travelStrain + compressionStrain + repetition */
  strain: number;
  /** 移動距離（半徑）：只在排序值幾乎相同時用來分先後 */
  efficiency: number;
  /** intuition + 2^(4 − 8·homePreference/100) · strain */
  rankingValue: number;
}

/** 均已乘內部係數；freeDistance 單位為盤面半徑。 */
export interface ScoreBreakdown {
  assignmentAffinity: number;
  sideExposure: number;
  crossExposure: number;
  handover: number;
  travelStrain: number;
  compressionStrain: number;
  repetition: number;
  freeDistance: number;
}

interface SolutionBase {
  id: string;
  assignments: Assignment[];
  handovers: Handover[];
  palmPlacements: PalmPlacement[];
  leftSegments: MotionSegment[];
  rightSegments: MotionSegment[];
  warnings: string[];
}

export interface LegacySolution extends SolutionBase {
  scoringModel?: 'legacy-v1';
  totalCost: number;
  costBreakdown: CostBreakdown;
  configSnapshot: LegacySolverConfig;
}

export interface V2Solution extends SolutionBase {
  scoringModel: 'hand-affinity-v2';
  score: Score;
  scoreBreakdown: ScoreBreakdown;
  configSnapshot: V2SolverConfig;
}

export type Solution = LegacySolution | V2Solution;

export interface AnalyzeResponse {
  /** legacy-v1 為 2，hand-affinity-v2 為 3 */
  schemaVersion: number;
  requestId: string;
  status: AnalyzeStatus | string;
  diagnostics: Diagnostic[];
  chart: Chart | null;
  solutions: Solution[];
}

/** Majdata 搜尋結果（Rust search_majdata_charts，camelCase）。 */
export interface MajdataChartSummary {
  id: string;
  title: string;
  artist: string;
  designer: string;
  description: string;
  /** 依 &inote_1… 排列，空難度為 null 或空字串 */
  levels: (string | null)[];
  uploader: string;
  timestamp: string;
  tags: string[];
  publicTags: string[];
}
