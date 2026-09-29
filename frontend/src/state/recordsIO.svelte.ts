// 譜面紀錄的批量匯出與匯入（整包標註檔，見 lib/bundle.ts）。
import { cleanDraft, emptyDraft, isHumanLabeled, merge, toFile, type AnnotationDraft } from '../lib/annotation';
import { appVersion, chartNoteKeys, isDesktop } from '../lib/api';
import {
  BUNDLE_FORMAT,
  BUNDLE_VERSION,
  bundleFileName,
  parseImport,
  serializeBundle,
  type Bundle,
  type ParsedItem,
} from '../lib/bundle';
import { hashText } from '../lib/db';
import { annotation, checkSource } from './annotation.svelte';
import { cleanMarkers, recordTitle, records, type ChartRecord, type RecordTag } from './records.svelte';

/** 依譜面順序的音符鍵：目前開啟且解析完成的直接用；其他的請 Rust 只解析不求解。 */
async function noteKeys(record: ChartRecord): Promise<string[] | null> {
  if (annotation.recordId === record.id && annotation.keysReady) {
    return annotation.ordered.map((note) => note.key ?? '');
  }
  if (!isDesktop()) return null;
  try {
    return await chartNoteKeys(record.source);
  } catch {
    return null;
  }
}

export interface ExportResult {
  text: string;
  fileName: string;
  count: number;
  /** 沒有任何真人手順的份數（仍會匯出原譜與整理資訊） */
  unlabeled: number;
  /** 拿不到解析結果、改依時間排列音符的份數 */
  unordered: number;
}

/** 把這些紀錄（依傳入順序）打包成一個整包標註檔。 */
export async function exportRecords(ids: string[]): Promise<ExportResult | null> {
  annotation.flush();
  const list = ids.map((id) => records.get(id)).filter((record): record is ChartRecord => record !== null);
  if (list.length === 0) return null;
  let unlabeled = 0;
  let unordered = 0;
  const used = new Set<string>();
  const items = [];
  for (const record of list) {
    const snapshot = $state.snapshot(record) as ChartRecord;
    const draft: AnnotationDraft = cleanDraft(snapshot.annotation) ?? emptyDraft(recordTitle(snapshot));
    if (!draft.title) draft.title = recordTitle(snapshot);
    if (!Object.values(draft.notes).some(isHumanLabeled)) unlabeled += 1;
    const keys = await noteKeys(snapshot);
    if (!keys) unordered += 1;
    const folder = records.folderById(snapshot.folder)?.name;
    const tags = snapshot.tags ?? [];
    for (const tag of tags) used.add(tag);
    items.push({
      record: {
        ...(snapshot.name?.trim() ? { name: snapshot.name.trim() } : {}),
        ...(folder ? { folder } : {}),
        tags,
        markers: cleanMarkers(snapshot.markers).map(({ time, label }) => ({ time, label })),
        createdAt: new Date(snapshot.createdAt).toISOString(),
        ...(snapshot.majdata ? { majdata: snapshot.majdata } : {}),
        ...(snapshot.wiki ? { wiki: snapshot.wiki } : {}),
      },
      annotation: toFile(draft, {
        source: snapshot.source,
        sha256: snapshot.sourceHash ?? (await hashText(snapshot.source)),
        firstSeconds: 0,
        keys,
      }),
    });
  }
  const bundle: Bundle = {
    format: BUNDLE_FORMAT,
    version: BUNDLE_VERSION,
    exportedAt: new Date().toISOString(),
    app: await appVersion(),
    tags: records.tags.filter((tag) => used.has(tag.name)).map((tag) => ({ ...tag })),
    items,
  };
  // 檔名：只有一份用它的標題；全部在同一個資料夾用資料夾名稱。
  const folders = new Set(list.map((record) => record.folder ?? ''));
  const base =
    list.length === 1
      ? recordTitle(list[0])
      : folders.size === 1 && list[0].folder
        ? (records.folderById(list[0].folder)?.name ?? 'records')
        : 'records';
  return {
    text: serializeBundle(bundle),
    fileName: bundleFileName(base, list.length),
    count: list.length,
    unlabeled,
    unordered,
  };
}

/** 讀進來等待確認的標記檔；App 層的匯入視窗依此開啟。 */
class ImportRequest {
  open = $state(false);
  items = $state.raw<ParsedItem[]>([]);
  tags = $state.raw<RecordTag[]>([]);
  errors = $state.raw<string[]>([]);

  show(items: ParsedItem[], tags: RecordTag[], errors: string[]): void {
    this.items = items;
    this.tags = tags;
    this.errors = errors;
    this.open = true;
  }
}

export const importRequest = new ImportRequest();

/** 讀取選到的檔案（整包或單份標記檔都接受），合併成一次匯入。 */
export function parseImportTexts(files: { name: string; text: string }[]): {
  items: ParsedItem[];
  tags: RecordTag[];
  errors: string[];
} {
  const items: ParsedItem[] = [];
  const tags: RecordTag[] = [];
  const errors: string[] = [];
  for (const file of files) {
    const parsed = parseImport(file.text);
    if (!parsed.ok) {
      errors.push(`${file.name}：${parsed.error}`);
      continue;
    }
    items.push(...parsed.items);
    tags.push(...parsed.tags.filter((tag) => !tags.some((item) => item.name === tag.name)));
    errors.push(...parsed.errors.map((error) => `${file.name} ${error}`));
  }
  return { items, tags, errors };
}

export interface ImportPreview {
  title: string;
  /** 已有同一份原譜的紀錄 */
  existing: ChartRecord | null;
  /** 真人已確認的標註數（主線） */
  labeled: number;
  folder?: string;
  tags: string[];
  markers: number;
  /** 原譜被改過之類不能匯入的原因 */
  error?: string;
}

export async function previewImport(items: ParsedItem[]): Promise<ImportPreview[]> {
  const out: ImportPreview[] = [];
  for (const item of items) {
    const file = item.annotation;
    const error = (await checkSource(file)) ?? undefined;
    const existing = error ? null : records.findByHash(await hashText(file.chart.source));
    out.push({
      title: item.record.name || file.title || recordTitle({ id: '', createdAt: 0, source: file.chart.source }),
      existing,
      labeled: file.notes.filter(isHumanLabeled).length,
      folder: item.record.folder,
      tags: item.record.tags,
      markers: item.record.markers.length,
      ...(error ? { error } : {}),
    });
  }
  return out;
}

export interface ImportOptions {
  mode: 'fill' | 'replace';
  /** 套用檔案裡的資料夾、標籤與時間軸標籤 */
  organize: boolean;
}

export interface ImportSummary {
  created: number;
  merged: number;
  failed: string[];
  /** 新增的手順顆數 */
  added: number;
  /** 手順不同的顆數 */
  conflicts: number;
  markers: number;
}

/** 匯入：有同一份原譜的紀錄就合併，沒有就連同原譜新增（不切換目前開啟的譜面）。 */
export async function importItems(
  items: ParsedItem[],
  tags: RecordTag[],
  options: ImportOptions,
): Promise<ImportSummary> {
  annotation.flush();
  const summary: ImportSummary = { created: 0, merged: 0, failed: [], added: 0, conflicts: 0, markers: 0 };
  if (options.organize) for (const tag of tags) records.createTag(tag.name, tag.color);
  for (const item of items) {
    const file = item.annotation;
    const tampered = await checkSource(file);
    if (tampered) {
      summary.failed.push(`${file.title || '（未命名）'}：${tampered}`);
      continue;
    }
    const hash = await hashText(file.chart.source);
    let record = records.findByHash(hash);
    const created = !record;
    if (!record) {
      record = await records.add(file.chart.source, {
        name: item.record.name,
        majdata: item.record.majdata,
        wiki: item.record.wiki,
        activate: false,
      });
      summary.created += 1;
    } else {
      summary.merged += 1;
    }
    const id = record.id;
    if (annotation.recordId === id) {
      const result = annotation.applyMerge(file, options.mode);
      summary.added += result.added;
      summary.conflicts += result.conflicts.length;
    } else {
      const current = records.get(id)!;
      const mine = cleanDraft($state.snapshot(current.annotation)) ?? emptyDraft(recordTitle(current));
      const result = merge(mine, file, options.mode);
      result.draft.updatedAt = Date.now();
      records.setAnnotation(id, result.draft);
      summary.added += result.added;
      summary.conflicts += result.conflicts.length;
    }
    if (!options.organize) continue;
    if (item.record.folder && (created || !records.get(id)?.folder)) {
      const folder = records.createFolder(item.record.folder);
      if (folder) records.moveTo([id], folder.id);
    }
    for (const tag of item.record.tags) records.setTag([id], tag, true);
    summary.markers += records.addMarkers(id, item.record.markers);
  }
  return summary;
}
