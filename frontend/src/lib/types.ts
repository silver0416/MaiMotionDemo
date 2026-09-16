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

export interface SolverConfig {
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
  speedReference: number;
  repetitionSeconds: number;
  distanceWeight: number;
  speedWeight: number;
  sideWeight: number;
  crossWeight: number;
  repetitionWeight: number;
  handoverWeight: number;
  /**
   * 一隻手掌同時覆蓋多個 Touch 的圓形近似半徑（盤面半徑為 1）。
   * 0–1，預設 0.5；0 表示關閉手掌覆蓋，每個 Touch 都要各自接觸。
   * 覆蓋是否成立一律由 Rust 判定，前端不自行計算覆蓋組合。
   */
  palmRadius: number;
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

export interface Solution {
  id: string;
  totalCost: number;
  costBreakdown: CostBreakdown;
  assignments: Assignment[];
  handovers: Handover[];
  palmPlacements: PalmPlacement[];
  leftSegments: MotionSegment[];
  rightSegments: MotionSegment[];
  configSnapshot: SolverConfig;
  warnings: string[];
}

export interface AnalyzeResponse {
  schemaVersion: number;
  requestId: string;
  status: AnalyzeStatus | string;
  diagnostics: Diagnostic[];
  chart: Chart | null;
  solutions: Solution[];
}
