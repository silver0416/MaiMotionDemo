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

/**
 * 影片歸屬：影片 id → 譜面紀錄 id。在哪份譜面開著時下載或選用的影片就歸那份譜面，
 * 一部影片可以同時屬於多份譜面。存在 localStorage，主視窗與影片視窗同一個來源共用。
 */
const OWNERS_KEY = 'maimotion.video-owners.v1';

function loadOwners(): Record<string, string[]> {
  try {
    const raw = JSON.parse(localStorage.getItem(OWNERS_KEY) ?? 'null') as unknown;
    if (!raw || typeof raw !== 'object') return {};
    const out: Record<string, string[]> = {};
    for (const [id, list] of Object.entries(raw as Record<string, unknown>)) {
      if (Array.isArray(list)) out[id] = list.filter((item): item is string => typeof item === 'string');
    }
    return out;
  } catch {
    return {};
  }
}

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
  owners = $state<Record<string, string[]>>(loadOwners());

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
    // 另一個視窗改了歸屬時跟著更新。
    const onStorage = (event: StorageEvent) => {
      if (event.key === OWNERS_KEY) this.owners = loadOwners();
    };
    window.addEventListener('storage', onStorage);
    this.#stops.push(() => window.removeEventListener('storage', onStorage));
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

  /** 改歸屬前先讀最新的（另一個視窗可能改過），再寫回。 */
  #updateOwners(change: (owners: Record<string, string[]>) => void): void {
    const next = loadOwners();
    change(next);
    this.owners = next;
    try {
      localStorage.setItem(OWNERS_KEY, JSON.stringify(next));
    } catch {
      // 存不了只影響下次開啟時的歸屬篩選。
    }
  }

  /** 這部影片屬於這份譜面嗎。 */
  ownedBy(videoId: string, recordId: string | null): boolean {
    return !!recordId && (this.owners[videoId]?.includes(recordId) ?? false);
  }

  /** 把影片歸到譜面（已歸屬就不動）。 */
  assign(videoId: string, recordId: string | null): void {
    if (!recordId || this.ownedBy(videoId, recordId)) return;
    this.#updateOwners((owners) => {
      if (!owners[videoId]?.includes(recordId)) owners[videoId] = [...(owners[videoId] ?? []), recordId];
    });
  }

  /** 從譜面移除影片的歸屬（影片檔不刪）。 */
  unassign(videoId: string, recordId: string | null): void {
    if (!recordId) return;
    this.#updateOwners((owners) => {
      const rest = (owners[videoId] ?? []).filter((item) => item !== recordId);
      if (rest.length > 0) owners[videoId] = rest;
      else delete owners[videoId];
    });
  }

  entry(id: string | undefined): VideoEntry | undefined {
    return id ? this.entries.find((item) => item.id === id) : undefined;
  }

  /**
   * 下載一部影片；失敗時錯誤記在 downloadErrors，回傳 null。
   * owner 是開始下載時開著的譜面：下載完成就歸那份譜面，中途切換譜面也不會歸錯。
   */
  async download(
    item: Pick<SearchItem, 'id'> & Partial<SearchItem>,
    owner: string | null = null,
  ): Promise<VideoEntry | null> {
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
      this.assign(entry.id, owner);
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
      this.#updateOwners((owners) => delete owners[id]);
      await this.refreshList();
      return null;
    } catch (error) {
      return message(error);
    }
  }

  async clear(): Promise<string | null> {
    try {
      await videoApi.clearCache();
      this.#updateOwners((owners) => {
        for (const id of Object.keys(owners)) delete owners[id];
      });
      await this.refreshList();
      return null;
    } catch (error) {
      return message(error);
    }
  }
}
