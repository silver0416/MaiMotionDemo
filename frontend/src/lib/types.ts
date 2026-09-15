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
  samples: PathSample[];
}

export type NoteKind = 'tap' | 'hold' | 'slide';

export interface Note {
  id: string;
  kind: NoteKind | string;
  /** 1–8 外圈鍵位 */
  button: number;
  timeSeconds: number;
  endSeconds: number;
  position: Point;
  pathId: string | null;
  motionStart: number | null;
  motionEnd: number | null;
  sourceSpan: SourceSpan;
}

export interface Chart {
  durationSeconds: number;
  notes: Note[];
  paths: SlidePath[];
}

export interface SolverConfig {
  beamWidth: number;
  topK: number;
  allowHandover: boolean;
  checkpointSeconds: number;
  contactSeconds: number;
  handoverSeconds: number;
  handoverCooldown: number;
  preparationSeconds: number;
  speedReference: number;
  repetitionSeconds: number;
  distanceWeight: number;
  speedWeight: number;
  sideWeight: number;
  crossWeight: number;
  repetitionWeight: number;
  handoverWeight: number;
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

export type MotionMode = 'travel' | 'tap' | 'hold' | 'slide' | 'handover' | 'idle';

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
