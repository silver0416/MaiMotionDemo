import { BUILD_ID, BUILD_TIME, FRONTEND_VERSION } from './build';
import {
  SCHEMA_VERSION,
  SCORING_LABEL,
  SCORING_V3,
  STATUS_LABEL,
  TEMOTO_MODELS,
  v3Label,
} from './contract';
import { formatClock } from './format';
import { byteOffsetToIndex } from './text';
import type { AnalyzeResponse, AnalyzeStatus, ConfigDraft, Diagnostic, Note, Solution, SolverConfig } from './types';

export interface DebugInput {
  /** 在哪裡發生，例如「方案分頁」「新增譜面」「Majdata 匯入」。 */
  context: string;
  source: string;
  appVersion: string;
  config?: SolverConfig | null;
  firstSeconds?: number | null;
  requestId?: string | null;
  response?: AnalyzeResponse | null;
  /** IPC 或下載失敗的訊息（沒有 response 時）。 */
  error?: string | null;
  /** 額外資訊，例如 Majdata song id。 */
  extra?: Record<string, string>;
}

const CONTEXT_LINES = 2;

/** 把診斷對應回原文：附前後幾行，並用 ^ 標出欄位範圍。 */
function excerpt(source: string, diagnostic: Diagnostic): string | null {
  const span = diagnostic.sourceSpan;
  if (!span) return null;
  const lines = source.split('\n');
  const lineIndex = span.line - 1;
  if (lineIndex < 0 || lineIndex >= lines.length) return null;
  const from = Math.max(0, lineIndex - CONTEXT_LINES);
  const to = Math.min(lines.length - 1, lineIndex + CONTEXT_LINES);
  const width = String(to + 1).length;
  const out: string[] = [];
  // 範圍長度以字元計；跨行時只標到這一行結尾。
  const startIndex = byteOffsetToIndex(source, span.start);
  const endIndex = byteOffsetToIndex(source, span.end);
  const lineText = lines[lineIndex].replace(/\r$/, '');
  const column = Math.max(1, span.column);
  const length = Math.max(1, Math.min([...source.slice(startIndex, endIndex)].length, [...lineText].length - column + 1));
  for (let index = from; index <= to; index += 1) {
    const text = lines[index].replace(/\r$/, '');
    const marker = index === lineIndex ? '>' : ' ';
    out.push(`${marker} ${String(index + 1).padStart(width)} | ${text}`);
    if (index === lineIndex) {
      const pad = [...text].slice(0, column - 1).map((char) => (char === '\t' ? '\t' : ' ')).join('');
      out.push(`  ${' '.repeat(width)} | ${pad}${'^'.repeat(length)}`);
    }
  }
  return out.join('\n');
}

function noteLine(note: Note): string {
  const where = note.touchArea ? `${note.touchArea}${note.button || ''}` : `鍵 ${note.button}`;
  const span = note.sourceSpan ? `第 ${note.sourceSpan.line} 行第 ${note.sourceSpan.column} 欄` : '無位置';
  const duration =
    note.endSeconds > note.timeSeconds ? ` ～ ${formatClock(note.endSeconds)}` : '';
  return `  - ${note.id}（${note.kind}，${where}）${formatClock(note.timeSeconds)}${duration}・${span}`;
}

/** 方案的分數欄位。 */
function scoreEntry(solution: Solution): Record<string, unknown> {
  return { id: solution.id, scoringModel: solution.scoringModel, score: solution.score, scoreBreakdown: solution.scoreBreakdown };
}

function fence(text: string, lang = ''): string {
  // 原文裡若有 ``` 就加長圍欄，避免貼到 Markdown 時被截斷。
  let ticks = '```';
  while (text.includes(ticks)) ticks += '`';
  return `${ticks}${lang}\n${text}\n${ticks}`;
}

export function buildDebugReport(input: DebugInput): string {
  const { response } = input;
  const status = response?.status as AnalyzeStatus | undefined;
  const parts: string[] = [];

  parts.push('# MaiMotionDemo 除錯資訊');
  const meta = [
    `- 產生時間：${new Date().toISOString()}`,
    `- 版本：${input.appVersion}`,
    `- 位置：${input.context}`,
  ];
  if (status) meta.push(`- 狀態：${status}（${STATUS_LABEL[status] ?? '未知狀態'}）`);
  if (input.config) {
    const model = (input.config as { scoringModel?: string }).scoringModel ?? 'legacy-v1';
    meta.push(`- 評分方式：${SCORING_LABEL}（${model}，預期 schemaVersion ${SCHEMA_VERSION}）`);
  }
  if (response) meta.push(`- schemaVersion：${response.schemaVersion}`);
  const requestId = response?.requestId ?? input.requestId;
  if (requestId) meta.push(`- requestId：${requestId}`);
  if (input.firstSeconds !== undefined && input.firstSeconds !== null) {
    meta.push(`- 起始秒數 firstSeconds：${input.firstSeconds}`);
  }
  if (response?.chart) {
    meta.push(
      `- 譜面：${response.chart.notes.length} 個音符，長度 ${formatClock(response.chart.durationSeconds)}`,
    );
    meta.push(`- 候選方案數：${response.solutions.length}`);
  }
  for (const [key, value] of Object.entries(input.extra ?? {})) meta.push(`- ${key}：${value}`);
  meta.push(`- 原文長度：${input.source.length} 字元、${input.source.split('\n').length} 行`);
  parts.push(meta.join('\n'));

  if (input.error) {
    parts.push(`## 錯誤訊息\n\n${fence(input.error)}`);
  }

  const diagnostics = response?.diagnostics ?? [];
  if (diagnostics.length > 0) {
    const noteById = new Map<string, Note>();
    for (const note of response?.chart?.notes ?? []) noteById.set(note.id, note);
    const items = diagnostics.map((diagnostic, index) => {
      const lines = [`### ${index + 1}. [${diagnostic.severity}] ${diagnostic.code}`, '', diagnostic.message, ''];
      if (diagnostic.sourceSpan) {
        const span = diagnostic.sourceSpan;
        lines.push(`- 原文位置：第 ${span.line} 行、第 ${span.column} 欄（UTF-8 byte ${span.start}–${span.end}）`);
      } else {
        lines.push('- 原文位置：核心未提供');
      }
      if (diagnostic.timeSeconds !== null) {
        lines.push(`- 譜面時間：${formatClock(diagnostic.timeSeconds)}（${diagnostic.timeSeconds} 秒）`);
      }
      if (diagnostic.noteIds.length > 0) {
        lines.push('- 相關音符：');
        for (const noteId of diagnostic.noteIds) {
          const note = noteById.get(noteId);
          lines.push(note ? noteLine(note) : `  - ${noteId}（譜面資料中找不到）`);
          if (note && !diagnostic.sourceSpan) {
            const noteExcerpt = excerpt(input.source, { ...diagnostic, sourceSpan: note.sourceSpan });
            if (noteExcerpt) lines.push('', fence(noteExcerpt), '');
          }
        }
      }
      const text = excerpt(input.source, diagnostic);
      if (text) lines.push('', fence(text));
      return lines.join('\n');
    });
    parts.push(`## 問題點（${diagnostics.length} 項）\n\n${items.join('\n\n')}`);
  } else if (status && status !== 'ok') {
    parts.push('## 問題點\n\n核心沒有回傳任何診斷。');
  }

  if (input.config) {
    parts.push(`## 參數 solverConfig\n\n${fence(JSON.stringify(input.config, null, 2), 'json')}`);
  }

  const scores = (response?.solutions ?? []).map(scoreEntry);
  if (scores.length > 0) {
    // 只輸出評分與分項，不含動作段；V3 需要完整十項才看得出為什麼選這個打法。
    parts.push(`## 候選評分（依核心排序）\n\n${fence(JSON.stringify(scores, null, 2), 'json')}`);
  }

  if (diagnostics.length > 0) {
    parts.push(`## 原始診斷 JSON\n\n${fence(JSON.stringify(diagnostics, null, 2), 'json')}`);
  }

  parts.push(`## 完整譜面\n\n${fence(input.source)}`);
  return `${parts.join('\n\n')}\n`;
}

export interface VersionInfoInput {
  /** 桌面版的 App 版本（api.ts appVersion）；瀏覽器預覽改寫前端版本。 */
  appVersion: string;
  desktop: boolean;
  /** 應用程式種類的顯示名稱（update.svelte.ts DISTRIBUTION_LABEL）。 */
  distribution: string;
  /** 判定規則修訂（session.svelte.ts SOLVER_REVISION）。 */
  solverRevision: string;
  config: Pick<ConfigDraft, 'mlModel' | 'v3Variant' | 'stairFlow'>;
}

/**
 * 版本資訊，每項一行（不含項目符號）：設定 →「版本與更新」的「複製」與比對回報共用。
 * 畫面上只顯示系列名稱與顏色，這裡才帶模型 id。
 */
export function versionInfoLines(input: VersionInfoInput): string[] {
  const temoto = TEMOTO_MODELS.find((model) => model.id === input.config.mlModel) ?? null;
  // 基座：TemotoAI 用模型附帶的基座，沒選模型時是參數頁選的基座。
  const baseName = v3Label(temoto ? temoto.base : input.config.v3Variant);
  const model = temoto ? `${temoto.label}（${temoto.id}，基於 ${baseName}）` : baseName;
  return [
    `App 版本：${input.desktop ? input.appVersion : `${FRONTEND_VERSION}（瀏覽器預覽）`}`,
    `應用程式種類：${input.distribution}`,
    `Build：${BUILD_ID}（${BUILD_TIME}）`,
    `判定規則修訂：${input.solverRevision}`,
    `模型：${model}`,
    `快速階梯一隻手刷完：${input.config.stairFlow ? '開' : '關'}`,
    `評分方式：${SCORING_LABEL}（${SCORING_V3}，schema ${SCHEMA_VERSION}）`,
  ];
}

/** 比對頁三種「走不下去」提示的標題：畫面與比對回報共用，兩邊字一樣。 */
export const COMPARE_ALERT_TITLE = {
  modelSkipped: (count: number) => `有 ${count} 處模型走不下去，那幾顆沒有比對`,
  skipped: (count: number) => `有 ${count} 處照標註走不下去，已先略過`,
  infeasible: '模型照標註走不下去',
} as const;

/** 比對頁清單上的一處（照標註略過或模型略過）：文字照畫面上的清單。 */
export interface CompareSpot {
  timeSeconds: number | null;
  noteIds: string[];
  /** 音符說明，同畫面清單。 */
  what: string;
  /** 標記者標的手，同畫面清單；沒有時為空字串。 */
  hands: string;
}

export interface CompareReportInput {
  versionLines: string[];
  /** 譜面標題（標註草稿的 title）。 */
  title: string;
  requestId: string;
  /** 比對回傳的 status（ok／human_infeasible…）。 */
  status: string;
  /** 比對結果區上方那一行狀態（暫存、標註之後改過、正在比對）；沒有時為空字串。 */
  statusLine: string;
  /** 「開始比對」失敗的訊息；沒有時為 null。 */
  error: string | null;
  firstSeconds: number;
  /** 是否有打法分支。 */
  hasBranches: boolean;
  /** 目前路線走到的線，例如「主線」「打法 B＋打法 C」。 */
  currentRoute: string;
  /** 正在編輯的那條線。 */
  activeLine: string;
  /** 比對採用的打法組合（有分支時）。 */
  chosenRoute: string;
  routeScores: { name: string; agreed: number; compared: number; chosen: boolean }[];
  /** 畫面上實際出現的提示標題（依畫面順序）。 */
  alerts: string[];
  infeasible: Diagnostic | null;
  skipped: CompareSpot[];
  modelSkipped: CompareSpot[];
  summary: {
    labeled: number;
    matched: number;
    unmatchedKeys: string[];
    agreed: number;
    compared: number;
    modelCost: number | null;
    humanCost: number | null;
    divergences: number;
  };
  config: SolverConfig;
  diagnostics: Diagnostic[];
  /** 用來把音符 id 對回種類、時間與原文位置。 */
  notes: Note[];
  source: string;
  /** 「分享 → 複製標註」同一份標註檔 JSON。 */
  annotationJson: string;
}

export const ANNOTATION_BEGIN = '<<<<< 標註檔開始 >>>>>';
export const ANNOTATION_END = '<<<<< 標註檔結束 >>>>>';

function percent(agreed: number, compared: number): string {
  return compared > 0 ? `${agreed}／${compared}（${Math.round((1000 * agreed) / compared) / 10}%）` : '—';
}

function clockWithSeconds(seconds: number | null): string {
  return seconds === null ? '無時間' : `${formatClock(seconds)}（${seconds} 秒）`;
}

function spotLines(spots: CompareSpot[]): string {
  return spots
    .map((spot) => {
      const ids = spot.noteIds.length > 0 ? `　[${spot.noteIds.join('、')}]` : '';
      return `- ${clockWithSeconds(spot.timeSeconds)}　${spot.what}　標記 ${spot.hands || '—'}${ids}`;
    })
    .join('\n');
}

/**
 * 比對頁的「複製除錯資訊」：版本、比對狀態與畫面上的提示、參數、原始診斷，最後是完整標註檔。
 * 開發者拿到這一份就能重現，不用再跟標註者要東西。段落寫法比照 buildDebugReport。
 */
export function buildCompareReport(input: CompareReportInput): string {
  const parts: string[] = [];
  const noteById = new Map(input.notes.map((note) => [note.id, note]));

  parts.push('# MaiMotionDemo 比對回報');
  const meta = [
    `- 產生時間：${new Date().toISOString()}`,
    ...input.versionLines.map((line) => `- ${line}`),
    '- 位置：標註 → 比對',
    `- 譜面：${input.title || '（未命名）'}`,
    `- 比對狀態：${input.status}`,
    `- requestId：${input.requestId}`,
    `- 起始秒數 firstSeconds：${input.firstSeconds}`,
  ];
  if (input.hasBranches) {
    meta.push(`- 目前路線：${input.currentRoute}`);
    meta.push(`- 正在編輯的線：${input.activeLine}`);
    meta.push(`- 比對採用的打法：${input.chosenRoute}`);
  } else {
    meta.push('- 打法分支：沒有（全部在主線）');
  }
  if (input.statusLine) meta.push(`- 畫面狀態：${input.statusLine}`);
  parts.push(meta.join('\n'));

  if (input.error) parts.push(`## 錯誤訊息\n\n${fence(input.error)}`);

  if (input.alerts.length > 0) {
    const lines = input.alerts.map((title) => `- ${title}`);
    parts.push(`## 畫面上的提示（${input.alerts.length} 項）\n\n${lines.join('\n')}`);
  }

  if (input.infeasible) {
    const item = input.infeasible;
    const lines = [`## ${COMPARE_ALERT_TITLE.infeasible}`, '', item.message, '', `- 譜面時間：${clockWithSeconds(item.timeSeconds)}`];
    if (item.sourceSpan) lines.push(`- 原文位置：第 ${item.sourceSpan.line} 行、第 ${item.sourceSpan.column} 欄`);
    if (item.noteIds.length > 0) {
      lines.push('- 相關音符：');
      for (const noteId of item.noteIds) {
        const note = noteById.get(noteId);
        lines.push(note ? noteLine(note) : `  - ${noteId}（譜面資料中找不到）`);
      }
      const first = noteById.get(item.noteIds[0]);
      const text = excerpt(input.source, { ...item, sourceSpan: item.sourceSpan ?? first?.sourceSpan ?? null });
      if (text) lines.push('', fence(text));
    }
    parts.push(lines.join('\n'));
  }

  const { summary } = input;
  const summaryLines = [
    `- 已確認的標註：${summary.labeled} 顆`,
    `- 參與比對：${summary.matched} 顆${summary.unmatchedKeys.length > 0 ? `（${summary.unmatchedKeys.length} 顆對不上音符）` : ''}`,
    `- 吻合：${percent(summary.agreed, summary.compared)}`,
    `- 模型分數：${summary.modelCost === null ? '—' : summary.modelCost.toFixed(3)}`,
    `- 照標註：${summary.humanCost === null ? '做不到' : summary.humanCost.toFixed(3)}${input.skipped.length > 0 && summary.humanCost !== null ? `（略過 ${input.skipped.length} 處）` : ''}`,
  ];
  if (summary.modelCost !== null && summary.humanCost !== null) {
    const gap = summary.humanCost - summary.modelCost;
    summaryLines.push(`- 差距：${gap >= 0 ? '+' : ''}${gap.toFixed(3)}`);
  }
  summaryLines.push(`- 與模型不同：${summary.divergences} 處`);
  if (summary.unmatchedKeys.length > 0) {
    summaryLines.push('- 對不上音符的標記：', ...summary.unmatchedKeys.map((key) => `  - ${key}`));
  }
  parts.push(`## 比對摘要\n\n${summaryLines.join('\n')}`);

  if (input.routeScores.length > 1) {
    const lines = input.routeScores.map(
      (item) => `- ${item.name}${item.chosen ? '（採用）' : ''}：${percent(item.agreed, item.compared)}`,
    );
    parts.push(`## 各種打法的吻合\n\n${lines.join('\n')}`);
  }

  if (input.skipped.length > 0) {
    parts.push(`## 照標註略過的地方（${input.skipped.length}）\n\n${spotLines(input.skipped)}`);
  }
  if (input.modelSkipped.length > 0) {
    parts.push(`## 模型略過的地方（${input.modelSkipped.length}）\n\n${spotLines(input.modelSkipped)}`);
  }

  parts.push(`## 參數 solverConfig\n\n${fence(JSON.stringify(input.config, null, 2), 'json')}`);

  if (input.diagnostics.length > 0) {
    parts.push(`## 原始診斷 JSON\n\n${fence(JSON.stringify(input.diagnostics, null, 2), 'json')}`);
  } else {
    parts.push('## 原始診斷 JSON\n\n核心沒有回傳任何診斷。');
  }

  parts.push(
    [
      '## 標註檔',
      '',
      '和「分享 → 複製標註」同一份（含原譜、標註、備註、分組與打法分支）。下面「標註檔開始」與「標註檔結束」兩行標記之間、圍欄裡的內容就是完整標註檔，可以直接貼到 App 的「標註 → 分享 → 匯入別人的標註」。',
      '',
      ANNOTATION_BEGIN,
      fence(input.annotationJson.replace(/\n+$/, ''), 'json'),
      ANNOTATION_END,
    ].join('\n'),
  );
  return `${parts.join('\n\n')}\n`;
}

/** 寫入剪貼簿；Clipboard API 不可用時退回 execCommand。 */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // 落到後備方式。
  }
  try {
    const area = document.createElement('textarea');
    area.value = text;
    area.setAttribute('readonly', '');
    area.style.position = 'fixed';
    area.style.top = '-1000px';
    const active = document.activeElement as HTMLElement | null;
    // 開著的 modal 會讓 body 其他地方 inert，textarea 要放進同一個 dialog 才選得到。
    (active?.closest('dialog') ?? document.body).appendChild(area);
    area.select();
    const ok = document.execCommand('copy');
    area.remove();
    active?.focus();
    return ok;
  } catch {
    return false;
  }
}
