import {
  appDistribution,
  checkForUpdate,
  downloadUpdate,
  isDesktop,
  openExternalUrl,
  previousVersionPath,
  restartToUpdate,
  settlePreviousVersion,
} from '../lib/api';
import type { AppDistribution, DownloadedUpdate, UpdateInfo } from '../lib/types';
import { formatBytes, listenEvent } from '../lib/video';
import { toasts } from './toasts.svelte';

const STORAGE_KEY = 'maimotion.update-check.v1';
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
  /** 程式內更新：下載進度、下載好的檔案與錯誤。 */
  downloading = $state(false);
  progress = $state<{ downloaded: number; total: number } | null>(null);
  downloaded = $state<DownloadedUpdate | null>(null);
  downloadError = $state<string | null>(null);
  restarting = $state(false);
  #initialized = false;

  /** Portable 版而且這一版有可下載的執行檔，才能在程式內更新。 */
  get canSelfUpdate(): boolean {
    return this.distribution === 'portable' && !!this.result?.hasUpdate && !!this.result.asset;
  }

  get percent(): number {
    if (!this.progress || this.progress.total <= 0) return 0;
    return Math.min(100, Math.round((100 * this.progress.downloaded) / this.progress.total));
  }

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

  /** 每次開啟都自動檢查一次；開發與瀏覽器預覽版不查。 */
  shouldAutoCheck(): boolean {
    if (!isDesktop()) return false;
    return this.distribution !== 'dev' && this.distribution !== 'preview';
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

  /** 下載新版；進度與結果以右下角通知顯示。成功回傳 true。 */
  async download(): Promise<boolean> {
    const asset = this.result?.asset;
    if (!asset || this.downloading) return false;
    if (this.downloaded) {
      this.#announceReady();
      return true;
    }
    toasts.dismiss('update-available');
    this.downloading = true;
    this.downloadError = null;
    this.progress = { downloaded: 0, total: asset.size };
    const version = this.result?.latest ?? '';
    const show = () =>
      toasts.show({
        id: 'update-download',
        tone: 'busy',
        title: `正在下載 v${version}`,
        body: `${formatBytes(this.progress?.downloaded)} / ${formatBytes(this.progress?.total)}・${this.percent}%`,
        sticky: true,
        progress: this.percent,
      });
    show();
    let lastPercent = -1;
    const stop = await listenEvent<{ downloaded: number; total: number }>('update-download-progress', (progress) => {
      this.progress = progress;
      if (this.percent !== lastPercent) {
        lastPercent = this.percent;
        show();
      }
    });
    try {
      this.downloaded = await downloadUpdate($state.snapshot(asset));
      this.#announceReady();
      return true;
    } catch (error) {
      this.downloadError = typeof error === 'string' ? error : String(error);
      toasts.show({
        id: 'update-download',
        tone: 'error',
        title: '更新下載失敗',
        body: this.downloadError,
        sticky: true,
        action: { label: '到 GitHub 下載', icon: 'external-link', run: () => void this.openDownload() },
      });
      return false;
    } finally {
      stop();
      this.downloading = false;
    }
  }

  #announceReady(): void {
    const file = this.downloaded;
    if (!file) return;
    toasts.show({
      id: 'update-download',
      tone: 'ok',
      title: `v${this.result?.latest ?? ''} 已下載`,
      body: file.fallback
        ? `程式所在的資料夾不能寫入，已下載到「下載」資料夾（${file.name}）。重新啟動會開啟新版。`
        : '重新啟動就會換成新版，舊版可以在新版開啟後刪除。',
      sticky: true,
      action: { label: '重新啟動', icon: 'refresh-cw', run: () => void this.restart() },
    });
  }

  /** 開啟新版並關閉目前的程式。 */
  async restart(): Promise<void> {
    if (!this.downloaded || this.restarting) return;
    this.restarting = true;
    try {
      await restartToUpdate();
    } catch (error) {
      this.restarting = false;
      toasts.show({
        id: 'update-download',
        tone: 'error',
        title: '無法開啟新版',
        body: typeof error === 'string' ? error : String(error),
        sticky: true,
      });
    }
  }

  /** 透過程式內更新重新啟動後，自動刪除舊版執行檔。 */
  async removePrevious(): Promise<void> {
    if (!isDesktop()) return;
    const path = await previousVersionPath().catch(() => null);
    if (!path) return;
    const name = path.split(/[\\/]/).pop() ?? path;
    const version = await import('../lib/api').then(({ appVersion }) => appVersion());
    try {
      await settlePreviousVersion(true);
      toasts.show({ id: 'update-previous', tone: 'ok', title: `已更新到 v${version}`, body: `已移除舊版 ${name}。` });
    } catch (error) {
      toasts.show({
        id: 'update-previous',
        tone: 'warn',
        title: `已更新到 v${version}，但舊版沒有移除`,
        body: `${typeof error === 'string' ? error : String(error)}。可以之後自己刪除 ${path}。`,
        sticky: true,
      });
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
