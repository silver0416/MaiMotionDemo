/**
 * 本機資料庫：WebView 的 IndexedDB。
 * - records：譜面紀錄（原文、名稱、Majdata 來源）
 * - settings：顯示設定與參數草稿
 * - analyses：Rust 分析結果快取，依「原文＋參數＋核心版本」為鍵，切換紀錄時不必重跑
 * - evaluations：標註比對結果的暫存（2026-11-08 起），依「紀錄＋路線＋參數」為鍵、以 recordId 建索引；
 *   換模型時先顯示暫存，見 state/annotation.svelte.ts。放在另一個資料庫（maimotion-evaluations）：
 *   主資料庫的版本號不動，舊版 App 和新版同時開著、或改回舊版時，主資料庫照樣打得開。
 *
 * 所有函式在資料庫無法開啟時都不丟例外，讓應用退回只存在記憶體的模式。
 */

const DB_NAME = 'maimotion';
const DB_VERSION = 1;
const EVALUATION_DB_NAME = 'maimotion-evaluations';
const EVALUATION_DB_VERSION = 1;

export const STORE_RECORDS = 'records';
export const STORE_SETTINGS = 'settings';
export const STORE_ANALYSES = 'analyses';
export const STORE_EVALUATIONS = 'evaluations';

const openings = new Map<string, Promise<IDBDatabase | null>>();

function open(name: string, version: number, upgrade: (db: IDBDatabase) => void): Promise<IDBDatabase | null> {
  const cached = openings.get(name);
  if (cached) return cached;
  const opening = new Promise<IDBDatabase | null>((resolve) => {
    if (typeof indexedDB === 'undefined') {
      resolve(null);
      return;
    }
    let request: IDBOpenDBRequest;
    try {
      request = indexedDB.open(name, version);
    } catch {
      resolve(null);
      return;
    }
    request.onupgradeneeded = () => upgrade(request.result);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => resolve(null);
    request.onblocked = () => resolve(null);
  });
  openings.set(name, opening);
  return opening;
}

function openMain(): Promise<IDBDatabase | null> {
  return open(DB_NAME, DB_VERSION, (db) => {
    if (!db.objectStoreNames.contains(STORE_RECORDS)) {
      db.createObjectStore(STORE_RECORDS, { keyPath: 'id' });
    }
    if (!db.objectStoreNames.contains(STORE_SETTINGS)) {
      db.createObjectStore(STORE_SETTINGS);
    }
    if (!db.objectStoreNames.contains(STORE_ANALYSES)) {
      const store = db.createObjectStore(STORE_ANALYSES, { keyPath: 'key' });
      store.createIndex('sourceHash', 'sourceHash', { unique: false });
    }
  });
}

function openEvaluations(): Promise<IDBDatabase | null> {
  return open(EVALUATION_DB_NAME, EVALUATION_DB_VERSION, (db) => {
    if (!db.objectStoreNames.contains(STORE_EVALUATIONS)) {
      const store = db.createObjectStore(STORE_EVALUATIONS, { keyPath: 'key' });
      store.createIndex('recordId', 'recordId', { unique: false });
    }
  });
}

/** store 所在的資料庫。 */
function openDb(store: string): Promise<IDBDatabase | null> {
  return store === STORE_EVALUATIONS ? openEvaluations() : openMain();
}

function done<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function finished(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

export async function dbGetAll<T>(store: string): Promise<T[] | null> {
  const db = await openDb(store);
  if (!db) return null;
  try {
    return (await done(db.transaction(store, 'readonly').objectStore(store).getAll())) as T[];
  } catch {
    return null;
  }
}

export async function dbGet<T>(store: string, key: IDBValidKey): Promise<T | undefined> {
  const db = await openDb(store);
  if (!db) return undefined;
  try {
    return (await done(db.transaction(store, 'readonly').objectStore(store).get(key))) as T | undefined;
  } catch {
    return undefined;
  }
}

/** 寫入一筆；out-of-line key 的 store（settings）要給 key。 */
export async function dbPut(store: string, value: unknown, key?: IDBValidKey): Promise<boolean> {
  const db = await openDb(store);
  if (!db) return false;
  try {
    const tx = db.transaction(store, 'readwrite');
    tx.objectStore(store).put(value, key);
    await finished(tx);
    return true;
  } catch {
    return false;
  }
}

export async function dbDelete(store: string, key: IDBValidKey): Promise<void> {
  const db = await openDb(store);
  if (!db) return;
  try {
    const tx = db.transaction(store, 'readwrite');
    tx.objectStore(store).delete(key);
    await finished(tx);
  } catch {
    // 刪除失敗只會留下孤兒資料，不影響使用。
  }
}

/** 刪除某份原文的所有分析快取（不同參數各一筆）。 */
export async function dbDeleteAnalysesBySource(sourceHash: string): Promise<void> {
  const db = await openDb(STORE_ANALYSES);
  if (!db) return;
  try {
    const tx = db.transaction(STORE_ANALYSES, 'readwrite');
    const index = tx.objectStore(STORE_ANALYSES).index('sourceHash');
    const keys = await done(index.getAllKeys(IDBKeyRange.only(sourceHash)));
    const store = tx.objectStore(STORE_ANALYSES);
    for (const key of keys) store.delete(key);
    await finished(tx);
  } catch {
    // 同上。
  }
}

/** 依索引取出多筆（例如某筆紀錄的所有比對暫存）；資料庫不可用時回傳空陣列。 */
export async function dbGetAllByIndex<T>(store: string, index: string, value: IDBValidKey): Promise<T[]> {
  const db = await openDb(store);
  if (!db) return [];
  try {
    const request = db.transaction(store, 'readonly').objectStore(store).index(index).getAll(IDBKeyRange.only(value));
    return (await done(request)) as T[];
  } catch {
    return [];
  }
}

/** 一次刪除多個鍵。 */
export async function dbDeleteKeys(store: string, keys: IDBValidKey[]): Promise<void> {
  if (keys.length === 0) return;
  const db = await openDb(store);
  if (!db) return;
  try {
    const tx = db.transaction(store, 'readwrite');
    const target = tx.objectStore(store);
    for (const key of keys) target.delete(key);
    await finished(tx);
  } catch {
    // 刪除失敗只會留下孤兒資料，不影響使用。
  }
}

/** 依索引刪除多筆（例如刪除紀錄時一併刪掉它的比對暫存）。 */
export async function dbDeleteByIndex(store: string, index: string, value: IDBValidKey): Promise<void> {
  const db = await openDb(store);
  if (!db) return;
  try {
    const tx = db.transaction(store, 'readwrite');
    const target = tx.objectStore(store);
    const keys = await done(target.index(index).getAllKeys(IDBKeyRange.only(value)));
    for (const key of keys) target.delete(key);
    await finished(tx);
  } catch {
    // 同上。
  }
}

/** 某個 store 的筆數；資料庫不可用時回傳 null。 */
export async function dbCount(store: string): Promise<number | null> {
  const db = await openDb(store);
  if (!db) return null;
  try {
    return await done(db.transaction(store, 'readonly').objectStore(store).count());
  } catch {
    return null;
  }
}

/** 清空指定的 store（可跨兩個資料庫，各自一個交易）；全部成功才回傳 true。 */
export async function dbClear(...stores: string[]): Promise<boolean> {
  const groups = [stores.filter((store) => store !== STORE_EVALUATIONS), stores.filter((store) => store === STORE_EVALUATIONS)];
  let ok = true;
  for (const group of groups) {
    if (group.length === 0) continue;
    const db = await openDb(group[0]);
    if (!db) {
      ok = false;
      continue;
    }
    try {
      const tx = db.transaction(group, 'readwrite');
      for (const store of group) tx.objectStore(store).clear();
      await finished(tx);
    } catch {
      ok = false;
    }
  }
  return ok;
}

/** 鍵依字母排序、略過 undefined 的 JSON：同一組設定不論欄位順序都得到同一個快取鍵。 */
export function stableJson(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, item]) => item !== undefined)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return `{${entries.map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`).join(',')}}`;
}

/**
 * 同步的 53 位元雜湊（cyrb53），只用來判斷內容有沒有變（例如比對暫存之後標註改過沒有），
 * 不當作識別或安全用途。
 */
export function quickHash(text: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    h1 = Math.imul(h1 ^ code, 2654435761);
    h2 = Math.imul(h2 ^ code, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(36);
}

/** SHA-256 十六進位；不支援 WebCrypto 時退回簡單的 FNV-1a（只用於快取鍵與重複判定）。 */
export async function hashText(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text);
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    try {
      const digest = await crypto.subtle.digest('SHA-256', bytes);
      return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
    } catch {
      // 落到下面的後備算法。
    }
  }
  let hash = 0x811c9dc5;
  for (const byte of bytes) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `fnv-${hash.toString(16)}-${bytes.length}`;
}
