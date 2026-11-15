/**
 * 目前是在哪種環境看這個站：一般瀏覽器，還是從手機主畫面開啟的 web app（standalone）。
 * 外框（頂列、底部導覽列）與安裝提示都讀這一份，不要各自再判斷一次。
 *
 * - `app`：要套用哪一套 App 外框。只有 standalone（或用下面的預覽開關）時才不是 null。
 * - `os`：裝置的系統，和是不是 standalone 無關（安裝提示用它決定顯示哪一種引導）。
 * - `inApp`：是不是 App 內建瀏覽器（LINE、Facebook…），這種環境不能安裝，要引導改用系統瀏覽器。
 *
 * 預覽開關：網址加 `?app=ios` 或 `?app=android` 可以在一般瀏覽器（含電腦）看 App 外框，
 * `?app=off` 取消。選擇記在 sessionStorage，只影響這個分頁。沒有 iPhone 也能看 iOS 版的樣子。
 *
 * `src/app.html` 有一段行內腳本在第一次繪製前用同樣的規則設定 `<html data-app>`，避免外框閃一下；
 * 兩邊的規則要一致，改這裡時一起改那段腳本。
 */

export type AppOs = 'ios' | 'android' | 'other';
export type AppShell = 'ios' | 'android' | null;

/** 預覽開關存放的鍵（sessionStorage）。 */
export const APP_PREVIEW_KEY = 'maimotion.docs.app-preview';

export const platform = $state({
  /** 裝置的系統。 */
  os: 'other' as AppOs,
  /** 是不是從主畫面開啟（display-mode: standalone，或 iOS 的 navigator.standalone）。 */
  standalone: false,
  /** 要套用的 App 外框；null 表示一般網頁外框。 */
  app: null as AppShell,
  /** App 內建瀏覽器的名稱（'line'、'facebook'…）；不是內建瀏覽器時為 null。 */
  inApp: null as string | null,
  /** 是不是手機或平板（觸控為主的裝置）。 */
  mobile: false,
  /** 是否已經在瀏覽器端判斷過（預先算繪與 hydration 前為 false）。 */
  ready: false,
});

/** 判斷系統。iPadOS 會偽裝成 Mac，所以另外看觸控點數。 */
export function detectOs(ua: string, nav: Navigator): AppOs {
  if (/iPhone|iPad|iPod/.test(ua)) return 'ios';
  if (nav.platform === 'MacIntel' && nav.maxTouchPoints > 1) return 'ios';
  if (/Android/.test(ua)) return 'android';
  return 'other';
}

/** App 內建瀏覽器（用 WebView、不能把網站安裝成 web app 的那些）。 */
export function detectInApp(ua: string, win: Window): string | null {
  if (/\bLine\//i.test(ua)) return 'line';
  if (/\bFB[\w_]+\/|FBAN|FBAV|FB_IAB/.test(ua)) return /Messenger/.test(ua) ? 'messenger' : 'facebook';
  if (/\bInstagram/.test(ua)) return 'instagram';
  if (/\bBarcelona/.test(ua)) return 'threads';
  if (/\bTwitter/.test(ua)) return 'x';
  if (/MicroMessenger\//.test(ua)) return 'wechat';
  const w = win as unknown as Record<string, unknown>;
  if (w.TelegramWebview || w.TelegramWebviewProxy || w.TelegramWebviewProxyProto) return 'telegram';
  return null;
}

export function detectStandalone(win: Window): boolean {
  const nav = win.navigator as Navigator & { standalone?: boolean };
  return win.matchMedia('(display-mode: standalone)').matches || nav.standalone === true;
}

/** 讀取（並在網址帶 `?app=` 時更新）預覽開關。 */
export function readPreview(win: Window): AppShell | 'off' | null {
  let value: string | null = null;
  try {
    const param = new URLSearchParams(win.location.search).get('app');
    if (param === 'ios' || param === 'android') win.sessionStorage.setItem(APP_PREVIEW_KEY, param);
    else if (param === 'off') win.sessionStorage.removeItem(APP_PREVIEW_KEY);
    value = win.sessionStorage.getItem(APP_PREVIEW_KEY);
  } catch {
    // sessionStorage 不能用（隱私模式等）時就不支援預覽。
  }
  return value === 'ios' || value === 'android' ? value : null;
}

/**
 * 在瀏覽器端判斷一次並寫進 `platform`，同時把結果反映到 `<html data-app>`。
 * 由外框在 onMount 呼叫；回傳清除函式（停止監聽 display-mode 的變化）。
 */
export function initPlatform(win: Window = window): () => void {
  const ua = win.navigator.userAgent;
  const apply = () => {
    const os = detectOs(ua, win.navigator);
    const standalone = detectStandalone(win);
    const preview = readPreview(win);
    platform.os = os;
    platform.standalone = standalone;
    platform.inApp = detectInApp(ua, win);
    platform.mobile = os !== 'other' || win.matchMedia('(pointer: coarse) and (hover: none)').matches;
    // standalone 時依系統決定外框；電腦上安裝成 web app 不換外框。預覽開關優先。
    platform.app = preview === 'ios' || preview === 'android' ? preview : standalone && os !== 'other' ? os : null;
    platform.ready = true;
    const root = win.document.documentElement;
    if (platform.app) root.dataset.app = platform.app;
    else delete root.dataset.app;
    // 手機的系統（和 standalone 無關）；app.html 的行內腳本在第一次繪製前已經設過一次。
    if (os !== 'other') root.dataset.os = os;
    else delete root.dataset.os;
  };
  apply();
  const query = win.matchMedia('(display-mode: standalone)');
  query.addEventListener('change', apply);
  return () => query.removeEventListener('change', apply);
}
