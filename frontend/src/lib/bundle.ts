// 多份譜面的整包標註檔（maimotion-hand-annotation-bundle 第 1 版）。
// 每一份的 annotation 就是單份標註檔（maimotion-hand-annotation 第 1 版）原樣，
// 解析與合併沿用 lib/annotation.ts；record 另外帶譜面紀錄的整理資訊，匯入時還原。

import { parseValue, serialize } from './annotation';
import type { HandAnnotation } from './types';
import type { MajdataOrigin, RecordTag, WikiOrigin } from '../state/records.svelte';

export const BUNDLE_FORMAT = 'maimotion-hand-annotation-bundle';
export const BUNDLE_VERSION = 1;

/** 譜面紀錄的整理資訊；資料夾與標籤以名稱記錄，匯入時依名稱對應或建立。 */
export interface BundleRecord {
  name?: string;
  folder?: string;
  tags: string[];
  /** 時間軸標籤（秒） */
  markers: { time: number; label: string }[];
  /** 原本新增的時間（ISO） */
  createdAt?: string;
  majdata?: MajdataOrigin;
  wiki?: WikiOrigin;
}

export interface BundleItem {
  record: BundleRecord;
  annotation: HandAnnotation;
}

export interface Bundle {
  format: typeof BUNDLE_FORMAT;
  version: typeof BUNDLE_VERSION;
  exportedAt: string;
  app: string;
  /** 用到的標籤與顏色 */
  tags: RecordTag[];
  items: BundleItem[];
}

/**
 * 整包檔的文字：一般 JSON，但每份的 record 一行、annotation 沿用單份標註檔的排版
 * （每顆音符一行、原譜在最後），方便閱讀與 diff。
 */
export function serializeBundle(bundle: Bundle): string {
  const line = (value: unknown) => JSON.stringify(value);
  const indent = (text: string, pad: string) =>
    text
      .trimEnd()
      .split('\n')
      .map((row, index) => (index === 0 ? row : pad + row))
      .join('\n');
  const items = bundle.items.map(
    (item) =>
      `    {\n      "record": ${line(item.record)},\n      "annotation": ${indent(serialize(item.annotation), '      ')}\n    }`,
  );
  const tags = bundle.tags.map((tag) => `    ${line(tag)}`);
  return [
    '{',
    `  "format": ${line(bundle.format)},`,
    `  "version": ${bundle.version},`,
    `  "exportedAt": ${line(bundle.exportedAt)},`,
    `  "app": ${line(bundle.app)},`,
    `  "tags": ${tags.length > 0 ? `[\n${tags.join(',\n')}\n  ]` : '[]'},`,
    `  "items": ${items.length > 0 ? `[\n${items.join(',\n')}\n  ]` : '[]'}`,
    '}',
    '',
  ].join('\n');
}

/** 讀檔結果中的一份；單份標註檔也包成這個形狀。 */
export interface ParsedItem {
  record: BundleRecord;
  annotation: HandAnnotation;
  /** 格式不對而略過的單顆標註數 */
  skipped: number;
}

export type ParsedImport =
  | { ok: true; kind: 'single' | 'bundle'; items: ParsedItem[]; tags: RecordTag[]; errors: string[] }
  | { ok: false; error: string };

function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function cleanRecord(value: unknown): BundleRecord {
  const raw = (typeof value === 'object' && value !== null ? value : {}) as Record<string, unknown>;
  const markers = Array.isArray(raw.markers)
    ? raw.markers.flatMap((item) => {
        const marker = (typeof item === 'object' && item !== null ? item : {}) as Record<string, unknown>;
        return typeof marker.label === 'string' && Number.isFinite(marker.time)
          ? [{ time: marker.time as number, label: marker.label }]
          : [];
      })
    : [];
  const origin = <T>(key: string, check: (v: Record<string, unknown>) => boolean): T | undefined => {
    const v = raw[key];
    return typeof v === 'object' && v !== null && check(v as Record<string, unknown>) ? (v as T) : undefined;
  };
  const majdata = origin<MajdataOrigin>('majdata', (v) => typeof v.id === 'string' && Array.isArray(v.levels));
  const wiki = origin<WikiOrigin>(
    'wiki',
    (v) => Number.isFinite(v.pageId) && typeof v.chartType === 'string' && typeof v.difficulty === 'string',
  );
  return {
    ...(text(raw.name) ? { name: text(raw.name) } : {}),
    ...(text(raw.folder) ? { folder: text(raw.folder) } : {}),
    tags: Array.isArray(raw.tags) ? raw.tags.filter((tag): tag is string => typeof tag === 'string' && tag !== '') : [],
    markers,
    ...(text(raw.createdAt) ? { createdAt: text(raw.createdAt) } : {}),
    ...(majdata ? { majdata } : {}),
    ...(wiki ? { wiki } : {}),
  };
}

/** 開啟的檔案文字：整包或單份標註檔都接受。單份的部分失敗記在 errors，不整份放棄。 */
export function parseImport(input: string): ParsedImport {
  let raw: unknown;
  try {
    raw = JSON.parse(input);
  } catch {
    return { ok: false, error: '不是有效的 JSON。' };
  }
  if (typeof raw !== 'object' || raw === null) return { ok: false, error: '檔案內容是空的。' };
  const data = raw as Record<string, unknown>;
  if (data.format !== BUNDLE_FORMAT) {
    const single = parseValue(raw);
    if (!single.ok) return single;
    return {
      ok: true,
      kind: 'single',
      items: [{ record: { tags: [], markers: [] }, annotation: single.file, skipped: single.skipped }],
      tags: [],
      errors: [],
    };
  }
  if (data.version !== BUNDLE_VERSION) {
    return { ok: false, error: `整包標註檔版本 ${String(data.version)} 不支援，本版只讀第 ${BUNDLE_VERSION} 版。` };
  }
  const items: ParsedItem[] = [];
  const errors: string[] = [];
  (Array.isArray(data.items) ? data.items : []).forEach((value, index) => {
    const entry = (typeof value === 'object' && value !== null ? value : {}) as Record<string, unknown>;
    const parsed = parseValue(entry.annotation);
    if (!parsed.ok) {
      errors.push(`第 ${index + 1} 份：${parsed.error}`);
      return;
    }
    items.push({ record: cleanRecord(entry.record), annotation: parsed.file, skipped: parsed.skipped });
  });
  const tags = Array.isArray(data.tags)
    ? data.tags.flatMap((item) => {
        const tag = (typeof item === 'object' && item !== null ? item : {}) as Record<string, unknown>;
        return typeof tag.name === 'string' && tag.name ? [{ name: tag.name, color: text(tag.color) }] : [];
      })
    : [];
  if (items.length === 0 && errors.length === 0) return { ok: false, error: '整包標註檔裡沒有任何譜面。' };
  return { ok: true, kind: 'bundle', items, tags, errors };
}

/** 整包檔名：名稱轉成安全字元，附日期與份數。 */
export function bundleFileName(base: string, count: number, now = new Date()): string {
  const safe = (base || 'records').replace(/[\\/:*?"<>|\s]+/g, '_').slice(0, 60);
  const day = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, '0')}${String(now.getDate()).padStart(2, '0')}`;
  return `${safe}.${day}.${count}份.maimotion-hands-bundle.json`;
}
