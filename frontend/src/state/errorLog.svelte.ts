import { appVersion } from '../lib/api';
import { BUILD_ID } from '../lib/build';
import { STORE_SETTINGS, dbGet, dbPut, hashText } from '../lib/db';
import { buildDebugReport, type DebugInput } from '../lib/debug';

/** 失敗發生在哪一步。 */
export type ImportFailureKind = 'download' | 'analysis' | 'rejected';

export const FAILURE_KIND_LABEL: Record<ImportFailureKind, string> = {
  download: '下載失敗',
  analysis: '呼叫核心失敗',
  rejected: '核心退回',
};

export interface ErrorEntry {
  fingerprint: string;
  kind: ImportFailureKind;
  /** 匯入管道，例如「Majdata」「simai Wiki」「新增譜面」。 */
  origin: string;
  /** 譜面名稱或來源識別，清單上辨識用。 */
  title: string;
  /** 一行摘要。 */
  summary: string;
  firstAt: number;
  lastAt: number;
  /** 同一個錯誤發生的次數；重複發生只累加，不另開一筆。 */
  count: number;
  /** 第一次發生時組好的完整除錯報告（Markdown）。 */
  report: string;
}

export interface ImportFailure {
  kind: ImportFailureKind;
  origin: string;
  title: string;
  /**
   * 判斷「同一個錯誤」用的譜面身分，例如 Majdata song id、Wiki 頁面＋難度；
   * 手動貼上的譜面留空，改用原文雜湊。
   */
  identity?: string;
  summary: string;
  debug: Omit<DebugInput, 'appVersion'>;
}

interface StoredLog {
  entries: ErrorEntry[];
  /** 已複製給開發者的錯誤指紋；同一份建置再發生不會重新記錄。 */
  reported: string[];
}

const LOG_KEY = 'errorLog';
const MAX_ENTRIES = 50;
const MAX_REPORTED = 500;

function isEntry(value: unknown): value is ErrorEntry {
  if (!value || typeof value !== 'object') return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.fingerprint === 'string' &&
    typeof item.report === 'string' &&
    typeof item.lastAt === 'number' &&
    typeof item.count === 'number'
  );
}

/**
 * 譜面匯入失敗的紀錄，存在 settings store。
 * 指紋包含 App 版本與 build 號：同一份建置重複發生只記一次；換了建置仍失敗會重新記錄。
 */
export class ErrorLog {
  entries = $state<ErrorEntry[]>([]);
  #reported: string[] = [];
  #ready: Promise<void>;

  constructor() {
    this.#ready = this.#load();
  }

  async #load(): Promise<void> {
    const stored = await dbGet<StoredLog>(STORE_SETTINGS, LOG_KEY);
    if (!stored) return;
    const loaded = Array.isArray(stored.entries) ? stored.entries.filter(isEntry) : [];
    // 讀取期間記下的錯誤保留。
    const known = new Set(this.entries.map((entry) => entry.fingerprint));
    this.entries = [...this.entries, ...loaded.filter((entry) => !known.has(entry.fingerprint))];
    this.#reported = Array.isArray(stored.reported)
      ? stored.reported.filter((item): item is string => typeof item === 'string')
      : [];
  }

  #save(): Promise<boolean> {
    const value: StoredLog = {
      entries: $state.snapshot(this.entries) as ErrorEntry[],
      reported: this.#reported.slice(-MAX_REPORTED),
    };
    return dbPut(STORE_SETTINGS, value, LOG_KEY);
  }

  async record(failure: ImportFailure): Promise<void> {
    await this.#ready;
    const version = await appVersion();
    const identity = failure.identity || (await hashText(failure.debug.source));
    const diagnostics = (failure.debug.response?.diagnostics ?? [])
      .map((item) => `${item.code}@${item.sourceSpan?.start ?? '-'}`)
      .join(',');
    const fingerprint = await hashText(
      [version, BUILD_ID, failure.origin, failure.kind, identity, failure.summary, diagnostics].join('\n'),
    );
    if (this.#reported.includes(fingerprint)) return;

    const now = Date.now();
    const index = this.entries.findIndex((entry) => entry.fingerprint === fingerprint);
    if (index >= 0) {
      const entry = this.entries[index];
      this.entries[index] = { ...entry, lastAt: now, count: entry.count + 1 };
    } else {
      const report = buildDebugReport({
        ...failure.debug,
        appVersion: `${version}（build ${BUILD_ID}）`,
        extra: { 匯入管道: failure.origin, 失敗步驟: FAILURE_KIND_LABEL[failure.kind], ...failure.debug.extra },
      });
      const entry: ErrorEntry = {
        fingerprint,
        kind: failure.kind,
        origin: failure.origin,
        title: failure.title,
        summary: failure.summary,
        firstAt: now,
        lastAt: now,
        count: 1,
        report,
      };
      this.entries = [entry, ...this.entries].slice(0, MAX_ENTRIES);
    }
    await this.#save();
  }

  /** 全部紀錄合併成一份報告，依發生時間新到舊。 */
  buildReport(): string {
    const lines = [`# MaiMotionDemo 匯入錯誤紀錄（${this.entries.length} 項）`, ''];
    this.entries.forEach((entry, index) => {
      const times =
        entry.count > 1
          ? `發生 ${entry.count} 次，首次 ${new Date(entry.firstAt).toISOString()}，最近 ${new Date(entry.lastAt).toISOString()}`
          : `發生於 ${new Date(entry.firstAt).toISOString()}`;
      lines.push('---', '', `<!-- 第 ${index + 1} 項：${entry.origin}・${entry.title}・${times} -->`, '');
      // 各項報告的標題降一級，合併後仍是一份結構清楚的 Markdown。
      lines.push(entry.report.replace(/^(#+) /gm, '#$1 '));
    });
    return `${lines.join('\n')}\n`;
  }

  /** 複製完成後呼叫：清空清單，並記住這些錯誤已經回報過。 */
  async markReported(): Promise<void> {
    await this.#ready;
    this.#reported = [...this.#reported, ...this.entries.map((entry) => entry.fingerprint)];
    this.entries = [];
    await this.#save();
  }

  /** 清除使用者資料時一併歸零（不寫回資料庫，由呼叫端清空整個 store）。 */
  reset(): void {
    this.entries = [];
    this.#reported = [];
  }
}

export const errorLog = new ErrorLog();
