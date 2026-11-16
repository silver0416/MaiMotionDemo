/**
 * 安裝的動作。入口是首頁的「安裝手冊到手機」與選單抽屜的「加到主畫面」（都是 InstallEntry.svelte），
 * 點了之後的畫面在 InstallPrompt.svelte。
 *
 * 不再自動跳出提示（使用者 2026-11-15：改成手機首頁上原本「下載」的位置直接放安裝鈕）。
 */
import { installKind, pwa, type BeforeInstallPromptEvent } from './state.svelte';

const KEY_INSTALLED = 'maimotion.docs.pwa.installed';

function read(storage: 'local' | 'session', key: string): string | null {
  try {
    return (storage === 'local' ? localStorage : sessionStorage).getItem(key);
  } catch {
    return null;
  }
}

function write(storage: 'local' | 'session', key: string, value: string): void {
  try {
    (storage === 'local' ? localStorage : sessionStorage).setItem(key, value);
  } catch {
    // 隱私模式等存不了：這次照樣運作，只是下次不記得。
  }
}

// 安裝事件可能很早就來，模組一載入就開始聽（InstallPrompt 由外框掛在每一頁）。
if (typeof window !== 'undefined') {
  if (read('local', KEY_INSTALLED) === '1') pwa.installed = true;
  window.addEventListener('beforeinstallprompt', (event) => {
    // 先不讓瀏覽器自己跳出安裝列，等時機到了由我們的提示呼叫 prompt()。
    event.preventDefault();
    pwa.deferred = event as BeforeInstallPromptEvent;
  });
  window.addEventListener('appinstalled', () => {
    markInstalled();
  });
}

function markInstalled(): void {
  pwa.installed = true;
  pwa.deferred = null;
  pwa.promptOpen = false;
  write('local', KEY_INSTALLED, '1');
}

/** Android：問瀏覽器這個 web app 是不是已經裝了（manifest 的 related_applications 指向自己）。 */
export async function checkInstalled(): Promise<void> {
  const nav = navigator as Navigator & { getInstalledRelatedApps?: () => Promise<unknown[]> };
  if (!nav.getInstalledRelatedApps) return;
  try {
    const apps = await nav.getInstalledRelatedApps();
    if (apps.length > 0) markInstalled();
  } catch {
    // 不支援或被拒：照常判斷。
  }
}

/** 關掉安裝的提示卡。 */
export function dismissPrompt(): void {
  pwa.promptOpen = false;
  pwa.stepsOpen = false;
}

/** Android／Chromium：叫出系統的安裝視窗。 */
export async function runNativePrompt(): Promise<void> {
  const event = pwa.deferred;
  if (!event) return;
  // 同一個事件只能 prompt() 一次。
  pwa.deferred = null;
  try {
    await event.prompt();
    const { outcome } = await event.userChoice;
    if (outcome === 'accepted') markInstalled();
    else dismissPrompt();
  } catch {
    pwa.promptOpen = false;
  }
}

/** 安裝入口：Android 有安裝事件就直接叫出系統視窗，其餘打開對應的步驟或引導。 */
export function openInstall(): void {
  const kind = installKind();
  if (kind === 'none') return;
  if (kind === 'native') {
    void runNativePrompt();
    return;
  }
  pwa.promptOpen = true;
  pwa.stepsOpen = kind !== 'in-app';
}
