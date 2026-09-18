import { appDistribution, checkForUpdate, isDesktop, openExternalUrl } from '../lib/api';
import type { AppDistribution, UpdateInfo } from '../lib/types';

const STORAGE_KEY = 'maimotion.update-check.v1';
/** 自動檢查間隔：上次檢查超過 24 小時才在啟動時再查，避免每次開啟都打 GitHub API。 */
export const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
export const RELEASES_URL = 'https://github.com/silver0416/MaiMotionDemo/releases';

export const DISTRIBUTION_LABEL: Record<AppDistribution, string> = {
  portable: 'Portable',
  installed: 'Installer',
  dev: 'Dev',
  preview: 'Preview',
};

/** 版本檢查狀態：啟動時自動查一次（桌面版限定），設定頁可手動再查。 */
class UpdateState {
  distribution = $state<AppDistribution>('preview');
  checking = $state(false);
  result = $state<UpdateInfo | null>(null);
  error = $state<string | null>(null);
  lastChecked = $state<number | null>(null);
  #initialized = false;

  async init(): Promise<void> {
    if (this.#initialized) return;
    this.#initialized = true;
    this.lastChecked = readLastChecked();
    try {
      this.distribution = await appDistribution();
    } catch {
      this.distribution = isDesktop() ? 'portable' : 'preview';
    }
  }

  /** 上次自動檢查是否已超過間隔；沒查過也算要查。 */
  shouldAutoCheck(now: number = Date.now()): boolean {
    if (!isDesktop()) return false;
    if (this.distribution === 'dev' || this.distribution === 'preview') return false;
    if (this.lastChecked == null) return true;
    return now - this.lastChecked >= UPDATE_CHECK_INTERVAL_MS;
  }

  /**
   * 查一次 GitHub Releases。失敗時把錯誤字串放在 `error`，不拋出，
   * 呼叫端（啟動自動檢查）可選擇靜默忽略，設定頁則顯示錯誤。
   */
  async check(): Promise<UpdateInfo | null> {
    if (!isDesktop() || this.checking) return this.result;
    this.checking = true;
    this.error = null;
    try {
      const info = await checkForUpdate();
      this.result = info;
      this.lastChecked = Date.now();
      writeLastChecked(this.lastChecked);
      return info;
    } catch (error) {
      this.error = typeof error === 'string' ? error : String(error);
      return null;
    } finally {
      this.checking = false;
    }
  }

  /** 開啟新版下載頁；還沒檢查過就開 Releases 總覽。 */
  async openDownload(): Promise<void> {
    await openExternalUrl(this.result?.url ?? RELEASES_URL);
  }
}

function readLastChecked(): number | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (parsed && typeof parsed === 'object' && typeof (parsed as { at?: unknown }).at === 'number') {
      return (parsed as { at: number }).at;
    }
    return null;
  } catch {
    return null;
  }
}

function writeLastChecked(at: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ at }));
  } catch {
    // 存不到只影響下次啟動是否自動再查，不擋更新提示。
  }
}

export const updateState = new UpdateState();
