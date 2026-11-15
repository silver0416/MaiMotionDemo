/**
 * 頁面和 service worker（src/service-worker/index.ts）之間的訊息。兩邊都 import 這個檔，不要各寫各的字串。
 *
 * - `sync`：每次整頁載入、SW 就緒後由頁面送出。`urls` 是 SW 還沒接手前就載入的東西（第一次安裝時），
 *   讓 SW 補存；`full` 為 true（已安裝成 App）時 SW 在背景把整站抓進快取（同一版只做一次）。
 *   SW 也趁這時把「改版前讀過的頁面」重新抓一份（見 activate）。
 * - `skip-waiting`：新版 SW 裝好在等待時，頁面要它立刻接手（App 裡是使用者點了「重新整理」）。
 * - `full-offline`：SW 回報完整離線預抓的結果。
 */
export type PageToWorker =
  | { type: 'sync'; urls: string[]; full: boolean }
  | { type: 'skip-waiting' };

export type WorkerToPage = {
  type: 'full-offline';
  ok: boolean;
  /** 這次實際抓的檔案數（已經在快取裡的不算）。 */
  fetched: number;
  /** 預抓清單的總檔案數與總大小（bytes，已在快取裡的也算）。 */
  files: number;
  bytes: number;
  /** 是否早就做過（這一版）。 */
  cached: boolean;
};

/** SW 的網址（SvelteKit 輸出在 build/service-worker.js）。 */
export const SW_URL = '/service-worker.js';

/** 這個站的快取名稱開頭（後面接種類與建置的 version）。 */
export const CACHE_PREFIX = 'mm-docs-';
