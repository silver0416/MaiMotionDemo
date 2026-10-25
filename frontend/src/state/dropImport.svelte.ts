// 把檔案拖進主視窗匯入：標記檔交給匯入視窗（同「開啟標記檔」），譜面文字檔等同貼進「新增」再生成。
import { STATUS_LABEL } from '../lib/contract';
import { hashText } from '../lib/db';
import { byteOffsetToIndex } from '../lib/text';
import type { AnalyzeResponse, AnalyzeStatus, Diagnostic } from '../lib/types';
import { errorLog } from './errorLog.svelte';
import { records, sourceHeading } from './records.svelte';
import { importRequest, parseImportTexts } from './recordsIO.svelte';
import { session } from './session.svelte';
import { toasts } from './toasts.svelte';

/** 譜面文字檔接受的副檔名；沒有副檔名的也當文字檔試讀。 */
const CHART_EXTENSIONS = new Set(['txt', 'simai', '']);
const MAX_BYTES = 20 * 1024 * 1024;
const TOAST_ID = 'drop-import';

/** 新增視窗要預先放進輸入框並立即生成的譜面（拖進來的譜面分析失敗時，從通知打開看診斷）。 */
class ChartDraftRequest {
  source = $state<string | null>(null);
  /** App 層打開新增視窗。 */
  open: () => void = () => {};

  show(source: string): void {
    this.source = source;
    this.open();
  }

  take(): string | null {
    const source = this.source;
    this.source = null;
    return source;
  }
}

export const chartDraftRequest = new ChartDraftRequest();

/** 這次拖曳帶的是檔案（不是頁面內拖曳清單列或選取的文字）。 */
export function carriesFiles(event: DragEvent): boolean {
  return [...(event.dataTransfer?.types ?? [])].includes('Files');
}

function extension(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
}

function usable(response: AnalyzeResponse): boolean {
  return response.chart !== null && response.status !== 'invalid' && response.status !== 'unsupported';
}

/** 第一項錯誤的位置與原因，例如「第 12 行第 3 字：無法辨識的音符」。 */
function firstProblem(source: string, response: AnalyzeResponse): string {
  const diagnostic: Diagnostic | undefined =
    response.diagnostics.find((item) => item.severity === 'error') ?? response.diagnostics[0];
  const status = response.status as AnalyzeStatus;
  const label = STATUS_LABEL[status] ?? response.status;
  if (!diagnostic) return label;
  const span = diagnostic.sourceSpan;
  if (!span) return `${label}，${diagnostic.message}`;
  const before = source.slice(0, byteOffsetToIndex(source, span.start)).split('\n');
  return `第 ${before.length} 行第 ${before[before.length - 1].length + 1} 字：${diagnostic.message}`;
}

/** 開著任何對話框時不接拖放：對話框是 modal，匯入視窗或新增視窗不能疊上去。 */
export function dialogOpen(): boolean {
  return document.querySelector('dialog[open]') !== null;
}

interface ReadFile {
  name: string;
  text: string;
}

async function readFiles(event: DragEvent): Promise<{ files: ReadFile[]; errors: string[] }> {
  const transfer = event.dataTransfer;
  const files: ReadFile[] = [];
  const errors: string[] = [];
  if (!transfer) return { files, errors };
  // 資料夾在 files 裡也是一個 File，只能從 entry 分辨。
  const folders = new Set(
    [...transfer.items]
      .map((item) => item.webkitGetAsEntry?.())
      .filter((entry) => entry?.isDirectory)
      .map((entry) => entry!.name),
  );
  for (const file of [...transfer.files]) {
    if (folders.has(file.name)) {
      errors.push(`${file.name}：這是資料夾，請把裡面的檔案拖進來。`);
      continue;
    }
    if (file.size > MAX_BYTES) {
      errors.push(`${file.name}：檔案超過 20 MB，不像標記檔或譜面。`);
      continue;
    }
    try {
      files.push({ name: file.name, text: await file.text() });
    } catch {
      errors.push(`${file.name}：讀不到檔案內容。`);
    }
  }
  return { files, errors };
}

/** 依副檔名與內容分成標記檔（JSON）與譜面文字檔；其他類型列進錯誤。 */
function classify(files: ReadFile[], errors: string[]): { marks: ReadFile[]; charts: ReadFile[] } {
  const marks: ReadFile[] = [];
  const charts: ReadFile[] = [];
  for (const file of files) {
    const ext = extension(file.name);
    const text = file.text;
    if (ext === 'json' || (CHART_EXTENSIONS.has(ext) && text.trimStart().startsWith('{'))) {
      marks.push(file);
      continue;
    }
    if (!CHART_EXTENSIONS.has(ext)) {
      errors.push(`${file.name}：不支援 .${ext} 檔，只能匯入標記檔（.json）或 simai 譜面文字檔（.txt）。`);
      continue;
    }
    if (text.includes('\u0000')) {
      errors.push(`${file.name}：不是文字檔。`);
      continue;
    }
    if (text.includes('�')) {
      errors.push(`${file.name}：不是 UTF-8 編碼，請先另存成 UTF-8 再匯入。`);
      continue;
    }
    if (text.trim().length === 0) {
      errors.push(`${file.name}：檔案是空的。`);
      continue;
    }
    charts.push(file);
  }
  return { marks, charts };
}

/** 一份一份送 Rust 核心分析，可用的才新增紀錄（同新增視窗的「生成並新增」）。 */
async function addCharts(
  charts: ReadFile[],
  errors: string[],
): Promise<{ added: number; opened: number; rejected: string | null }> {
  let added = 0;
  let opened = 0;
  /** 第一份被核心退回的原文，通知上可打開新增視窗看完整診斷。 */
  let firstRejected: string | null = null;
  for (const chart of charts) {
    const existing = records.findByHash(await hashText(chart.text));
    if (existing) {
      // 同一份原文已在紀錄裡：直接開啟，不重複新增。
      records.activeId = existing.id;
      opened += 1;
      continue;
    }
    const response = await session.analyze(chart.text, usable);
    if (!response) {
      const failure = session.lastFailure?.request.source === chart.text ? session.lastFailure : null;
      const message = failure?.message ?? session.errorMessage ?? '呼叫核心失敗';
      errors.push(`${chart.name}：${message}`);
      if (failure) {
        void errorLog.record({
          kind: 'analysis',
          origin: '拖曳匯入',
          title: sourceHeading(chart.text),
          summary: message,
          debug: $state.snapshot({
            context: '拖曳匯入',
            source: chart.text,
            config: failure.request.solverConfig,
            firstSeconds: failure.request.firstSeconds,
            requestId: failure.request.requestId,
            response: null,
            error: message,
          }),
        });
      }
      continue;
    }
    if (!usable(response)) {
      firstRejected ??= chart.text;
      const problem = firstProblem(chart.text, response);
      errors.push(`${chart.name}：${problem}`);
      void errorLog.record({
        kind: 'rejected',
        origin: '拖曳匯入',
        title: sourceHeading(chart.text),
        summary: `${STATUS_LABEL[response.status as AnalyzeStatus] ?? response.status}：${response.diagnostics[0]?.message ?? '沒有診斷'}`,
        debug: $state.snapshot({
          context: '拖曳匯入',
          source: chart.text,
          config: session.requestConfig,
          firstSeconds: session.firstSeconds,
          requestId: response.requestId,
          response,
          error: null,
        }),
      });
      continue;
    }
    await records.add(chart.text);
    added += 1;
  }
  return { added, opened, rejected: firstRejected };
}

/** 放下檔案：標記檔開匯入視窗確認，譜面逐份分析新增；每個不能匯入的檔案都列出原因。 */
export async function importDropped(event: DragEvent): Promise<void> {
  if (dialogOpen()) {
    toasts.show({
      id: TOAST_ID,
      tone: 'info',
      title: '先關閉開著的視窗',
      body: '關掉目前的對話框後，再把檔案拖進主視窗匯入。',
    });
    return;
  }
  const { files, errors } = await readFiles(event);
  const { marks, charts } = classify(files, errors);

  // 有標記檔可匯入時，前面讀不了的檔案一起列在匯入視窗裡（匯入視窗是 modal，通知會被蓋住）。
  let dialogShown = false;
  if (marks.length > 0) {
    const parsed = parseImportTexts(marks);
    if (parsed.items.length > 0) {
      importRequest.show(parsed.items, parsed.tags, [...errors, ...parsed.errors]);
      errors.length = 0;
      dialogShown = true;
    } else {
      errors.push(...parsed.errors);
    }
  }

  const { added, opened, rejected } =
    charts.length > 0 ? await addCharts(charts, errors) : { added: 0, opened: 0, rejected: null };
  const done = [
    ...(added > 0 ? [`已新增 ${added} 份譜面`] : []),
    ...(opened > 0 ? [`${opened} 份已在紀錄裡，直接開啟`] : []),
  ];

  if (errors.length > 0) {
    toasts.show({
      id: TOAST_ID,
      tone: done.length > 0 || dialogShown ? 'warn' : 'error',
      title: `${errors.length} 個檔案沒有匯入`,
      body: [...done, ...errors].map((line) => line.replace(/。$/, '')).join('；'),
      sticky: true,
      ...(rejected !== null
        ? { action: { label: '在新增視窗查看', icon: 'file-text' as const, run: () => chartDraftRequest.show(rejected) } }
        : {}),
    });
    return;
  }
  if (done.length > 0) {
    toasts.show({ id: TOAST_ID, tone: 'ok', title: done.join('，') });
    return;
  }
  // 只拖了標記檔：結果在匯入視窗裡。
  if (!dialogShown) toasts.show({ id: TOAST_ID, tone: 'error', title: '沒有讀到任何檔案' });
}
