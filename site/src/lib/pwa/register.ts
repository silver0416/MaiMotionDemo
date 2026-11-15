/**
 * 註冊 service worker（離線快取）與相關的更新流程。由外框在 onMount 呼叫一次（重複呼叫沒有作用）。
 *
 * - 只在手機註冊（platform.mobile）。桌面瀏覽器不註冊，已經有的註冊會取消，避免桌面讀者被快取卡在舊版。
 * - 已安裝成 App（standalone）：請 SW 在背景把整站抓進快取（完整離線），並申請持久儲存。
 *   一般手機瀏覽器只存讀過的頁面。
 * - 改版：新版 SW 裝好後先等待。App 裡顯示「有新版本」（UpdateNotice.svelte），點「重新整理」才換新並重新載入；
 *   一般瀏覽器不打擾，直接讓新版接手（不重新載入），之後的整頁載入自然是新版。
 * - App 回到前景、換頁時檢查更新（節流）。
 * - PWA_KILL_SWITCH=1 建置時：不註冊，並取消已有的註冊（SW 端也會自己清掉）。
 */
import { platform } from '$lib/app/platform.svelte';
import { CACHE_PREFIX, SW_URL, type PageToWorker, type WorkerToPage } from './protocol';
import { pwa } from './state.svelte';

declare const __PWA_KILL_SWITCH__: boolean;

/** 檢查更新的最短間隔。 */
const UPDATE_INTERVAL_MS = 60_000;

let started = false;
let registration: ServiceWorkerRegistration | null = null;
let waiting: ServiceWorker | null = null;
let lastCheck = 0;
let reloadRequested = false;
let loggedFull = false;

function post(worker: ServiceWorker | null | undefined, message: PageToWorker): void {
  worker?.postMessage(message);
}

/** platform 由外框在 onMount 判斷；外框呼叫順序不一定，等它好了再做。 */
function whenPlatformReady(run: () => void, tries = 0): void {
  if (platform.ready) run();
  else if (tries < 100) setTimeout(() => whenPlatformReady(run, tries + 1), 50);
}

/** 在這之前就載入的東西（SW 還沒接手，沒經過快取）：頁面本身與同源的資源。 */
function loadedUrls(): string[] {
  const urls = new Set<string>([location.href]);
  for (const entry of performance.getEntriesByType('resource')) {
    try {
      const url = new URL(entry.name);
      if (url.origin === location.origin) urls.add(url.href);
    } catch {
      // 忽略
    }
  }
  return [...urls];
}

/** 取消註冊並刪掉這個站的快取（桌面、緊急開關）。沒有註冊過時什麼都不做。 */
async function unregisterAll(): Promise<void> {
  try {
    const registrations = await navigator.serviceWorker.getRegistrations();
    await Promise.all(registrations.map((r) => r.unregister()));
    const names = (await caches.keys()).filter((name) => name.startsWith(CACHE_PREFIX));
    await Promise.all(names.map((name) => caches.delete(name)));
  } catch {
    // 忽略
  }
}

function onWaiting(worker: ServiceWorker): void {
  // 第一次安裝（頁面還沒有 SW 控制）不是改版，SW 會自己啟用。
  if (!navigator.serviceWorker.controller) return;
  waiting = worker;
  if (platform.standalone) pwa.updateReady = true;
  else post(worker, { type: 'skip-waiting' });
}

function track(reg: ServiceWorkerRegistration): void {
  if (reg.waiting) onWaiting(reg.waiting);
  reg.addEventListener('updatefound', () => {
    const worker = reg.installing;
    worker?.addEventListener('statechange', () => {
      if (worker.state === 'installed') onWaiting(worker);
    });
  });
}

async function start(): Promise<void> {
  const container = navigator.serviceWorker;
  const controlledAtLoad = Boolean(container.controller);

  container.addEventListener('controllerchange', () => {
    // 只有使用者點了「重新整理」才重新載入；第一次安裝時的接手、一般瀏覽器的換新都不打擾。
    if (reloadRequested) {
      location.reload();
      return;
    }
    // 新版接手了：請它馬上把改版前讀過的頁面重新抓好（App 裡則是重新完整預抓）。
    // 接手（controllerchange）比 activate 做完還早，要等它變成 activated 才看得到待重抓的清單。
    const worker = container.controller;
    if (!worker) return;
    const sync = () => post(worker, { type: 'sync', urls: [], full: platform.standalone });
    if (worker.state === 'activated') sync();
    else {
      worker.addEventListener('statechange', function onState() {
        if (worker.state !== 'activated') return;
        worker.removeEventListener('statechange', onState);
        sync();
      });
    }
  });
  container.addEventListener('message', (event: MessageEvent<WorkerToPage>) => {
    const data = event.data;
    if (data?.type === 'full-offline' && !data.cached && !loggedFull) {
      loggedFull = true;
      // 這一版第一次做完時留一行紀錄方便實機除錯（檔案數與大小），不顯示在畫面上。
      console.info(
        `[pwa] 完整離線${data.ok ? '' : '（未完成，下次再補）'}：${data.files} 個檔，${Math.round(data.bytes / 1024)} kB，這次抓了 ${data.fetched} 個`,
      );
    }
  });

  registration = await container.register(SW_URL, { type: 'module', scope: '/', updateViaCache: 'none' });
  track(registration);

  const ready = await container.ready;
  post(ready.active, { type: 'sync', urls: controlledAtLoad ? [] : loadedUrls(), full: platform.standalone });

  if (platform.standalone) {
    // 已安裝：請瀏覽器不要在空間不足時自動清掉離線資料（瀏覽器可能拒絕，沒關係）。
    void navigator.storage?.persist?.().catch(() => false);
  }

  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') checkForUpdate();
  });
}

export function registerPwa(): void {
  if (started || typeof window === 'undefined' || !('serviceWorker' in navigator)) return;
  started = true;
  if (__PWA_KILL_SWITCH__) {
    void unregisterAll();
    return;
  }
  whenPlatformReady(() => {
    if (!platform.mobile) {
      void unregisterAll();
      return;
    }
    // 等頁面載完再註冊，不和第一次載入搶頻寬。
    const go = () => void start().catch((error) => console.warn('[pwa] service worker 註冊失敗', error));
    if (document.readyState === 'complete') go();
    else window.addEventListener('load', go, { once: true });
  });
}

/** 檢查有沒有新版（App 回到前景、換頁時呼叫；一分鐘內最多一次）。 */
export function checkForUpdate(): void {
  if (!registration || Date.now() - lastCheck < UPDATE_INTERVAL_MS) return;
  lastCheck = Date.now();
  void registration.update().catch(() => undefined);
}

/** 「有新版本」的「重新整理」：讓新版接手，接手後重新載入。 */
export function applyUpdate(): void {
  const worker = waiting ?? registration?.waiting ?? null;
  pwa.updateReady = false;
  if (!worker) {
    location.reload();
    return;
  }
  reloadRequested = true;
  post(worker, { type: 'skip-waiting' });
}
