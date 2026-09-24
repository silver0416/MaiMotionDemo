// 影片工具（yt-dlp、JS 執行環境）與已下載影片的狀態。影片視窗與設定視窗各自使用一份。

import { isDesktop } from '../lib/api';
import {
  listenEvent,
  videoApi,
  type CacheInfo,
  type DownloadProgress,
  type SearchItem,
  type ToolProgress,
  type ToolsStatus,
  type VideoEntry,
  type VideoTool,
} from '../lib/video';

function message(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : String(error);
}

export class VideoLibrary {
  readonly desktop = isDesktop();
  status = $state<ToolsStatus | null>(null);
  checking = $state(false);
  /** 正在下載的工具與進度 */
  installing = $state<Partial<Record<VideoTool, ToolProgress>>>({});
  toolError = $state<string | null>(null);
  entries = $state<VideoEntry[]>([]);
  cache = $state<CacheInfo | null>(null);
  /** 正在下載的影片與進度 */
  downloads = $state<Record<string, DownloadProgress>>({});
  downloadErrors = $state<Record<string, string>>({});

  #started = false;
  #stops: (() => void)[] = [];

  /** 開始監聽 Rust 事件並讀取現況；重複呼叫不會重複監聽。 */
  async init(): Promise<void> {
    if (!this.desktop || this.#started) return;
    this.#started = true;
    this.#stops.push(
      await listenEvent<DownloadProgress>('video-download-progress', (progress) => {
        if (this.downloads[progress.id]) this.downloads[progress.id] = progress;
      }),
      await listenEvent<ToolProgress>('video-tool-progress', (progress) => {
        if (this.installing[progress.tool]) this.installing[progress.tool] = progress;
      }),
      await listenEvent<null>('video-library-changed', () => void this.refreshList()),
    );
    await Promise.all([this.refreshStatus(false), this.refreshList()]);
  }

  dispose(): void {
    for (const stop of this.#stops) stop();
    this.#stops = [];
    this.#started = false;
  }

  async refreshStatus(force: boolean): Promise<void> {
    if (!this.desktop) return;
    this.checking = true;
    try {
      this.status = await videoApi.toolsStatus(force);
    } catch (error) {
      this.toolError = message(error);
    } finally {
      this.checking = false;
    }
  }

  async refreshList(): Promise<void> {
    if (!this.desktop) return;
    try {
      const [entries, cache] = await Promise.all([videoApi.list(), videoApi.cacheInfo()]);
      this.entries = entries;
      this.cache = cache;
    } catch {
      // 讀不到清單時維持原狀。
    }
  }

  /** 下載（或更新）工具；成功回傳 true。 */
  async install(tool: VideoTool): Promise<boolean> {
    if (this.installing[tool]) return false;
    this.installing[tool] = { tool, downloaded: 0, total: null };
    this.toolError = null;
    try {
      this.status = await videoApi.installTool(tool);
      return true;
    } catch (error) {
      this.toolError = message(error);
      return false;
    } finally {
      delete this.installing[tool];
    }
  }

  async removeTool(tool: VideoTool): Promise<void> {
    this.toolError = null;
    try {
      this.status = await videoApi.removeTool(tool);
    } catch (error) {
      this.toolError = message(error);
    }
  }

  entry(id: string | undefined): VideoEntry | undefined {
    return id ? this.entries.find((item) => item.id === id) : undefined;
  }

  /** 下載一部影片；失敗時錯誤記在 downloadErrors，回傳 null。 */
  async download(item: Pick<SearchItem, 'id'> & Partial<SearchItem>): Promise<VideoEntry | null> {
    const id = item.id;
    if (this.downloads[id]) return null;
    this.downloads[id] = { id, downloaded: null, total: null, speed: null, eta: null };
    delete this.downloadErrors[id];
    try {
      const entry = await videoApi.download(id, {
        title: item.title,
        channel: item.channel,
        duration: item.duration ?? null,
      });
      await this.refreshList();
      return entry;
    } catch (error) {
      const text = message(error);
      if (text !== '已取消下載') this.downloadErrors[id] = text;
      return null;
    } finally {
      delete this.downloads[id];
    }
  }

  async cancel(id: string): Promise<void> {
    await videoApi.cancel(id).catch(() => {});
  }

  async remove(id: string): Promise<string | null> {
    try {
      await videoApi.remove(id);
      await this.refreshList();
      return null;
    } catch (error) {
      return message(error);
    }
  }

  async clear(): Promise<string | null> {
    try {
      await videoApi.clearCache();
      await this.refreshList();
      return null;
    } catch (error) {
      return message(error);
    }
  }
}
