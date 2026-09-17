/**
 * 本機資料庫：WebView 的 IndexedDB。
 * - records：譜面紀錄（原文、名稱、Majdata 來源）
 * - settings：顯示設定與參數草稿
 * - analyses：Rust 分析結果快取，依「原文＋參數＋核心版本」為鍵，切換紀錄時不必重跑
 *
 * 所有函式在資料庫無法開啟時都不丟例外，讓應用退回只存在記憶體的模式。
 */

const DB_NAME = 'maimotion';
const DB_VERSION = 1;

export const STORE_RECORDS = 'records';
export const STORE_SETTINGS = 'settings';
export const STORE_ANALYSES = 'analyses';

let opening: Promise<IDBDatabase | null> | null = null;

function openDb(): Promise<IDBDatabase | null> {
  if (opening) return opening;
  opening = new Promise((resolve) => {
    if (typeof indexedDB === 'undefined') {
      resolve(null);
      return;
    }
    let request: IDBOpenDBRequest;
    try {
      request = indexedDB.open(DB_NAME, DB_VERSION);
    } catch {
      resolve(null);
      return;
    }
    request.onupgradeneeded = () => {
      const db = request.result;
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
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => resolve(null);
    request.onblocked = () => resolve(null);
  });
  return opening;
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
  const db = await openDb();
  if (!db) return null;
  try {
    return (await done(db.transaction(store, 'readonly').objectStore(store).getAll())) as T[];
  } catch {
    return null;
  }
}

export async function dbGet<T>(store: string, key: IDBValidKey): Promise<T | undefined> {
  const db = await openDb();
  if (!db) return undefined;
  try {
    return (await done(db.transaction(store, 'readonly').objectStore(store).get(key))) as T | undefined;
  } catch {
    return undefined;
  }
}

/** 寫入一筆；out-of-line key 的 store（settings）要給 key。 */
export async function dbPut(store: string, value: unknown, key?: IDBValidKey): Promise<boolean> {
  const db = await openDb();
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
  const db = await openDb();
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
  const db = await openDb();
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

/** 某個 store 的筆數；資料庫不可用時回傳 null。 */
export async function dbCount(store: string): Promise<number | null> {
  const db = await openDb();
  if (!db) return null;
  try {
    return await done(db.transaction(store, 'readonly').objectStore(store).count());
  } catch {
    return null;
  }
}

/** 清空指定的 store；回傳是否成功。 */
export async function dbClear(...stores: string[]): Promise<boolean> {
  const db = await openDb();
  if (!db) return false;
  try {
    const tx = db.transaction(stores, 'readwrite');
    for (const store of stores) tx.objectStore(store).clear();
    await finished(tx);
    return true;
  } catch {
    return false;
  }
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
