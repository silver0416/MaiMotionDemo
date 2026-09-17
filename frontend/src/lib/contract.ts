import type {
  AnalyzeStatus,
  BaseSolverConfig,
  ConfigDraft,
  Hand,
  LegacySolution,
  LegacySolverConfig,
  LegacyWeights,
  MotionMode,
  PreferenceControls,
  ScoreBreakdown,
  ScoringModel,
  Solution,
  SolverConfig,
  V2Solution,
  V2SolverConfig,
  V3PreferenceControls,
  V3ScoreBreakdown,
  V3Solution,
  V3SolverConfig,
} from './types';

export const SCORING_V1 = 'legacy-v1' satisfies ScoringModel;
export const SCORING_V2 = 'hand-affinity-v2' satisfies ScoringModel;
export const SCORING_V3 = 'human-motion-v3' satisfies ScoringModel;

/**
 * 產品預設評分方式：人類動作 V3。
 * 只影響第一次啟動或沒有保存值的使用者；已保存的評分方式照舊保留（見 migrateDraft）。
 */
export const DEFAULT_SCORING_MODEL: ScoringModel = SCORING_V3;

/** 各評分方式對應的 response schemaVersion。 */
export const SCHEMA_VERSION: Record<ScoringModel, number> = {
  'legacy-v1': 2,
  'hand-affinity-v2': 3,
  'human-motion-v3': 4,
};

/** 介面名稱；版本標記（v1／v2／v3）另外並列顯示。 */
export const SCORING_LABEL: Record<ScoringModel, string> = {
  'human-motion-v3': '人類動作',
  'hand-affinity-v2': '分工優先',
  'legacy-v1': '舊版比較',
};

export interface ScoringModelInfo {
  id: ScoringModel;
  /** 簡短版本標記，介面上與名稱並列。 */
  version: string;
  hint: string;
}

/**
 * 可選的評分方式，依介面顯示順序（新的在前）。
 * 新增一版時在 types.ts 的 ScoringModel、SCHEMA_VERSION、SCORING_LABEL 與這裡各加一筆，
 * 參數頁的選單會自動列出。
 */
export const SCORING_MODELS: ScoringModelInfo[] = [
  { id: SCORING_V3, version: 'v3', hint: '兼顧少移動、左右分工、折返與短時間手部負荷' },
  { id: SCORING_V2, version: 'v2', hint: '優先維持左右分工，再比較動作負擔' },
  { id: SCORING_V1, version: 'v1', hint: '六項舊成本直接加總，供比較' },
];

export function isScoringModel(value: unknown): value is ScoringModel {
  return value === SCORING_V1 || value === SCORING_V2 || value === SCORING_V3;
}

/** 與 src/model.rs 的 SolverConfig::default() 一致的共用欄位。 */
export const DEFAULT_BASE: BaseSolverConfig = {
  beamWidth: 128,
  topK: 3,
  allowHandover: true,
  checkpointSeconds: 0.05,
  contactSeconds: 0.03,
  handoverSeconds: 0.04,
  handoverCooldown: 0.2,
  slidePickupSeconds: 0.12,
  glideDistance: 0.8,
  preparationSeconds: 1,
  repetitionSeconds: 0.15,
  palmRadius: 0.5,
};

/** 與 SolverConfig::default() 的 V1 權重一致。 */
export const DEFAULT_LEGACY: LegacyWeights = {
  speedReference: 4,
  distanceWeight: 1,
  speedWeight: 1,
  sideWeight: 1,
  crossWeight: 1,
  repetitionWeight: 1,
  handoverWeight: 1,
};

/** 與 src/scoring.rs 的 PreferenceConfig::default() 一致。 */
export const DEFAULT_PREFERENCES: PreferenceControls = {
  homePreference: 80,
  travelComfort: 30,
  repeatTolerance: 60,
  handoverWillingness: 40,
};

/** 與 src/scoring_v3.rs 的 PreferenceConfigV3::default() 一致。 */
export const DEFAULT_V3_PREFERENCES: V3PreferenceControls = {
  homePreference: 60,
  travelComfort: 6.5,
  jackTolerance: 60,
  handoverWillingness: 40,
};

export const DEFAULT_DRAFT: ConfigDraft = {
  scoringModel: DEFAULT_SCORING_MODEL,
  ...DEFAULT_BASE,
  ...DEFAULT_LEGACY,
  ...DEFAULT_PREFERENCES,
  v3: { ...DEFAULT_V3_PREFERENCES },
};

export const BASE_KEYS = Object.keys(DEFAULT_BASE) as (keyof BaseSolverConfig)[];
export const LEGACY_KEYS = Object.keys(DEFAULT_LEGACY) as (keyof LegacyWeights)[];
export const PREFERENCE_KEYS = Object.keys(DEFAULT_PREFERENCES) as (keyof PreferenceControls)[];
export const V3_PREFERENCE_KEYS = Object.keys(DEFAULT_V3_PREFERENCES) as (keyof V3PreferenceControls)[];

/**
 * 依評分版本精確投影成送給 Rust 的 solverConfig：
 * V3 只帶共用欄位與 V3 四個控制（jackTolerance，不帶 repeatTolerance）；
 * V2 只帶共用欄位與 V2 四個控制；V1 只帶共用欄位、速度基準與六個權重。
 */
export function projectConfig(draft: ConfigDraft): SolverConfig {
  const base = {} as Record<string, unknown>;
  for (const key of BASE_KEYS) base[key] = draft[key];
  switch (draft.scoringModel) {
    case 'human-motion-v3':
      for (const key of V3_PREFERENCE_KEYS) base[key] = draft.v3[key];
      return { scoringModel: 'human-motion-v3', ...base } as unknown as V3SolverConfig;
    case 'hand-affinity-v2':
      for (const key of PREFERENCE_KEYS) base[key] = draft[key];
      return { scoringModel: 'hand-affinity-v2', ...base } as unknown as V2SolverConfig;
    case 'legacy-v1':
      for (const key of LEGACY_KEYS) base[key] = draft[key];
      return { scoringModel: 'legacy-v1', ...base } as unknown as LegacySolverConfig;
  }
}

function finiteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

/**
 * 把資料庫讀回的參數合併到目前草稿，逐欄檢查型別，損壞或不認得的值保留目前值。
 * 加入 V3 前保存的資料沒有 `v3`，使用 V3 預設；頂層的 V2／V1 數值與已選的評分方式照舊，
 * 各版之間不互相換算。
 */
/**
 * V3.1 以前的 V3 預設快速移動容忍。當時的負擔按秒數累積，90 半徑/秒實際上從不觸發；
 * 改為按距離計算後，保存的舊預設值換成新預設，使用者自訂的其他值保留。
 */
const LEGACY_V3_TRAVEL_COMFORT = 90;

export function migrateDraft(stored: unknown, current: ConfigDraft): ConfigDraft {
  const next = cloneDraft(current);
  if (!stored || typeof stored !== 'object') return next;
  const raw = stored as Record<string, unknown>;
  const target = next as unknown as Record<string, unknown>;
  for (const key of [...BASE_KEYS, ...LEGACY_KEYS, ...PREFERENCE_KEYS]) {
    const value = raw[key];
    const reference = target[key];
    if (typeof reference === 'number' ? finiteNumber(value) : typeof value === typeof reference) {
      target[key] = value;
    }
  }
  if (isScoringModel(raw.scoringModel)) next.scoringModel = raw.scoringModel;
  const v3 = raw.v3;
  if (v3 && typeof v3 === 'object') {
    for (const key of V3_PREFERENCE_KEYS) {
      const value = (v3 as Record<string, unknown>)[key];
      if (key === 'travelComfort' && value === LEGACY_V3_TRAVEL_COMFORT) continue;
      if (finiteNumber(value)) next.v3[key] = value;
    }
  }
  return next;
}

/**
 * 設定或快照的評分版本；沒有 scoringModel 的舊資料視為 legacy-v1。
 * 不認得的字串回傳 null，呼叫端不可把它當成任何一版。
 */
export function scoringModelOf(config: { scoringModel?: string }): ScoringModel | null {
  if (config.scoringModel === undefined) return SCORING_V1;
  return isScoringModel(config.scoringModel) ? config.scoringModel : null;
}

/** 已投影設定的評分版本；型別保證只有三種，省略時為 legacy-v1。 */
export function requestModelOf(config: SolverConfig): ScoringModel {
  return config.scoringModel ?? SCORING_V1;
}

/** 只看 scoringModel 判別；V2 與 V3 都有 score，不可用欄位存在與否判斷版本。 */
export function isV2Solution(solution: Solution): solution is V2Solution {
  return solution.scoringModel === SCORING_V2;
}

export function isV3Solution(solution: Solution): solution is V3Solution {
  return solution.scoringModel === SCORING_V3;
}

/** Rust 序列化 legacy 方案時不輸出 scoringModel。 */
export function isLegacySolution(solution: Solution): solution is LegacySolution {
  return solution.scoringModel === undefined || solution.scoringModel === SCORING_V1;
}

/** 方案實際的評分版本；不認得的 scoringModel 回傳 null。 */
export function solutionModelOf(solution: Solution): ScoringModel | null {
  if (isV3Solution(solution)) return SCORING_V3;
  if (isV2Solution(solution)) return SCORING_V2;
  if (isLegacySolution(solution)) return SCORING_V1;
  return null;
}

/** 每個欄位都必須是有限數值；NaN、Infinity 或字串代表資料損壞。 */
function numbersAt(value: unknown, keys: readonly string[]): boolean {
  if (!value || typeof value !== 'object') return false;
  return keys.every((key) => finiteNumber((value as Record<string, unknown>)[key]));
}

/** V2 scoreBreakdown 的八個欄位，自 v0.2.0 起即為固定形狀。 */
const V2_BREAKDOWN_KEYS: (keyof ScoreBreakdown)[] = [
  'assignmentAffinity',
  'sideExposure',
  'crossExposure',
  'handover',
  'travelStrain',
  'compressionStrain',
  'repetition',
  'freeDistance',
];

/** 確認分數欄位真的是該版的形狀，避免畫面讀到 undefined。 */
function hasScoreShape(solution: Solution): boolean {
  if (isV3Solution(solution)) {
    return (
      numbersAt(solution.score, ['movement', 'posture', 'fatigue', 'total']) &&
      numbersAt(solution.scoreBreakdown, V3_BREAKDOWN_KEYS)
    );
  }
  if (isV2Solution(solution)) {
    return (
      numbersAt(solution.score, ['intuition', 'strain', 'efficiency', 'rankingValue']) &&
      numbersAt(solution.scoreBreakdown, V2_BREAKDOWN_KEYS)
    );
  }
  return numbersAt(solution, ['totalCost']) && numbersAt(solution.costBreakdown, COST_KEYS);
}

/**
 * 檢查回應是否屬於請求的評分版本：schemaVersion 對得上，
 * 而且每個方案的 scoringModel 與分數欄位都是同一版。
 * 一致時回傳 null，否則回傳給使用者看的原因。新結果與快取都要經過這一關。
 */
export function responseMismatch(
  response: { schemaVersion: number; solutions: Solution[] },
  model: ScoringModel,
): string | null {
  if (response.schemaVersion !== SCHEMA_VERSION[model]) {
    return (
      `核心回傳 schemaVersion ${response.schemaVersion}，與「${SCORING_LABEL[model]}」需要的 ` +
      `${SCHEMA_VERSION[model]} 不符；核心可能尚未支援這個評分方式，結果未套用。`
    );
  }
  for (const solution of response.solutions ?? []) {
    if (solutionModelOf(solution) !== model || !hasScoreShape(solution)) {
      return `核心回傳的方案 ${solution.id} 不是「${SCORING_LABEL[model]}」的評分格式，結果未套用。`;
    }
  }
  return null;
}

/** λ = 2^(4 − 8·homePreference/100)：V2 動作負擔換算成排序值時的係數；V3 不使用。 */
export function strainFactor(homePreference: number): number {
  return 2 ** (4 - 8 * (homePreference / 100));
}

export interface ScoreItem {
  key: keyof ScoreBreakdown;
  label: string;
  hint: string;
}

export interface ScoreGroup {
  id: 'intuition' | 'strain' | 'efficiency';
  label: string;
  hint: string;
  unit: string;
  items: ScoreItem[];
}

/** V2 方案面板的三組分數與分項，依 Rust 的 scoreBreakdown 欄位。 */
export const SCORE_GROUPS: ScoreGroup[] = [
  {
    id: 'intuition',
    label: '左右分工與姿態',
    hint: '手留在本側、雙手不交叉、少交接；越低越自然。',
    unit: '',
    items: [
      { key: 'assignmentAffinity', label: '接觸分工', hint: '每次接觸落在對側的程度。' },
      { key: 'sideExposure', label: '持續停在對側', hint: 'Hold、Slide、手掌覆蓋期間停在對側的時間。' },
      { key: 'crossExposure', label: '雙手交叉', hint: '兩手同時接觸時左手在右手右邊的程度；平面代理，不是手臂碰撞。' },
      { key: 'handover', label: '交接', hint: 'Slide 中途換手的附加費；碰頭互換不收。' },
    ],
  },
  {
    id: 'strain',
    label: '動作負擔',
    hint: '超過舒適速度的移動、趕接壓縮與同手連打；越低越輕鬆。',
    unit: '',
    items: [
      { key: 'travelStrain', label: '快速移動', hint: '移動速度超過「快速移動容忍」的部分。' },
      { key: 'compressionStrain', label: 'Slide 趕接壓縮', hint: '晚接或提早掃完造成比原定更快的追蹤。' },
      { key: 'repetition', label: '同手連打', hint: '同一隻手在短時間內重新擊打。' },
    ],
  },
  {
    id: 'efficiency',
    label: '移動距離',
    hint: '手不在接觸時的移動長度；只在前兩項幾乎相同時分先後。',
    unit: '半徑',
    items: [],
  },
];

export interface V3ScoreItem {
  key: keyof V3ScoreBreakdown;
  label: string;
  hint: string;
}

export interface V3ScoreGroup {
  id: 'movement' | 'posture' | 'fatigue';
  label: string;
  /** 候選摘要用的短名稱 */
  short: string;
  hint: string;
  items: V3ScoreItem[];
}

/** V3 方案面板的三群分數與十個分項，依 Rust 的 ScoreBreakdownV3 欄位。 */
export const V3_SCORE_GROUPS: V3ScoreGroup[] = [
  {
    id: 'movement',
    label: '動作效率',
    short: '動作效率',
    hint: '移動距離、超過容忍速度的部分與 Slide 趕接；越低越省力。',
    items: [
      { key: 'travel', label: '移動距離', hint: '接觸之間的移動長度；低速移動也會計入。' },
      { key: 'speedStrain', label: '高速移動', hint: '移動速度超過「快速移動容忍」的額外負擔，依移動距離累計；極短時間的大移動最吃力。' },
      { key: 'compressionStrain', label: 'Slide 趕接', hint: '晚接或提早掃完造成比原定更快的追蹤。' },
    ],
  },
  {
    id: 'posture',
    label: '姿態與分工',
    short: '姿態',
    hint: '手離開本側、雙手交叉、Slide 中途換手與打破短期分工；越低越自然。',
    items: [
      { key: 'excursion', label: '跨區', hint: '手到另一側的程度與停留時間，由「左右分工傾向」控制。' },
      { key: 'crossExposure', label: '雙手交叉', hint: '左手位於右手右邊的程度，加上兩手同時停在對側的持續時間；短暫交叉很便宜，一直交叉才會累積。平面代理，不是手臂碰撞。' },
      { key: 'handover', label: 'Slide 換手', hint: 'Slide 中途交給另一隻手的附加費。' },
      { key: 'ownershipSwitch', label: '打破分工', hint: '同一鍵位剛由某隻手處理又改用另一隻手，或離開局部分工去接另一隻手負責的目標；隨時間衰減，同時音會減輕。' },
    ],
  },
  {
    id: 'fatigue',
    label: '短期負荷',
    short: '短期負荷',
    hint: '同點高速連打、來回折返與短時間集中在同一隻手；越低越輕鬆。',
    items: [
      { key: 'jackFatigue', label: '同點高速連打', hint: '同一隻手高速重複敲同一位置，由「同點連打容忍」控制。' },
      { key: 'reversal', label: '反覆折返', hint: '同一隻手短時間內來回改變移動方向。' },
      { key: 'workload', label: '單手集中負荷', hint: '短時間內動作集中在同一隻手；不懲罰另一隻手閒置。' },
    ],
  },
];

export const V3_BREAKDOWN_KEYS: (keyof V3ScoreBreakdown)[] = V3_SCORE_GROUPS.flatMap((group) =>
  group.items.map((item) => item.key),
);

export const WEIGHT_KEYS = [
  'distanceWeight',
  'speedWeight',
  'sideWeight',
  'crossWeight',
  'repetitionWeight',
  'handoverWeight',
] as const;

export type WeightKey = (typeof WEIGHT_KEYS)[number];

export const COST_KEYS = ['distance', 'speed', 'side', 'cross', 'repetition', 'handover'] as const;

export type CostKey = (typeof COST_KEYS)[number];

export const COST_LABEL: Record<CostKey, string> = {
  distance: '移動距離',
  speed: '移動速度負擔',
  side: '手伸到對側',
  cross: '雙手交叉',
  repetition: '同手快速連打',
  handover: '換手次數',
};

export const COST_HINT: Record<CostKey, string> = {
  distance: '兩手所有移動段的長度總和（盤面半徑為 1）。',
  speed: '移動速度相對於速度基準的平方積分，越急越高。',
  side: '左手跑到右半邊、右手跑到左半邊的姿態成本。',
  cross: '左手位置越過右手的姿態成本，只是平面代理，不是手臂碰撞偵測。',
  repetition: '同一手在很短時間內連續敲擊的負擔。',
  handover: 'Slide 中途換手的一次性費用。',
};

export const WEIGHT_OF_COST: Record<CostKey, WeightKey> = {
  distance: 'distanceWeight',
  speed: 'speedWeight',
  side: 'sideWeight',
  cross: 'crossWeight',
  repetition: 'repetitionWeight',
  handover: 'handoverWeight',
};

export const MODE_LABEL: Record<MotionMode, string> = {
  travel: '移動中',
  glide: '滑移中',
  tap: '敲擊',
  hold: '按住',
  slide: '滑行',
  handover: '交接中',
  palm: '手掌覆蓋',
  idle: '待命',
};

export const PART_LABEL: Record<string, string> = {
  head: 'Slide 起點',
  contact: '接觸',
  slide: 'Slide 軌道',
  group: 'Group 連帶判定',
};

export const KIND_LABEL: Record<string, string> = {
  tap: 'Tap',
  hold: 'Hold',
  slide: 'Slide',
  touch: 'Touch',
  touchHold: 'Touch Hold',
};

/** 形狀符號 → 一般人看得懂的名稱。 */
export const SHAPE_LABEL: Record<string, string> = {
  '-': '直線',
  '^': '圓弧',
  '<': '逆向圓弧',
  '>': '順向圓弧',
  v: '折返中心',
  V: '大 V',
  p: '左繞圈',
  q: '右繞圈',
  pp: '左大繞圈',
  qq: '右大繞圈',
  s: 'S 形',
  z: 'Z 形',
  w: 'Wifi',
};

export function shapeLabel(shape: string): string {
  if (SHAPE_LABEL[shape]) return SHAPE_LABEL[shape];
  return shape;
}

export const STATUS_LABEL: Record<AnalyzeStatus, string> = {
  ok: '分析完成',
  invalid: '語法錯誤',
  unsupported: '尚未支援的語法',
  no_solution: '模型找不到可行方案',
  search_limit: '搜尋達到上限',
};

export const STATUS_HINT: Record<AnalyzeStatus, string> = {
  ok: '',
  invalid: '下方診斷標出行列位置。',
  unsupported: '這段語法不在支援範圍，核心不會改成 Tap 帶過。',
  no_solution: '譜面合法，但這組參數找不到可行的雙手動作，因此只顯示譜面層。',
  search_limit: '已達計算預算；可調大搜尋寬度或縮短片段。',
};

export const HAND_LABEL: Record<Hand, string> = { L: '左手', R: '右手' };

export const SUPPORT_NOTES: { title: string; body: string }[] = [
  { title: '節奏', body: '(120) {4} {#0.25} , ` E ||註解' },
  { title: 'Tap', body: '1–8，同時音 / 或直接連寫；修飾 b x $ $$' },
  { title: 'Hold', body: '1h[4:1] 1h[#1.5] 1h[120#4:1]' },
  { title: 'Touch', body: 'A1–A8 B1–B8 C D1–D8 E1–E8，Touch Hold Ch[4:1]，煙火 f' },
  { title: 'Slide 形狀', body: '- ^ < > v V p q pp qq s z w' },
  { title: 'Slide 長度', body: '[4:1] [#1.5] [160#8:1] [等待##移動]' },
  { title: 'Slide 組合', body: '連續 1-3-5[4:1]、接續 1-3[4:1]-5[4:1]、同頭 *、無頭 ? !' },
  { title: '檔案', body: '可直接貼 maidata.txt，自動取難度最高的 &inote_n' },
  { title: '上限', body: '10000 個音符、3600 秒；超出計算預算會回報 search_limit' },
];

export interface ConfigIssue {
  field: string;
  message: string;
}

/**
 * 參數欄位的合法範圍，與 src/model.rs SolverConfig::validate()、
 * src/scoring.rs PreferenceConfig::validate() 及 src/scoring_v3.rs PreferenceConfigV3::validate()
 * 一致（含端點）。V2 與 V3 同名的 homePreference／travelComfort／handoverWillingness 範圍相同。
 */
export const CONFIG_RANGE = {
  beamWidth: { min: 1, max: 256 },
  topK: { min: 1, max: 5 },
  checkpointSeconds: { min: 0.02, max: 0.2 },
  contactSeconds: { min: 0.001, max: 1 },
  handoverSeconds: { min: 0.001, max: 0.2 },
  handoverCooldown: { min: 0.001, max: 10 },
  slidePickupSeconds: { min: 0, max: 2 },
  glideDistance: { min: 0, max: 2 },
  palmRadius: { min: 0, max: 1 },
  preparationSeconds: { min: 0.01, max: 10 },
  repetitionSeconds: { min: 0.001, max: 10 },
  speedReference: { min: 0.1, max: 100 },
  weight: { min: 0, max: 100 },
  homePreference: { min: 0, max: 100 },
  travelComfort: { min: 1, max: 200 },
  repeatTolerance: { min: 0, max: 100 },
  jackTolerance: { min: 0, max: 100 },
  handoverWillingness: { min: 0, max: 100 },
  firstSeconds: { min: 0, max: 120 },
} as const;

/** 錯誤清單上顯示的欄位名稱。 */
export const FIELD_LABEL: Record<string, string> = {
  beamWidth: '搜尋寬度',
  topK: '保留候選方案數',
  checkpointSeconds: '搜尋取樣間隔',
  contactSeconds: '敲擊接觸時間',
  handoverSeconds: '換手重疊時間',
  handoverCooldown: '兩次換手最短間隔',
  slidePickupSeconds: 'Slide 最晚接上時間',
  glideDistance: '相鄰連擊滑移距離',
  palmRadius: '手掌半徑',
  preparationSeconds: '開始前預備時間',
  repetitionSeconds: '同手連打判定間隔',
  speedReference: '速度基準',
  distanceWeight: '移動距離權重',
  speedWeight: '移動速度負擔權重',
  sideWeight: '手伸到對側權重',
  crossWeight: '雙手交叉權重',
  repetitionWeight: '同手快速連打權重',
  handoverWeight: '換手次數權重',
  homePreference: '左右分工傾向',
  travelComfort: '快速移動容忍',
  repeatTolerance: '同手連打容忍',
  jackTolerance: '同點連打容忍',
  handoverWillingness: 'Slide 換手意願',
  firstSeconds: '起始秒數',
};

/** 草稿中哪些欄位屬於進階區；有錯誤時自動展開。 */
export const ADVANCED_FIELDS = new Set<string>([
  'beamWidth',
  'topK',
  'checkpointSeconds',
  'contactSeconds',
  'handoverSeconds',
  'handoverCooldown',
  'slidePickupSeconds',
  'glideDistance',
  'palmRadius',
  'preparationSeconds',
  'repetitionSeconds',
  'firstSeconds',
]);

function outside(value: number, range: { min: number; max: number }): boolean {
  return !Number.isFinite(value) || value < range.min || value > range.max;
}

/**
 * 對應 Rust 的驗證，先在前端提示，實際仍由 Rust 判定。
 * 只檢查這次評分方式會送出的欄位；另一版保存在草稿裡的值不影響送出。
 */
export function validateConfig(config: ConfigDraft, firstSeconds: number): ConfigIssue[] {
  const issues: ConfigIssue[] = [];
  const R = CONFIG_RANGE;
  const positives: [string, number][] = [
    ['checkpointSeconds', config.checkpointSeconds],
    ['contactSeconds', config.contactSeconds],
    ['handoverSeconds', config.handoverSeconds],
    ['handoverCooldown', config.handoverCooldown],
    ['preparationSeconds', config.preparationSeconds],
    ['repetitionSeconds', config.repetitionSeconds],
  ];
  if (config.scoringModel === 'legacy-v1') positives.push(['speedReference', config.speedReference]);
  for (const [field, value] of positives) {
    if (!Number.isFinite(value) || value <= 0) {
      issues.push({ field, message: '時間與速度參數必須為有限正數' });
    }
  }
  if (!Number.isInteger(config.beamWidth) || config.beamWidth < 1 || config.beamWidth > 256) {
    issues.push({ field: 'beamWidth', message: '搜尋寬度必須是 1–256 的整數' });
  }
  if (!Number.isInteger(config.topK) || config.topK < 1 || config.topK > 5) {
    issues.push({ field: 'topK', message: '候選數必須是 1–5 的整數' });
  }
  if (config.topK > config.beamWidth) {
    issues.push({ field: 'topK', message: '候選數不可大於搜尋寬度' });
  }
  if (config.checkpointSeconds < 0.02 || config.checkpointSeconds > 0.2) {
    issues.push({ field: 'checkpointSeconds', message: '搜尋取樣間隔必須是 0.02–0.2 秒' });
  }
  if (config.handoverSeconds > config.checkpointSeconds) {
    issues.push({ field: 'handoverSeconds', message: '交接重疊不可超過搜尋取樣間隔' });
  }
  if (
    !Number.isFinite(config.slidePickupSeconds) ||
    config.slidePickupSeconds < 0 ||
    config.slidePickupSeconds > 2
  ) {
    issues.push({ field: 'slidePickupSeconds', message: 'Slide 最晚接上時間必須是 0–2 秒' });
  }
  if (
    !Number.isFinite(config.glideDistance) ||
    config.glideDistance < 0 ||
    config.glideDistance > 2
  ) {
    issues.push({ field: 'glideDistance', message: '滑移距離必須是 0–2' });
  }
  if (!Number.isFinite(config.palmRadius) || config.palmRadius < 0 || config.palmRadius > 1) {
    issues.push({ field: 'palmRadius', message: '手掌半徑必須是 0–1；0 表示關閉手掌覆蓋' });
  }
  if (config.preparationSeconds > 10) {
    issues.push({ field: 'preparationSeconds', message: '預備時間最多 10 秒' });
  }
  const first = (field: string) => issues.some((issue) => issue.field === field);
  if (!first('contactSeconds') && outside(config.contactSeconds, R.contactSeconds)) {
    issues.push({ field: 'contactSeconds', message: '接觸時間必須是 0.001–1 秒' });
  }
  if (!first('repetitionSeconds') && outside(config.repetitionSeconds, R.repetitionSeconds)) {
    issues.push({ field: 'repetitionSeconds', message: '連打判定間隔必須是 0.001–10 秒' });
  }
  if (!first('preparationSeconds') && outside(config.preparationSeconds, R.preparationSeconds)) {
    issues.push({ field: 'preparationSeconds', message: '預備時間必須是 0.01–10 秒' });
  }
  if (!first('handoverSeconds') && outside(config.handoverSeconds, R.handoverSeconds)) {
    issues.push({ field: 'handoverSeconds', message: '換手重疊時間必須是 0.001–0.2 秒' });
  }
  if (
    !first('handoverCooldown') &&
    (!Number.isFinite(config.handoverCooldown) ||
      config.handoverCooldown < config.handoverSeconds ||
      config.handoverCooldown > 10)
  ) {
    issues.push({ field: 'handoverCooldown', message: '換手最短間隔必須介於換手重疊時間與 10 秒之間' });
  }
  if (config.scoringModel === 'legacy-v1') {
    if (!first('speedReference') && outside(config.speedReference, R.speedReference)) {
      issues.push({ field: 'speedReference', message: '速度基準必須是 0.1–100' });
    }
    for (const key of WEIGHT_KEYS) {
      if (outside(config[key], R.weight)) {
        issues.push({ field: key, message: '成本權重必須是 0–100 的有限數值' });
      }
    }
  } else if (config.scoringModel === 'hand-affinity-v2') {
    for (const key of PREFERENCE_KEYS) {
      const range = R[key];
      if (outside(config[key], range)) {
        issues.push({ field: key, message: `必須是 ${range.min}–${range.max} 的有限數值` });
      }
    }
  } else {
    // V3 的欄位名稱與 V2 部分相同，但數值存在 config.v3；錯誤仍以 wire 欄位名稱回報。
    for (const key of V3_PREFERENCE_KEYS) {
      const range = R[key];
      if (outside(config.v3[key], range)) {
        issues.push({ field: key, message: `必須是 ${range.min}–${range.max} 的有限數值` });
      }
    }
  }
  if (outside(firstSeconds, R.firstSeconds)) {
    issues.push({ field: 'firstSeconds', message: '起始秒數必須是 0–120 秒' });
  }
  return issues;
}

/** 比較兩份已投影的設定（含 scoringModel 與欄位集合）。 */
export function configEquals(a: SolverConfig, b: SolverConfig): boolean {
  const left = a as unknown as Record<string, unknown>;
  const right = b as unknown as Record<string, unknown>;
  if (scoringModelOf(a) !== scoringModelOf(b)) return false;
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  keys.delete('scoringModel');
  for (const key of keys) {
    if (left[key] !== right[key]) return false;
  }
  return true;
}

export function cloneDraft(draft: ConfigDraft): ConfigDraft {
  return { ...draft, v3: { ...draft.v3 } };
}

/** V1 成本拆解總和（乘權重後）應等於 totalCost，容許浮點誤差；只用於 legacy 方案的自我檢查。 */
export function costSum(solution: LegacySolution): number {
  return COST_KEYS.reduce((sum, key) => sum + solution.costBreakdown[key], 0);
}
