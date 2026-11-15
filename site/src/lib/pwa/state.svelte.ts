/**
 * Web App（PWA）的共用狀態：更新提示、安裝提示與「安裝成 App」入口都讀這一份。
 * 環境判斷（手機、standalone、App 內建瀏覽器）一律讀 $lib/app/platform.svelte，不在這裡另外判斷。
 */
import { platform } from '$lib/app/platform.svelte';

/** Chromium 的安裝事件（還不在 TypeScript 的 DOM 型別裡）。 */
export interface BeforeInstallPromptEvent extends Event {
  prompt(): Promise<void>;
  readonly userChoice: Promise<{ outcome: 'accepted' | 'dismissed'; platform: string }>;
}

/** 要顯示哪一種安裝引導。 */
export type InstallKind =
  /** Android／Chromium：瀏覽器給了安裝事件，直接叫出系統的安裝視窗。 */
  | 'native'
  /** Android 但瀏覽器（還）沒給安裝事件：教從瀏覽器選單安裝。 */
  | 'android-manual'
  /** iOS Safari：一步一步教「加入主畫面」。 */
  | 'ios'
  /** iOS 上的 Chrome、Edge、Firefox…：步驟改成從它們的分享選單。 */
  | 'ios-other'
  /** App 內建瀏覽器（LINE、Instagram…）：不能安裝，請改用瀏覽器開啟。 */
  | 'in-app'
  | 'none';

export const pwa = $state({
  /** App 模式下有新版 SW 在等待：顯示「有新版本」。 */
  updateReady: false,
  /** 攔下來的安裝事件（Android／Chromium）。 */
  deferred: null as BeforeInstallPromptEvent | null,
  /** 已經裝過（appinstalled 或 getInstalledRelatedApps 說有）。 */
  installed: false,
  /** 安裝提示卡是否顯示。 */
  promptOpen: false,
  /** iOS 的步驟是否展開。 */
  stepsOpen: false,
});

/** 目前環境可以給哪一種安裝引導（只在手機、還沒安裝時）。 */
export function installKind(): InstallKind {
  if (!platform.ready || !platform.mobile || platform.standalone || pwa.installed) return 'none';
  if (platform.inApp) return 'in-app';
  if (platform.os === 'ios') {
    return /CriOS|EdgiOS|FxiOS|OPiOS/.test(navigator.userAgent) ? 'ios-other' : 'ios';
  }
  if (pwa.deferred) return 'native';
  // Android 上事件可能晚一點才來，或瀏覽器不支援（Firefox 等）：先給手動步驟，事件來了自動換成直接安裝。
  return platform.os === 'android' ? 'android-manual' : 'none';
}
