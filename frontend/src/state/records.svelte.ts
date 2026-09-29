import { isHumanLabeled, type AnnotationDraft } from '../lib/annotation';
import {
  STORE_RECORDS,
  STORE_SETTINGS,
  dbDelete,
  dbDeleteAnalysesBySource,
  dbGet,
  dbGetAll,
  dbPut,
  hashText,
} from '../lib/db';

/** 從 Majdata 匯入時記下的來源；id 是 Majdata 每次上傳唯一的 song id。 */
export interface MajdataOrigin {
  id: string;
  title: string;
  artist: string;
  designer: string;
  uploader: string;
  levels: (string | null)[];
}

/** simai Wiki 的難度 key，依顯示順序排列；索引與 &inote_1… 對齊（DX 沒有 easy）。 */
export const WIKI_DIFFICULTIES = [
  { key: 'easy', label: 'EASY' },
  { key: 'basic', label: 'BASIC' },
  { key: 'advanced', label: 'ADVANCED' },
  { key: 'expert', label: 'EXPERT' },
  { key: 'master', label: 'MASTER' },
  { key: 'reMaster', label: 'Re:MASTER' },
] as const;

export type WikiDifficultyKey = (typeof WIKI_DIFFICULTIES)[number]['key'];

export const WIKI_TYPE_LABEL: Record<'standard' | 'deluxe', string> = {
  standard: 'Standard',
  deluxe: 'DX',
};

/** 從 simai Wiki 匯入時記下的來源；同 pageId＋chartType＋difficulty 視為同一張。 */
export interface WikiOrigin {
  pageId: number;
  chartType: 'standard' | 'deluxe';
  difficulty: WikiDifficultyKey;
  title: string;
  level: string | null;
  pageUrl: string;
}

export function wikiDifficultyLabel(key: string): string {
  return WIKI_DIFFICULTIES.find((item) => item.key === key)?.label ?? key;
}

export interface ChartRecord {
  id: string;
  /** 新增時間（epoch 毫秒），顯示為副標。 */
  createdAt: number;
  source: string;
  /** 使用者自訂名稱；空白時以譜面開頭（&title 或第一行）當標題。 */
  name?: string;
  /** 原文 SHA-256，用來判斷兩筆是否為同一份譜面、清除分析快取。 */
  sourceHash?: string;
  majdata?: MajdataOrigin;
  /** 舊紀錄沒有這個欄位。 */
  wiki?: WikiOrigin;
  /** 使用者在時間軸上加的標籤，依時間排序。 */
  markers?: TimelineMarker[];
  /** 真人手順標註草稿（lib/annotation.ts）；舊紀錄沒有。 */
  annotation?: AnnotationDraft;
  /** 所在資料夾 id；沒有表示「未分類」。一筆只放一個資料夾。 */
  folder?: string;
  /** 標籤名稱（顏色在 Records.tags），依加入順序。 */
  tags?: string[];
  /** 最後一次開啟的時間（epoch 毫秒）；舊紀錄沒有。 */
  openedAt?: number;
}

/** 譜面紀錄的資料夾，只有一層。 */
export interface RecordFolder {
  id: string;
  name: string;
  createdAt: number;
}

/** 標籤；紀錄以名稱引用，重新命名時一併改掉。 */
export interface RecordTag {
  name: string;
  color: string;
}

/** 標籤色盤：在深色底上清楚，避開左右手的粉紅與藍色。 */
export const TAG_COLORS = ['#f0b429', '#5fc98a', '#a78bfa', '#f08a4b', '#3fc1b0', '#c9d36a', '#8ea3ff', '#e87c9a', '#b0a58f'];

export function nextTagColor(used: string[]): string {
  return TAG_COLORS.find((color) => !used.includes(color)) ?? TAG_COLORS[used.length % TAG_COLORS.length];
}

export function isTagColor(value: unknown): value is string {
  return typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);
}

/** 資料夾與標籤清單存在 settings store 的這個鍵。 */
const ORGANIZE_KEY = 'records.organize';

interface StoredOrganize {
  folders: RecordFolder[];
  tags: RecordTag[];
}

/** 資料夾與標籤名稱：連續空白併成一個、去頭尾，最多 40 字。 */
export function cleanName(value: string): string {
  return value.replace(/\s+/g, ' ').trim().slice(0, 40);
}

function cleanFolders(value: unknown): RecordFolder[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  const out: RecordFolder[] = [];
  for (const item of value) {
    if (typeof item !== 'object' || item === null) continue;
    const { id, name, createdAt } = item as Partial<RecordFolder>;
    if (typeof id !== 'string' || typeof name !== 'string' || !cleanName(name) || seen.has(id)) continue;
    seen.add(id);
    out.push({ id, name: cleanName(name), createdAt: Number.isFinite(createdAt) ? (createdAt as number) : 0 });
  }
  return out;
}

function cleanTagList(value: unknown): RecordTag[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  const out: RecordTag[] = [];
  for (const item of value) {
    if (typeof item !== 'object' || item === null) continue;
    const { name, color } = item as Partial<RecordTag>;
    const clean = typeof name === 'string' ? cleanName(name) : '';
    if (!clean || seen.has(clean)) continue;
    seen.add(clean);
    out.push({ name: clean, color: isTagColor(color) ? color : TAG_COLORS[0] });
  }
  return out;
}

/** 真人已確認的標註數（主線）；排序「標註進度」用。 */
export function recordLabeled(record: ChartRecord): number {
  const notes = record.annotation?.notes;
  return notes ? Object.values(notes).filter(isHumanLabeled).length : 0;
}

/** 排序用的難度：最高一個難度的數值，「+」加 0.5；沒有難度為 -1。 */
export function recordTopLevel(record: ChartRecord): number {
  let top = -1;
  for (const level of recordLevels(record)) {
    if (!level) continue;
    const value = Number.parseFloat(level);
    if (Number.isFinite(value)) top = Math.max(top, value + (level.includes('+') ? 0.5 : 0));
  }
  return top;
}

export interface TimelineMarker {
  id: string;
  /** 譜面時間（秒）。 */
  time: number;
  label: string;
}

/** 讀回資料庫時過濾掉格式不對的標籤。 */
export function cleanMarkers(value: unknown): TimelineMarker[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter(
      (item): item is TimelineMarker =>
        typeof item === 'object' &&
        item !== null &&
        typeof (item as TimelineMarker).id === 'string' &&
        typeof (item as TimelineMarker).label === 'string' &&
        Number.isFinite((item as TimelineMarker).time),
    )
    .sort((a, b) => a.time - b.time);
}

const LEGACY_STORAGE_KEY = 'maimotion.records.v1';

function isRecord(value: unknown): value is ChartRecord {
  if (typeof value !== 'object' || value === null) return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.id === 'string' &&
    typeof item.createdAt === 'number' &&
    typeof item.source === 'string' &&
    (item.name === undefined || typeof item.name === 'string')
  );
}

function loadLegacy(): ChartRecord[] {
  try {
    const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isRecord) : [];
  } catch {
    return [];
  }
}

function pad(value: number): string {
  return String(value).padStart(2, '0');
}

/** 讀 maidata.txt 的 `&key=value`（取第一個）。 */
export function maidataField(source: string, key: string): string {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`^&${escaped}=(.*)$`, 'm').exec(source)?.[1]?.trim() ?? '';
}

/** 譜面開頭：maidata.txt 取 &title，否則取第一個非空白行。 */
export function sourceHeading(source: string): string {
  const title = maidataField(source, 'title');
  if (title) return title;
  const line = source.split('\n').find((item) => item.trim().length > 0);
  return line?.trim() ?? '（空白）';
}

/** Wiki 匯入的原文只有譜面本文，標題改用來源資訊，並註明種類與難度區分同曲的多筆。 */
export function wikiHeading(wiki: WikiOrigin): string {
  return `${wiki.title}（${WIKI_TYPE_LABEL[wiki.chartType] ?? wiki.chartType} ${wikiDifficultyLabel(wiki.difficulty)}）`;
}

/** 主標：自訂名稱，沒有就用 Wiki 來源或譜面開頭。 */
export function recordTitle(record: ChartRecord): string {
  const name = record.name?.trim();
  if (name) return name;
  return record.wiki ? wikiHeading(record.wiki) : sourceHeading(record.source);
}

/** 副標：新增時間。 */
export function recordDate(record: ChartRecord): string {
  const date = new Date(record.createdAt);
  return (
    `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
  );
}

/** 依 &inote_1… 排列的難度；Majdata 匯入以來源資料為準，否則讀 &lv_N。 */
export function recordLevels(record: ChartRecord): (string | null)[] {
  if (record.majdata) return record.majdata.levels;
  if (record.wiki) {
    const { difficulty, level } = record.wiki;
    return WIKI_DIFFICULTIES.map((item) => (item.key === difficulty ? level?.trim() || '?' : null));
  }
  return Array.from({ length: 7 }, (_, index) => {
    const hasChart = new RegExp(`^&inote_${index + 1}=`, 'm').test(record.source);
    const level = maidataField(record.source, `lv_${index + 1}`);
    return hasChart || level ? level || '?' : null;
  });
}

/** 本機搜尋用的文字欄位。 */
export function recordSearchText(record: ChartRecord): string {
  return [
    record.name ?? '',
    maidataField(record.source, 'title'),
    maidataField(record.source, 'artist'),
    maidataField(record.source, 'des'),
    record.majdata?.title ?? '',
    record.majdata?.artist ?? '',
    record.majdata?.designer ?? '',
    record.majdata?.uploader ?? '',
    record.wiki ? wikiHeading(record.wiki) : '',
    record.name || maidataField(record.source, 'title') || record.wiki ? '' : sourceHeading(record.source),
  ]
    .join('\n')
    .toLowerCase();
}

export interface AddOptions {
  name?: string;
  majdata?: MajdataOrigin;
  wiki?: WikiOrigin;
  /** 預設新增後就切過去；批量匯入時不切換目前的譜面。 */
  activate?: boolean;
}

/**
 * 譜面紀錄：存在 IndexedDB（lib/db.ts）。舊版 localStorage 的紀錄第一次啟動時搬過去。
 * 資料庫不可用時只保留在記憶體，本次執行期間仍可使用。
 */
export class Records {
  /** 新的在前。 */
  items = $state<ChartRecord[]>([]);
  folders = $state<RecordFolder[]>([]);
  tags = $state<RecordTag[]>([]);
  #activeId = $state<string | null>(null);

  /** 目前開啟的紀錄；切換時記下開啟時間（排序「最近開啟」用）。 */
  get activeId(): string | null {
    return this.#activeId;
  }

  set activeId(id: string | null) {
    this.#activeId = id;
    if (id) this.#patch(id, { openedAt: Date.now() });
  }
  /** 資料庫讀取完成前清單是空的，畫面用來區分「載入中」與「沒有紀錄」。 */
  loaded = $state(false);

  constructor() {
    void this.#load();
  }

  async #load(): Promise<void> {
    const organize = await dbGet<StoredOrganize>(STORE_SETTINGS, ORGANIZE_KEY);
    // 讀取期間新建的資料夾與標籤保留在後面。
    this.folders = cleanFolders([...(organize?.folders ?? []), ...this.folders]);
    this.tags = cleanTagList([...(organize?.tags ?? []), ...this.tags]);
    const stored = await dbGetAll<unknown>(STORE_RECORDS);
    let items = (stored ?? []).filter(isRecord);
    const legacy = loadLegacy();
    if (legacy.length > 0) {
      const known = new Set(items.map((item) => item.id));
      const moved = legacy.filter((item) => !known.has(item.id));
      let ok = stored !== null;
      for (const record of moved) {
        record.sourceHash = await hashText(record.source);
        if (stored !== null) ok = (await dbPut(STORE_RECORDS, $state.snapshot(record))) && ok;
      }
      items = [...items, ...moved];
      if (ok) {
        try {
          localStorage.removeItem(LEGACY_STORAGE_KEY);
        } catch {
          // 下次啟動會再比對一次 id，不會重複搬移。
        }
      }
    }
    // 補齊舊紀錄缺少的原文雜湊。
    for (const record of items) {
      if (record.sourceHash) continue;
      record.sourceHash = await hashText(record.source);
      void dbPut(STORE_RECORDS, record);
    }
    // 資料夾已不存在的改回未分類；標籤只留字串，缺顏色的補上。
    const folderIds = new Set(this.folders.map((folder) => folder.id));
    for (const record of items) {
      if (record.folder !== undefined && !folderIds.has(record.folder)) delete record.folder;
      if (record.tags !== undefined) {
        record.tags = Array.isArray(record.tags)
          ? [...new Set(record.tags.filter((tag) => typeof tag === 'string').map(cleanName).filter(Boolean))]
          : [];
        if (record.tags.length === 0) delete record.tags;
        for (const name of record.tags ?? []) this.#ensureTag(name);
      }
    }
    items.sort((a, b) => b.createdAt - a.createdAt);
    // 讀取期間新增的紀錄保留在最前面。
    const added = this.items.filter((item) => !items.some((other) => other.id === item.id));
    this.items = [...added, ...items];
    this.loaded = true;
  }

  get(id: string | null): ChartRecord | null {
    if (!id) return null;
    return this.items.find((item) => item.id === id) ?? null;
  }

  get active(): ChartRecord | null {
    return this.get(this.activeId);
  }

  findByMajdataId(songId: string): ChartRecord | null {
    return this.items.find((item) => item.majdata?.id === songId) ?? null;
  }

  findByWiki(pageId: number, chartType: string, difficulty: string): ChartRecord | null {
    return (
      this.items.find(
        (item) =>
          item.wiki?.pageId === pageId &&
          item.wiki.chartType === chartType &&
          item.wiki.difficulty === difficulty,
      ) ?? null
    );
  }

  findByHash(sourceHash: string): ChartRecord | null {
    return this.items.find((item) => item.sourceHash === sourceHash) ?? null;
  }

  async add(source: string, options: AddOptions = {}): Promise<ChartRecord> {
    const name = options.name?.trim();
    const record: ChartRecord = {
      id: `rec-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
      createdAt: Date.now(),
      source,
      sourceHash: await hashText(source),
      ...(name ? { name } : {}),
      ...(options.majdata ? { majdata: options.majdata } : {}),
      ...(options.wiki ? { wiki: options.wiki } : {}),
    };
    this.items = [record, ...this.items];
    if (options.activate !== false) this.activeId = record.id;
    void dbPut(STORE_RECORDS, $state.snapshot(record));
    return record;
  }

  /** 既有紀錄補上 Majdata 來源（原文完全相同的舊匯入），下次搜尋可直接辨識。 */
  attachMajdata(id: string, majdata: MajdataOrigin): void {
    const index = this.items.findIndex((item) => item.id === id);
    if (index < 0) return;
    const next = { ...this.items[index], majdata };
    this.items[index] = next;
    void dbPut(STORE_RECORDS, $state.snapshot(next));
  }

  /** 既有紀錄補上 Wiki 來源（原文完全相同的舊紀錄）。已有 Wiki 來源的不覆蓋。 */
  attachWiki(id: string, wiki: WikiOrigin): void {
    const index = this.items.findIndex((item) => item.id === id);
    if (index < 0 || this.items[index].wiki) return;
    const next = { ...this.items[index], wiki };
    this.items[index] = next;
    void dbPut(STORE_RECORDS, $state.snapshot(next));
  }

  /**
   * 編輯既有紀錄的名稱與原文。來源資訊（Majdata／Wiki）保留；
   * 原文改變時換掉雜湊，舊原文若沒有其他紀錄使用，一併清掉它的分析快取。
   */
  async update(id: string, changes: { name: string; source: string }): Promise<ChartRecord | null> {
    const index = this.items.findIndex((item) => item.id === id);
    if (index < 0) return null;
    const current = this.items[index];
    const name = changes.name.trim();
    const next: ChartRecord = { ...current, source: changes.source };
    if (name) next.name = name;
    else delete next.name;
    const oldHash = current.sourceHash;
    if (changes.source !== current.source) next.sourceHash = await hashText(changes.source);
    // 雜湊計算期間紀錄可能被刪掉。
    const latest = this.items.findIndex((item) => item.id === id);
    if (latest < 0) return null;
    this.items[latest] = next;
    void dbPut(STORE_RECORDS, $state.snapshot(next));
    if (oldHash && oldHash !== next.sourceHash && !this.items.some((item) => item.sourceHash === oldHash)) {
      void dbDeleteAnalysesBySource(oldHash);
    }
    return next;
  }

  /** 清除使用者資料後呼叫：只清記憶體，資料庫由呼叫端清空。 */
  reset(): void {
    this.items = [];
    this.folders = [];
    this.tags = [];
    this.#activeId = null;
  }

  /** 改一筆紀錄的部分欄位並寫回資料庫；值為 undefined 的欄位會被移除。 */
  #patch(id: string, changes: Partial<ChartRecord>): void {
    const index = this.items.findIndex((item) => item.id === id);
    if (index < 0) return;
    const next: ChartRecord = { ...this.items[index], ...changes };
    for (const key of Object.keys(changes) as (keyof ChartRecord)[]) {
      if (changes[key] === undefined) delete next[key];
    }
    this.items[index] = next;
    void dbPut(STORE_RECORDS, $state.snapshot(next));
  }

  #saveOrganize(): void {
    void dbPut(STORE_SETTINGS, $state.snapshot({ folders: this.folders, tags: this.tags }), ORGANIZE_KEY);
  }

  // ---- 資料夾 ----

  folderById(id: string | undefined): RecordFolder | null {
    return id ? (this.folders.find((folder) => folder.id === id) ?? null) : null;
  }

  folderByName(name: string): RecordFolder | null {
    const clean = cleanName(name);
    return this.folders.find((folder) => folder.name === clean) ?? null;
  }

  /** 建立資料夾；同名的已存在就回傳它。名稱空白回傳 null。 */
  createFolder(name: string): RecordFolder | null {
    const clean = cleanName(name);
    if (!clean) return null;
    const existing = this.folderByName(clean);
    if (existing) return existing;
    const folder: RecordFolder = {
      id: `fld-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
      name: clean,
      createdAt: Date.now(),
    };
    this.folders = [...this.folders, folder];
    this.#saveOrganize();
    return folder;
  }

  /** 重新命名；名稱空白或與其他資料夾同名時不改，回傳 false。 */
  renameFolder(id: string, name: string): boolean {
    const clean = cleanName(name);
    const other = this.folderByName(clean);
    if (!clean || (other && other.id !== id)) return false;
    this.folders = this.folders.map((folder) => (folder.id === id ? { ...folder, name: clean } : folder));
    this.#saveOrganize();
    return true;
  }

  /** 刪除資料夾；裡面的紀錄改回「未分類」，紀錄本身不刪。回傳被移出的紀錄 id（復原用）。 */
  deleteFolder(id: string): string[] {
    const moved = this.items.filter((item) => item.folder === id).map((item) => item.id);
    this.folders = this.folders.filter((folder) => folder.id !== id);
    for (const record of moved) this.#patch(record, { folder: undefined });
    this.#saveOrganize();
    return moved;
  }

  /** 把資料夾放回去（刪除後復原）。 */
  restoreFolder(folder: RecordFolder, ids: string[]): void {
    if (!this.folderById(folder.id)) {
      this.folders = [...this.folders, folder].sort((a, b) => a.createdAt - b.createdAt);
      this.#saveOrganize();
    }
    this.moveTo(ids, folder.id);
  }

  /** 把紀錄移到資料夾；null 表示「未分類」。 */
  moveTo(ids: string[], folderId: string | null): void {
    for (const id of ids) this.#patch(id, { folder: folderId ?? undefined });
  }

  // ---- 標籤 ----

  tagColor(name: string): string {
    return this.tags.find((tag) => tag.name === name)?.color ?? TAG_COLORS[TAG_COLORS.length - 1];
  }

  /** 標籤不存在就建立（沒給顏色時自動配色）；回傳正規化後的名稱，空白回傳 null。 */
  #ensureTag(name: string, color?: string): string | null {
    const clean = cleanName(name);
    if (!clean) return null;
    if (!this.tags.some((tag) => tag.name === clean)) {
      const pick = isTagColor(color) ? color : nextTagColor(this.tags.map((tag) => tag.color));
      this.tags = [...this.tags, { name: clean, color: pick }];
      this.#saveOrganize();
    }
    return clean;
  }

  createTag(name: string, color?: string): string | null {
    return this.#ensureTag(name, color);
  }

  setTagColor(name: string, color: string): void {
    if (!isTagColor(color)) return;
    this.tags = this.tags.map((tag) => (tag.name === name ? { ...tag, color } : tag));
    this.#saveOrganize();
  }

  /** 重新命名；新名稱已是另一個標籤時兩者合併。名稱空白回傳 false。 */
  renameTag(from: string, to: string): boolean {
    const clean = cleanName(to);
    if (!clean) return false;
    if (clean === from) return true;
    const merging = this.tags.some((tag) => tag.name === clean);
    this.tags = merging
      ? this.tags.filter((tag) => tag.name !== from)
      : this.tags.map((tag) => (tag.name === from ? { ...tag, name: clean } : tag));
    for (const record of this.items.filter((item) => item.tags?.includes(from))) {
      this.#patch(record.id, { tags: [...new Set(record.tags!.map((tag) => (tag === from ? clean : tag)))] });
    }
    this.#saveOrganize();
    return true;
  }

  /** 刪除標籤並從所有紀錄拿掉；回傳原本帶這個標籤的紀錄 id（復原用）。 */
  deleteTag(name: string): string[] {
    const tagged = this.items.filter((item) => item.tags?.includes(name)).map((item) => item.id);
    this.tags = this.tags.filter((tag) => tag.name !== name);
    for (const id of tagged) {
      const tags = this.get(id)!.tags!.filter((tag) => tag !== name);
      this.#patch(id, { tags: tags.length > 0 ? tags : undefined });
    }
    this.#saveOrganize();
    return tagged;
  }

  /** 替這些紀錄加上（on）或拿掉標籤；加上時標籤不存在就建立。 */
  setTag(ids: string[], name: string, on: boolean, color?: string): void {
    const clean = on ? this.#ensureTag(name, color) : cleanName(name);
    if (!clean) return;
    for (const id of ids) {
      const current = this.get(id)?.tags ?? [];
      if (current.includes(clean) === on) continue;
      const tags = on ? [...current, clean] : current.filter((tag) => tag !== clean);
      this.#patch(id, { tags: tags.length > 0 ? tags : undefined });
    }
  }

  /** 併入時間軸標籤：同一時間（1 毫秒內）同名的不重複加入。回傳新增數量。 */
  addMarkers(id: string, markers: { time: number; label: string }[]): number {
    const current = cleanMarkers(this.get(id)?.markers);
    const added = markers.filter(
      (marker) => !current.some((item) => Math.abs(item.time - marker.time) < 0.001 && item.label === marker.label),
    );
    if (added.length === 0) return 0;
    const stamp = Date.now().toString(36);
    this.setMarkers(id, [
      ...current,
      ...added.map((marker, index) => ({
        id: `mk-${stamp}-${index}-${Math.random().toString(36).slice(2, 6)}`,
        time: marker.time,
        label: marker.label,
      })),
    ]);
    return added.length;
  }

  /** 一次刪除多筆；回傳被刪掉的紀錄，交給 restore() 復原。 */
  removeMany(ids: string[]): ChartRecord[] {
    const removed = this.items
      .filter((item) => ids.includes(item.id))
      .map((item) => $state.snapshot(item) as ChartRecord);
    for (const record of removed) this.remove(record.id);
    return removed;
  }

  /** 復原刪除的紀錄；分析快取已經清掉，重新開啟時會再分析一次。 */
  restore(removed: ChartRecord[]): void {
    const known = new Set(this.items.map((item) => item.id));
    const folderIds = new Set(this.folders.map((folder) => folder.id));
    const back = removed.filter((record) => !known.has(record.id));
    for (const record of back) {
      if (record.folder && !folderIds.has(record.folder)) delete record.folder;
      for (const name of record.tags ?? []) this.#ensureTag(name);
      void dbPut(STORE_RECORDS, record);
    }
    this.items = [...this.items, ...back].sort((a, b) => b.createdAt - a.createdAt);
  }

  /** 取代某筆紀錄的真人手順標註並寫回資料庫。 */
  setAnnotation(id: string, annotation: AnnotationDraft): void {
    const index = this.items.findIndex((item) => item.id === id);
    if (index < 0) return;
    const next = { ...this.items[index], annotation };
    this.items[index] = next;
    void dbPut(STORE_RECORDS, $state.snapshot(next));
  }

  /** 取代某筆紀錄的時間軸標籤並寫回資料庫。 */
  setMarkers(id: string, markers: TimelineMarker[]): void {
    const index = this.items.findIndex((item) => item.id === id);
    if (index < 0) return;
    const sorted = [...markers].sort((a, b) => a.time - b.time);
    const next = { ...this.items[index], markers: sorted };
    this.items[index] = next;
    void dbPut(STORE_RECORDS, $state.snapshot(next));
  }

  remove(id: string): void {
    const target = this.get(id);
    this.items = this.items.filter((item) => item.id !== id);
    if (this.activeId === id) this.activeId = null;
    void dbDelete(STORE_RECORDS, id);
    const hash = target?.sourceHash;
    if (hash && !this.items.some((item) => item.sourceHash === hash)) {
      void dbDeleteAnalysesBySource(hash);
    }
  }
}

export const records = new Records();
