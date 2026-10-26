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
 * 評分模型版本。產品只剩人類動作 V3；V1（legacy-v1）與 V2（hand-affinity-v2）已從介面移除，
 * 前端只送、只收 V3（schemaVersion 4）。
 */
export type ScoringModel = 'human-motion-v3';

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
  /**
   * 備用手掌半徑 0–1，預設 0.7。用 palmRadius 整首找不到方案時才改用這個半徑重試，
   * 結果的 warnings 會註明；不大於 palmRadius 或關閉手掌覆蓋（palmRadius 0）時不重試。舊快照沒有這個欄位。
   */
  palmFallbackRadius: number;
  /**
   * Hold／Touch Hold 可以提早放手（結尾 12 幀不檢查、短 Hold 只看頭判）。
   * 預設 false：一律按到結尾。舊快照沒有這個欄位，當時是可以提早放手。
   */
  holdEarlyRelease: boolean;
}

/** V3 的四個使用者控制；reversal、workload 等內部係數不開放。 */
export interface V3PreferenceControls {
  /** 左右分工傾向 0–100；直接控制跨區（excursion）成本，0 表示不收跨區成本 */
  homePreference: number;
  /** 快速移動容忍 1–200 半徑/秒（預設 5.2）；超過才依移動距離加高速負擔，低於此速度仍計移動距離 */
  travelComfort: number;
  /** 同點連打容忍 0–100；越高越接受同一隻手高速重複敲同一位置 */
  jackTolerance: number;
  /** Slide 換手意願 0–100 */
  handoverWillingness: number;
}

/** V3：schemaVersion 4。平面物件，scoringModel 必填。 */
export interface V3SolverConfig extends BaseSolverConfig, V3PreferenceControls {
  scoringModel: 'human-motion-v3';
  /**
   * Slide 依判定佇列抄近（可跳區、進最後判定區就算完成）。預設 false：手沿星星路徑畫到終點，
   * 偷懶只靠手掌範圍同時覆蓋多條 Slide 或順手點到 Tap。只有 V3 接受；舊快照沒有這個欄位。
   */
  slideShortcut?: boolean;
}

/**
 * Temoto 模型（從玩家手元影片學到的打法修正 V3）。介面上統一顯示「TemotoAI-1」，
 * 三個變體只用顏色區分（瑠璃色、茜色、常磐色），不顯示 id。
 */
export type MlModelId = 'temoto-1-tokiwa' | 'temoto-1-akane' | 'temoto-1-ruri';

/** V3 變體（與 src/scoring_v3.rs 的 V3_VARIANTS 一致）：同一套規則，只換打法權重。 */
export type V3VariantId = 'v3.1' | 'v3.2';

/** 實際送給 Rust、也是 configSnapshot 的形狀。 */
export type SolverConfig = V3SolverConfig & {
  /** 省略為純 V3；舊快照沒有這個欄位。 */
  mlModel?: MlModelId;
  /** 省略為出貨的 V3 權重；和 mlModel 不會同時出現。 */
  v3Variant?: V3VariantId;
};

/**
 * 前端參數頁的草稿。行為控制存在 `v3`（沿用既有保存資料的位置）。
 * 不可直接送給 Rust，必須經 contract.ts 的 projectConfig() 投影。
 */
export interface ConfigDraft extends BaseSolverConfig {
  v3: V3PreferenceControls;
  slideShortcut: boolean;
  /** null 為純 V3。選了 Temoto 時「打法偏好」的四個控制不影響結果，介面應隱藏。 */
  mlModel: MlModelId | null;
  /** null 為出貨的 V3 權重；選了 Temoto 模型時一律為 null（模型自帶基座）。 */
  v3Variant: V3VariantId | null;
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

/** V3 沒有 totalCost／costBreakdown。 */
export interface V3Solution extends SolutionBase {
  scoringModel: 'human-motion-v3';
  score: V3Score;
  scoreBreakdown: V3ScoreBreakdown;
  configSnapshot: V3SolverConfig;
}

export type Solution = V3Solution;

export interface AnalyzeResponse {
  /** human-motion-v3 為 4 */
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

/** adxdls 目錄裡單一難度的定數（`12.8` 等字串）與譜師。 */
export interface AdxDifficultyInfo {
  level: string | null;
  designer: string | null;
}

/** adxdls（AstroDX 譜面目錄）的一首歌（Rust chart_search，camelCase）。Standard 與 DX 是兩筆。 */
export interface AdxSong {
  slug: string;
  title: string;
  titleRomaji: string | null;
  artist: string;
  artistRomaji: string | null;
  aliases: string[];
  chartType: WikiChartType;
  bpm: number | null;
  version: string | null;
  genre: string | null;
  /** key 與 WikiSong 相同（basic…reMaster）。 */
  difficulties: Record<string, AdxDifficultyInfo>;
  pageUrl: string;
}

/** adx_refresh_index 的結果：目錄留在 Rust，前端只拿筆數與時間。 */
export interface AdxIndexPayload {
  count: number;
  /** unix 秒 */
  fetchedAt: number;
  fromCache: boolean;
  /** adxdls 抓取失敗，回傳的是舊快取。 */
  stale: boolean;
}

/** adx_fetch_chart 的結果；chartText 只含選定難度，已通過核心 parser。 */
export interface AdxChartPayload {
  chartText: string;
  title: string;
  artist: string | null;
  bpm: number | null;
  level: string | null;
  designer: string | null;
  pageUrl: string;
  slug: string;
  difficulty: string;
  chartType: WikiChartType;
}

/** 合併搜尋的一首歌：同種類、同歌名的 Wiki 與 adxdls 合成一筆，至少一邊有值。 */
export interface ChartSearchHit {
  title: string;
  chartType: WikiChartType;
  wiki: WikiSong | null;
  adx: AdxSong | null;
}

export interface ChartSearchPayload {
  items: ChartSearchHit[];
  wikiReady: boolean;
  adxReady: boolean;
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
  /** 可以在程式內下載的執行檔；沒有就只能到 GitHub 手動下載 */
  asset: UpdateAsset | null;
}

export interface UpdateAsset {
  name: string;
  url: string;
  size: number;
  sha256: string | null;
}

export interface DownloadedUpdate {
  path: string;
  name: string;
  /** 程式所在資料夾不能寫入，改放在「下載」資料夾 */
  fallback: boolean;
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
  /** 第一步（接觸或 Slide 起點；沒有起點的 Slide 則是滑行）的信心，省略為確定 */
  confidence?: Confidence;
  /** 有起點的 Slide 滑行那一步的信心；省略時沿用 confidence */
  trackConfidence?: Confidence;
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

/**
 * 分組：玩家看譜時覺得「這幾顆是一組」的音符，以顆為單位。
 * 用來研究人類怎麼拆解譜面；目前模型不讀。
 */
export interface NoteGroup {
  /** 這組的音符定位鍵（依時間排序） */
  keys: string[];
  /**
   * v0.4.11 以時間範圍標的分組：keys 為空，範圍內（含兩端）的音符就是這一組；
   * 改動或匯出時換成 keys。
   */
  from?: number;
  to?: number;
  /** 選填名稱，例如「交替」「階梯」 */
  label?: string;
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
  /** 主線的手順 */
  notes: NoteAnnotation[];
  ranges: RangeMemo[];
  /** 分組（沒有時省略） */
  groups?: NoteGroup[];
  /** 對照用的 YouTube 影片與同步偏移（影片時間 = 譜面時間 + offset） */
  video?: AnnotationVideo;
  /** 其他打法：某一段換成不同手順的分支（只有前端讀；Rust 比對時只收到展開後的單一路線） */
  branches?: AnnotationBranchFile[];
}

/** 打法分支：from～to（含）這段的音符改用自己的手順，結束後併回 parent。 */
export interface AnnotationBranchFile {
  id: string;
  name: string;
  /** 從哪條線分出來：'main' 或另一條分支的 id */
  parent: string;
  /** 結束後併入哪條線；省略表示併回 parent */
  merge?: string;
  /** 這段第一顆與最後一顆音符的判定時間（秒） */
  from: number;
  to: number;
  memo?: string;
  by?: string;
  /** 分支圖上的線色（#rrggbb） */
  color?: string;
  notes: NoteAnnotation[];
}

export interface EvaluateRequest {
  requestId: string;
  source: string;
  firstSeconds: number;
  solverConfig: SolverConfig;
  annotation: HandAnnotation;
  /** 同一份原文、同一組設定的現有分析第一名；有的話 Rust 直接拿來比，不再重算模型。 */
  modelSolution?: Solution;
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
