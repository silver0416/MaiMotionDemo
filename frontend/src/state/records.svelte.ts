export interface ChartRecord {
  id: string;
  /** 新增時間（epoch 毫秒），初版直接當作標題。 */
  createdAt: number;
  source: string;
}

const STORAGE_KEY = 'maimotion.records.v1';

function isRecord(value: unknown): value is ChartRecord {
  if (typeof value !== 'object' || value === null) return false;
  const item = value as Record<string, unknown>;
  return (
    typeof item.id === 'string' &&
    typeof item.createdAt === 'number' &&
    typeof item.source === 'string'
  );
}

function load(): ChartRecord[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
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

export function recordTitle(record: ChartRecord): string {
  const date = new Date(record.createdAt);
  return (
    `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
  );
}

/** 副標：maidata.txt 取 &title，否則取第一個非空白行。 */
export function recordPreview(record: ChartRecord): string {
  const title = /^&title=(.*)$/m.exec(record.source)?.[1]?.trim();
  if (title) return title;
  const line = record.source.split('\n').find((item) => item.trim().length > 0);
  return line?.trim() ?? '（空白）';
}

/**
 * 譜面紀錄：只保存原文，存在 WebView 的 localStorage。
 * 分析結果不保存，點選時以目前參數重新交給 Rust 生成。
 */
export class Records {
  /** 新的在前。 */
  items = $state<ChartRecord[]>(load());
  activeId = $state<string | null>(null);

  add(source: string): ChartRecord {
    const record: ChartRecord = {
      id: `rec-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
      createdAt: Date.now(),
      source,
    };
    this.items = [record, ...this.items];
    this.activeId = record.id;
    this.#save();
    return record;
  }

  remove(id: string): void {
    this.items = this.items.filter((item) => item.id !== id);
    if (this.activeId === id) this.activeId = null;
    this.#save();
  }

  #save(): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.items));
    } catch {
      // 儲存空間不可用時只保留在記憶體，本次執行期間仍可使用。
    }
  }
}

export const records = new Records();
