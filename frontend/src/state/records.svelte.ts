import {
  STORE_RECORDS,
  dbDelete,
  dbDeleteAnalysesBySource,
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

/** 本地搜尋用的文字欄位。 */
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
}

/**
 * 譜面紀錄：存在 IndexedDB（lib/db.ts）。舊版 localStorage 的紀錄第一次啟動時搬過去。
 * 資料庫不可用時只保留在記憶體，本次執行期間仍可使用。
 */
export class Records {
  /** 新的在前。 */
  items = $state<ChartRecord[]>([]);
  activeId = $state<string | null>(null);
  /** 資料庫讀取完成前清單是空的，畫面用來區分「載入中」與「沒有紀錄」。 */
  loaded = $state(false);

  constructor() {
    void this.#load();
  }

  async #load(): Promise<void> {
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
    this.activeId = record.id;
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
