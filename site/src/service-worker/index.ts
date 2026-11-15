/**
 * 文件站的 service worker（離線快取）。只在手機註冊（見 src/lib/pwa/register.ts），SvelteKit 只在正式建置時打包它。
 *
 * 快取規則：
 * - `/_app/immutable/*`（檔名帶雜湊的 JS、CSS、字型、演示資料）：cache-first。
 * - HTML 頁面（導覽請求）：network-first，3 秒沒回應就先給快取（網路回來後在背景更新快取）；
 *   有 navigation preload 就用它。離線又沒快取的頁面回 /offline（static/offline.html）。
 * - `/search/chunks.json`：stale-while-revalidate（離線也能字面搜尋）。
 * - static/ 底下的其他檔（圖示、manifest…）：network-first，失敗才用快取。
 * - `/api/*`、`/search/vectors.bin`（只有 Worker 用）、跨網域、非 GET、Range 請求：完全不攔截。
 *
 * 兩種模式：一般手機瀏覽器只存「讀過的」；安裝成 App（standalone）時頁面送 `sync { full: true }`，
 * SW 在背景把全部預先算繪的頁面與站內靜態檔抓進快取（同一版只做一次）。
 *
 * 快取名稱帶 `version`（每次建置不同），activate 時刪掉舊版；改版前讀過的頁面記下來，下次 `sync` 時重新抓。
 *
 * 緊急開關：建置時設環境變數 PWA_KILL_SWITCH=1，這支 SW 會清空所有快取、取消註冊並重新載入開著的頁面
 * （頁面端也不再註冊）。用法見 site/README.md 的「Web App（PWA）」。
 */

import { version } from '$app/env';
import { assets, immutable, prerendered } from '$app/manifest';
import { self } from '$app/service-worker';
import { CACHE_PREFIX, type PageToWorker, type WorkerToPage } from '../lib/pwa/protocol';

declare const __PWA_KILL_SWITCH__: boolean;

const PREFIX = CACHE_PREFIX;
const STATIC = `${PREFIX}static-${version}`;
const PAGES = `${PREFIX}pages-${version}`;
const DATA = `${PREFIX}data-${version}`;
const OWN = new Set([STATIC, PAGES, DATA]);

const OFFLINE_PAGE = '/offline';
const SEARCH_INDEX = '/search/chunks.json';
const VECTORS = '/search/vectors.bin';
/** 完整離線預抓做完的記號（存在 STATIC 快取，內容是統計）。 */
const FULL_MARK = '/__pwa/full-offline';
/** 改版前讀過、還沒重新抓的頁面清單（存在 PAGES 快取）。 */
const PENDING_REFILL = '/__pwa/pending-refill';
const NAV_TIMEOUT_MS = 3000;
const CONCURRENCY = 6;
/**
 * 快取比對一律忽略 Vary 與查詢字串：Workers 靜態檔回 `Vary: Origin`，而 module script 的請求帶 Origin、
 * 預抓（SW 自己的 fetch）不帶，不忽略的話預抓的檔永遠對不到。內容只由路徑決定，忽略是安全的。
 */
const MATCH: CacheQueryOptions = { ignoreSearch: true, ignoreVary: true };

const path = (p: string) => '/' + p.replace(/^\/+/, '');

/** static/ 底下的檔案（不含只有 Worker 用的向量檔）。 */
const STATIC_FILES = new Set(assets.map((a) => path(a.path)).filter((p) => p !== VECTORS));

/** 完整離線預抓的清單：所有雜湊檔（.woff 是給舊瀏覽器的備援，現代瀏覽器只用 .woff2，跳過）＋靜態檔＋所有預先算繪的頁面。 */
function fullList(): { statics: string[]; pages: string[] } {
  const statics = [
    ...immutable.map((f) => path(f.path)).filter((p) => !p.endsWith('.woff')),
    ...[...STATIC_FILES].filter((p) => p !== SEARCH_INDEX && !p.endsWith('.html')),
  ];
  // prerendered 也含端點（sitemap.xml）；只要沒有副檔名的頁面。
  const pages = prerendered.map((p) => path(p.path)).filter((p) => !/\.[a-z0-9]+$/i.test(p));
  if (!pages.includes('/')) pages.unshift('/');
  return { statics, pages };
}

/** 頁面的快取鍵：同一頁不同寫法（尾斜線、.html、查詢字串）都對到同一筆。 */
function pageKey(input: string | URL): string {
  const url = new URL(input, self.location.origin);
  let p = url.pathname.replace(/\.html$/, '').replace(/\/index$/, '/');
  if (p.length > 1) p = p.replace(/\/+$/, '');
  return url.origin + p;
}

/** 經過轉址的回應不能拿來回導覽請求；複製成乾淨的回應再存。 */
async function clean(response: Response): Promise<Response> {
  if (!response.redirected) return response;
  return new Response(await response.blob(), {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
  });
}

function storable(response: Response): boolean {
  return response.ok && response.type === 'basic';
}

function isHtml(response: Response): boolean {
  return (response.headers.get('Content-Type') ?? '').includes('text/html');
}

function sleep(ms: number): Promise<'timeout'> {
  return new Promise((resolve) => setTimeout(() => resolve('timeout'), ms));
}

async function offlinePage(): Promise<Response> {
  const cached = await caches.match(OFFLINE_PAGE, MATCH);
  if (cached) return cached;
  return new Response('<!doctype html><meta charset="utf-8"><title>目前離線</title><p>目前離線。</p>', {
    status: 503,
    headers: { 'Content-Type': 'text/html; charset=utf-8' },
  });
}

/** 抓一個檔存進快取（已經有就跳過）。回傳實際抓到的大小；失敗丟例外。 */
async function store(cacheName: string, key: string, skipIfCached = true): Promise<number> {
  const cache = await caches.open(cacheName);
  if (skipIfCached && (await cache.match(key, MATCH))) return 0;
  const response = await fetch(key, { credentials: 'same-origin' });
  if (!storable(response)) throw new Error(`${key}: HTTP ${response.status}`);
  const body = await clean(response);
  // 解壓後的大小（Content-Length 可能是壓縮後的，或根本沒有）。
  const size = (await body.clone().blob()).size;
  await cache.put(key, body);
  return size;
}

/** 從頁面 HTML 找出它用到的雜湊檔（modulepreload、stylesheet、import()），相對路徑以頁面網址解析。 */
function referencedAssets(html: string, pageUrl: string): string[] {
  const found = new Set<string>();
  for (const match of html.matchAll(/(?:href|src)="([^"]+)"|import\("([^"]+)"\)/g)) {
    const raw = match[1] ?? match[2];
    if (!raw || !raw.includes('_app/immutable/')) continue;
    const url = new URL(raw, pageUrl);
    if (url.origin === self.location.origin) found.add(url.pathname);
  }
  return [...found];
}

/** 抓一頁存進快取，連同它用到的雜湊檔（離線打開時樣式與互動才完整）。 */
async function cachePage(target: string): Promise<number> {
  const key = pageKey(target);
  const response = await fetch(key, { credentials: 'same-origin' });
  if (!storable(response) || !isHtml(response)) throw new Error(`${key}: HTTP ${response.status}`);
  const body = await clean(response);
  const html = await body.clone().text();
  await (await caches.open(PAGES)).put(key, body);
  let bytes = html.length;
  for (const asset of referencedAssets(html, key)) {
    bytes += await store(STATIC, asset).catch(() => 0);
  }
  return bytes;
}

/** 依序處理清單，同時最多 CONCURRENCY 個。 */
async function pool<T>(items: T[], run: (item: T) => Promise<void>): Promise<void> {
  let next = 0;
  const workers = Array.from({ length: Math.min(CONCURRENCY, items.length) }, async () => {
    while (next < items.length) {
      const item = items[next];
      next += 1;
      await run(item);
    }
  });
  await Promise.all(workers);
}

let fullRun: Promise<WorkerToPage> | null = null;

/** 完整離線：把清單上的每個檔都抓進快取。全部成功才記下記號；失敗的下次 sync 再補。 */
function fullOffline(): Promise<WorkerToPage> {
  fullRun ??= (async (): Promise<WorkerToPage> => {
    const staticCache = await caches.open(STATIC);
    const mark = await staticCache.match(FULL_MARK);
    if (mark) {
      const stats = (await mark.json()) as { files: number; bytes: number };
      return { type: 'full-offline', ok: true, fetched: 0, files: stats.files, bytes: stats.bytes, cached: true };
    }
    const { statics, pages } = fullList();
    const pageCache = await caches.open(PAGES);
    let fetched = 0;
    let failed = 0;
    let bytes = 0;
    const sizeOf = async (cache: Cache, key: string) => {
      const hit = await cache.match(key, MATCH);
      return hit ? (await hit.blob()).size : 0;
    };
    await pool(statics, async (key) => {
      try {
        // 先算好再加：`bytes += await …` 會在等待前就讀了 bytes，同時進行的幾個會互相蓋掉。
        const size = await store(STATIC, key);
        if (size) fetched += 1;
        const counted = size || (await sizeOf(staticCache, key));
        bytes += counted;
      } catch {
        failed += 1;
      }
    });
    try {
      // 搜尋索引放在 DATA 快取（stale-while-revalidate）。
      const counted = (await store(DATA, SEARCH_INDEX)) || (await sizeOf(await caches.open(DATA), SEARCH_INDEX));
      bytes += counted;
    } catch {
      failed += 1;
    }
    await pool(pages, async (page) => {
      try {
        const key = pageKey(page);
        const hit = await pageCache.match(key, MATCH);
        if (hit) {
          const counted = (await hit.blob()).size;
          bytes += counted;
          return;
        }
        // 頁面用到的雜湊檔上面都抓過了；這裡只要 HTML。
        const response = await fetch(key, { credentials: 'same-origin' });
        if (!storable(response) || !isHtml(response)) throw new Error(`${key}: HTTP ${response.status}`);
        const body = await clean(response);
        const counted = (await body.clone().blob()).size;
        bytes += counted;
        await pageCache.put(key, body);
        fetched += 1;
      } catch {
        failed += 1;
      }
    });
    const files = statics.length + pages.length + 1;
    if (failed === 0) {
      await staticCache.put(
        FULL_MARK,
        new Response(JSON.stringify({ files, bytes, at: Date.now() }), {
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    }
    return { type: 'full-offline', ok: failed === 0, fetched, files, bytes, cached: false };
  })().finally(() => {
    fullRun = null;
  });
  return fullRun;
}

/** 改版前讀過的頁面：重新抓（新版的 HTML 與它用到的新雜湊檔）。 */
async function refill(): Promise<void> {
  const cache = await caches.open(PAGES);
  const pending = await cache.match(PENDING_REFILL);
  if (!pending) return;
  await cache.delete(PENDING_REFILL);
  const urls = (await pending.json()) as string[];
  await pool(urls, async (url) => {
    await cachePage(url).catch(() => undefined);
  });
}

/** 第一次安裝時，SW 接手前就載入的東西：頁面本身與它已經用到的雜湊檔、靜態檔。 */
async function warm(urls: string[]): Promise<void> {
  await pool(urls, async (raw) => {
    let url: URL;
    try {
      url = new URL(raw, self.location.origin);
    } catch {
      return;
    }
    if (url.origin !== self.location.origin) return;
    const p = url.pathname;
    try {
      if (p.startsWith('/_app/immutable/')) await store(STATIC, p);
      else if (p === SEARCH_INDEX) await store(DATA, p);
      else if (STATIC_FILES.has(p)) await store(STATIC, p);
      else if (!p.startsWith('/api/') && !p.startsWith('/_app/') && !/\.[a-z0-9]+$/i.test(p)) await cachePage(url.href);
    } catch {
      // 抓不到就算了，下次讀到時會再存。
    }
  });
}

// ---------------------------------------------------------------- 生命週期

self.addEventListener('install', (event) => {
  if (__PWA_KILL_SWITCH__) {
    void self.skipWaiting();
    return;
  }
  // 只預先存離線頁；其他（一般瀏覽器）讀到才存，或（App）由 sync 觸發完整預抓。
  // 不呼叫 skipWaiting：改版時由頁面決定何時換新（App 裡要使用者點「重新整理」）。
  event.waitUntil(store(STATIC, OFFLINE_PAGE, false));
});

self.addEventListener('activate', (event) => {
  const work = (async () => {
    const names = (await caches.keys()).filter((name) => name.startsWith(PREFIX));
    if (__PWA_KILL_SWITCH__) {
      await Promise.all(names.map((name) => caches.delete(name)));
      await self.registration.unregister();
      const windows = await self.clients.matchAll({ type: 'window' });
      await Promise.all(windows.map((client) => client.navigate(client.url).catch(() => null)));
      return;
    }
    // 舊版讀過的頁面記下來（新版的頁面引用新的雜湊檔，舊的快取不能直接沿用），下次 sync 時重新抓。
    const old = names.filter((name) => !OWN.has(name));
    const readBefore = new Set<string>();
    for (const name of old.filter((n) => n.startsWith(`${PREFIX}pages-`))) {
      const cache = await caches.open(name);
      for (const request of await cache.keys()) {
        if (!new URL(request.url).pathname.startsWith('/__pwa/')) readBefore.add(pageKey(request.url));
      }
      // 上一版還沒來得及重抓的也帶過來。
      const pending = await cache.match(PENDING_REFILL);
      if (pending) for (const url of (await pending.json().catch(() => [])) as string[]) readBefore.add(url);
    }
    if (readBefore.size > 0) {
      await (await caches.open(PAGES)).put(
        PENDING_REFILL,
        new Response(JSON.stringify([...readBefore]), { headers: { 'Content-Type': 'application/json' } }),
      );
    }
    await Promise.all(old.map((name) => caches.delete(name)));
    await self.registration.navigationPreload?.enable().catch(() => undefined);
    // 第一次安裝時接手目前開著的頁面（之後載入的東西才會進快取）；改版時頁面本來就由舊版控制，這裡換成新版。
    await self.clients.claim();
  })();
  event.waitUntil(work);
});

self.addEventListener('message', (event) => {
  const data = event.data as PageToWorker | null;
  if (!data || typeof data !== 'object') return;
  if (data.type === 'skip-waiting') {
    void self.skipWaiting();
    return;
  }
  if (data.type === 'sync' && !__PWA_KILL_SWITCH__) {
    const source = event.source;
    event.waitUntil(
      (async () => {
        await refill();
        if (Array.isArray(data.urls) && data.urls.length > 0) await warm(data.urls.slice(0, 200));
        if (data.full) {
          const result = await fullOffline();
          source?.postMessage(result satisfies WorkerToPage);
        }
      })(),
    );
  }
});

// ---------------------------------------------------------------- 請求

async function navigate(event: FetchEvent): Promise<Response> {
  const key = pageKey(event.request.url);
  const cache = await caches.open(PAGES);
  const network = (async () => {
    // navigation preload：SW 啟動的同時瀏覽器已經先發出請求；有就用它，沒有才自己抓。
    const preloaded = (await event.preloadResponse) as Response | undefined;
    const response = preloaded ?? (await fetch(event.request));
    if (storable(response) && isHtml(response)) {
      await cache.put(key, await clean(response.clone()));
    }
    return response;
  })();
  // 逾時先給了快取時，網路的回應仍在背景寫進快取。
  event.waitUntil(network.then(() => undefined, () => undefined));

  try {
    const first = await Promise.race([network, sleep(NAV_TIMEOUT_MS)]);
    if (first !== 'timeout') {
      if (first.status >= 500) return (await cache.match(key, MATCH)) ?? first;
      return first;
    }
    return (await cache.match(key, MATCH)) ?? (await network);
  } catch {
    return (await cache.match(key, MATCH)) ?? (await offlinePage());
  }
}

async function cacheFirst(request: Request): Promise<Response> {
  const cache = await caches.open(STATIC);
  const hit = await cache.match(request, MATCH);
  if (hit) return hit;
  const response = await fetch(request);
  if (storable(response)) await cache.put(request, response.clone());
  return response;
}

async function staleWhileRevalidate(event: FetchEvent, cacheName: string): Promise<Response> {
  const cache = await caches.open(cacheName);
  const key = new URL(event.request.url).pathname;
  const hit = await cache.match(key, MATCH);
  const update = fetch(event.request).then(async (response) => {
    if (storable(response)) await cache.put(key, response.clone());
    return response;
  });
  if (hit) {
    event.waitUntil(update.then(() => undefined, () => undefined));
    return hit;
  }
  return update;
}

async function networkFirst(request: Request, cacheName: string): Promise<Response> {
  const cache = await caches.open(cacheName);
  const key = new URL(request.url).pathname;
  try {
    const response = await fetch(request);
    if (storable(response)) await cache.put(key, await clean(response.clone()));
    return response;
  } catch (error) {
    const hit = await cache.match(key, MATCH);
    if (hit) return hit;
    throw error;
  }
}

self.addEventListener('fetch', (event) => {
  if (__PWA_KILL_SWITCH__) return;
  const { request } = event;
  if (request.method !== 'GET' || request.headers.has('range')) return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  const p = url.pathname;
  if (p.startsWith('/api/') || p === VECTORS || p === '/service-worker.js') return;

  if (request.mode === 'navigate') {
    event.respondWith(navigate(event));
  } else if (p.startsWith('/_app/immutable/')) {
    event.respondWith(cacheFirst(request));
  } else if (p === SEARCH_INDEX) {
    event.respondWith(staleWhileRevalidate(event, DATA));
  } else if (STATIC_FILES.has(p)) {
    event.respondWith(networkFirst(request, STATIC));
  }
  // 其他（/_app/version.json 等）不攔截，照一般網路請求。
});
