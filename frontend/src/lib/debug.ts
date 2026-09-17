import {
  SCHEMA_VERSION,
  SCORING_LABEL,
  SCORING_V1,
  STATUS_LABEL,
  isLegacySolution,
  isV2Solution,
  isV3Solution,
  requestModelOf,
} from './contract';
import { formatClock } from './format';
import { byteOffsetToIndex } from './text';
import type { AnalyzeResponse, AnalyzeStatus, Diagnostic, Note, Solution, SolverConfig } from './types';

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

/** 依方案實際的 scoringModel 取出對應的分數欄位，不用欄位是否存在來猜版本。 */
function scoreEntry(solution: Solution): Record<string, unknown> {
  const base = { id: solution.id, scoringModel: solution.scoringModel ?? SCORING_V1 };
  if (isV3Solution(solution) || isV2Solution(solution)) {
    return { ...base, score: solution.score, scoreBreakdown: solution.scoreBreakdown };
  }
  if (isLegacySolution(solution)) {
    return { ...base, totalCost: solution.totalCost, costBreakdown: solution.costBreakdown };
  }
  return { ...base, note: '無法辨識的 scoringModel' };
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
    const model = requestModelOf(input.config);
    meta.push(`- 評分方式：${SCORING_LABEL[model]}（${model}，預期 schemaVersion ${SCHEMA_VERSION[model]}）`);
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
