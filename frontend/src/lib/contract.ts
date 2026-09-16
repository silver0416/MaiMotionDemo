import type {
  AnalyzeResponse,
  AnalyzeStatus,
  Hand,
  MotionMode,
  Solution,
  SolverConfig,
} from './types';

/** 與 src/model.rs 的 SolverConfig::default() 一致。 */
export const DEFAULT_CONFIG: SolverConfig = {
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
  speedReference: 4,
  repetitionSeconds: 0.15,
  distanceWeight: 1,
  speedWeight: 1,
  sideWeight: 1,
  crossWeight: 1,
  repetitionWeight: 1,
  handoverWeight: 1,
  palmRadius: 0.5,
};

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

/** 對應 src/model.rs 的 SolverConfig::validate()，先在前端提示，實際仍由 Rust 判定。 */
export function validateConfig(config: SolverConfig, firstSeconds: number): ConfigIssue[] {
  const issues: ConfigIssue[] = [];
  const positives: [string, number][] = [
    ['checkpointSeconds', config.checkpointSeconds],
    ['contactSeconds', config.contactSeconds],
    ['handoverSeconds', config.handoverSeconds],
    ['handoverCooldown', config.handoverCooldown],
    ['preparationSeconds', config.preparationSeconds],
    ['speedReference', config.speedReference],
    ['repetitionSeconds', config.repetitionSeconds],
  ];
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
  for (const key of WEIGHT_KEYS) {
    const value = config[key];
    if (!Number.isFinite(value) || value < 0 || value > 100) {
      issues.push({ field: key, message: '成本權重必須是 0–100 的有限數值' });
    }
  }
  if (!Number.isFinite(firstSeconds) || firstSeconds < 0 || firstSeconds > 120) {
    issues.push({ field: 'firstSeconds', message: '起始秒數必須是 0–120 秒' });
  }
  return issues;
}

export function configEquals(a: SolverConfig, b: SolverConfig): boolean {
  return (Object.keys(DEFAULT_CONFIG) as (keyof SolverConfig)[]).every((key) => a[key] === b[key]);
}

export function cloneConfig(config: SolverConfig): SolverConfig {
  return { ...config };
}

/** 成本拆解總和（乘權重後）應等於 totalCost，容許浮點誤差；用於面板上的自我檢查提示。 */
export function costSum(solution: Solution): number {
  return COST_KEYS.reduce((sum, key) => sum + solution.costBreakdown[key], 0);
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/** fixtures 以 unknown 匯入，使用前做最低限度的形狀檢查。 */
export function asAnalyzeResponse(value: unknown, label: string): AnalyzeResponse {
  if (
    !isObject(value) ||
    typeof value.status !== 'string' ||
    !Array.isArray(value.diagnostics) ||
    !Array.isArray(value.solutions)
  ) {
    throw new Error(`範例資料格式不符：${label}`);
  }
  return value as unknown as AnalyzeResponse;
}
