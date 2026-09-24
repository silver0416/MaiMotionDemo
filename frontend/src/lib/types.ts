import type { AnnotationVideo } from './video';
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
  /** 引導星星進入最後判定區時的弧長比例（Slide 尾判正解位置）；舊快取可能沒有 */
  judgeProgress?: number;
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
  /** 跨版本穩定的定位鍵 `時間|種類|位置`（Rust 產生）；真人標註用它對應音符。舊快取沒有。 */
  key?: string;
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
 * 各版設定欄位互斥：V1 只帶六個權重與 speedReference；V2 帶 repeatTolerance、
 * V3 帶 jackTolerance，兩者都不得帶舊版權重，也不得帶對方的連打欄位。
 */
export type ScoringModel = 'legacy-v1' | 'hand-affinity-v2' | 'human-motion-v3';

/** 各版共用的搜尋、時間與手掌設定。 */
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

/** V2 的四個行為控制。 */
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

/** V3 的四個使用者控制；reversal、workload 等內部係數不開放。 */
export interface V3PreferenceControls {
  /** 左右分工傾向 0–100；直接控制跨區（excursion）成本，0 表示不收跨區成本 */
  homePreference: number;
  /** 快速移動容忍 1–200 半徑/秒（預設 6.5）；超過才依移動距離加高速負擔，低於此速度仍計移動距離 */
  travelComfort: number;
  /** 同點連打容忍 0–100；越高越接受同一隻手高速重複敲同一位置 */
  jackTolerance: number;
  /** Slide 換手意願 0–100 */
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

/** V3：schemaVersion 4。平面物件，scoringModel 必填。 */
export interface V3SolverConfig extends BaseSolverConfig, V3PreferenceControls {
  scoringModel: 'human-motion-v3';
}

/** 實際送給 Rust、也是 configSnapshot 的形狀。 */
export type SolverConfig = LegacySolverConfig | V2SolverConfig | V3SolverConfig;

/**
 * 前端參數頁的草稿：各版欄位同時保存，切換評分方式不互相換算。
 * 頂層的行為控制屬於 V2（沿用既有保存資料）；V3 同名欄位預設值不同，另存在 `v3`。
 * 不可直接送給 Rust，必須經 contract.ts 的 projectConfig() 投影。
 */
export interface ConfigDraft extends BaseSolverConfig, LegacyWeights, PreferenceControls {
  scoringModel: ScoringModel;
  v3: V3PreferenceControls;
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

/** group：Touch Group 過半判定連帶完成，沒有實際接觸 */
export type AssignmentPart = 'head' | 'contact' | 'slide' | 'group';

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

/** V3 三大群；total = movement + posture + fatigue，直接加總、越低越偏好。 */
export interface V3Score {
  movement: number;
  posture: number;
  fatigue: number;
  total: number;
}

/** V3 十個分項，依三大群排列。 */
export interface V3ScoreBreakdown {
  travel: number;
  speedStrain: number;
  compressionStrain: number;

  excursion: number;
  crossExposure: number;
  handover: number;
  /** V3.1：短時間內換掉鍵位原本的手，或離開較緊的局部分工。 */
  ownershipSwitch: number;

  jackFatigue: number;
  reversal: number;
  workload: number;
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

/** V3 沒有 totalCost／costBreakdown。 */
export interface V3Solution extends SolutionBase {
  scoringModel: 'human-motion-v3';
  score: V3Score;
  scoreBreakdown: V3ScoreBreakdown;
  configSnapshot: V3SolverConfig;
}

export type Solution = LegacySolution | V2Solution | V3Solution;

export interface AnalyzeResponse {
  /** legacy-v1 為 2，hand-affinity-v2 為 3，human-motion-v3 為 4 */
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

/** simai Wiki 的譜面種類；顯示為 Standard／DX。 */
export type WikiChartType = 'standard' | 'deluxe';

/** Wiki 索引裡單一難度的提示；availabilityHint 只是索引頁的推測，實際以 wiki_fetch_chart 為準。 */
export interface WikiDifficultyInfo {
  level: string | null;
  availabilityHint: boolean;
  anchorUrl: string | null;
}

/** simai Wiki 索引的一首歌（Rust wiki_refresh_index／wiki_search_songs，camelCase）。Standard 與 DX 是兩筆。 */
export interface WikiSong {
  pageId: number;
  title: string;
  chartType: WikiChartType;
  pageUrl: string;
  /** key 為 easy／basic／advanced／expert／master／reMaster；DX 沒有 easy。 */
  difficulties: Record<string, WikiDifficultyInfo>;
  section: string | null;
}

export interface WikiIndexPayload {
  songs: WikiSong[];
  /** unix 秒 */
  fetchedAt: number;
  fromCache: boolean;
  /** Wiki 抓取失敗，回傳的是舊快取。 */
  stale: boolean;
}

/** wiki_fetch_chart 的結果；chartText 已通過核心 parser，可直接送 analyze_chart。 */
export interface WikiChartPayload {
  chartText: string;
  title: string;
  artist: string | null;
  bpm: number | null;
  level: string | null;
  pageUrl: string;
  difficulty: string;
  chartType: string;
}

/**
 * 建置發佈通路（Rust app_distribution，camelCase 前後一致都用小寫字串）。
 * - portable：免安裝單一 exe，只能導向 GitHub 下載
 * - installed：未來的安裝版，保留給 updater 全自動更新用
 * - dev：debug 建置；preview：瀏覽器預覽（沒有 Rust 核心）
 */
export type AppDistribution = 'portable' | 'installed' | 'dev' | 'preview';

/** GitHub Releases 檢查結果（Rust check_update，camelCase）。 */
export interface UpdateInfo {
  current: string;
  latest: string;
  hasUpdate: boolean;
  url: string;
  notes: string;
  publishedAt: string;
  distribution: string;
}


// ---- 真人手順標註（maimotion-hand-annotation 第 1 版，見 docs/CONTRACT.md） ----

export const ANNOTATION_FORMAT = 'maimotion-hand-annotation';
export const ANNOTATION_VERSION = 1;

/** Slide 滑行的手；LR 為兩手一起（WiFi 2+1）。 */
export type TrackHand = Hand | 'LR';

/** sure：拿來比對與限制求解；unsure：只記錄；either：兩手皆可。 */
export type Confidence = 'sure' | 'unsure' | 'either';

export interface HandoverMark {
  /** 換手時刻（秒） */
  at: number;
  to: Hand;
}

export interface NoteAnnotation {
  key: string;
  /** 接觸的手：Tap／Hold／Touch，或 Slide 起點觸碰 */
  hand?: Hand;
  /** Slide 開始滑行時的手 */
  track?: TrackHand;
  handovers?: HandoverMark[];
  confidence?: Confidence;
  /** 由模型預填、尚未確認；不當作真人資料 */
  prefilled?: boolean;
  memo?: string;
  /** 最後編輯的標註者 */
  by?: string;
}

export interface RangeMemo {
  from: number;
  to: number;
  memo: string;
  by?: string;
}

export interface HandAnnotation {
  format: typeof ANNOTATION_FORMAT;
  version: typeof ANNOTATION_VERSION;
  title: string;
  annotators: string[];
  updatedAt: string;
  chart: {
    /** 原文 SHA-256（十六進位） */
    sha256: string;
    firstSeconds: number;
    noteCount: number;
    source: string;
  };
  memo: string;
  notes: NoteAnnotation[];
  ranges: RangeMemo[];
  /** 對照用的 YouTube 影片與同步偏移（影片時間 = 譜面時間 + offset） */
  video?: AnnotationVideo;
}

export interface EvaluateRequest {
  requestId: string;
  source: string;
  firstSeconds: number;
  solverConfig: SolverConfig;
  annotation: HandAnnotation;
}

export interface EvaluatedSolution {
  /** V2／V3 為分數 total，Legacy 為總成本；越低越好 */
  cost: number;
  score?: unknown;
  scoringModel?: string | null;
}

export interface Divergence {
  noteId: string;
  key: string;
  timeSeconds: number;
  /** hand：接觸；track：Slide 開始滑行 */
  part: 'hand' | 'track' | string;
  human: TrackHand;
  model: TrackHand;
}

export interface EvaluateResponse {
  requestId: string;
  /** ok／human_infeasible（模型照標註走不下去）／invalid 等 */
  status: string;
  diagnostics: Diagnostic[];
  labeled: number;
  matched: number;
  unmatchedKeys: string[];
  compared: number;
  agreed: number;
  divergences: Divergence[];
  model: EvaluatedSolution | null;
  human: EvaluatedSolution | null;
  humanSolution: Solution | null;
}
