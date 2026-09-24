// 影片同步比對：型別、Rust 指令包裝、主視窗與影片視窗之間的訊息通道。
// 時間換算：影片時間 = 譜面時間 + offset。

import { isDesktop } from './api';

/** 標註草稿記錄的影片連結。 */
export interface VideoLink {
  kind: 'youtube' | 'local';
  /** YouTube 影片 ID */
  id?: string;
  /** 本機影片的完整路徑（只存在自己的紀錄，不會匯出） */
  path?: string;
  title: string;
  channel?: string;
  /** 影片時間 − 譜面時間（秒）；null 表示還沒對齊 */
  offset: number | null;
  /** 左右翻轉顯示 */
  mirror?: boolean;
  /** 影片每秒格數，逐格前進用 */
  fps?: number;
}

/** 標註檔裡的影片資訊：只有 YouTube 影片會匯出。 */
export interface AnnotationVideo {
  url: string;
  title: string;
  channel?: string;
  offset: number | null;
  mirror?: boolean;
}

export interface ToolInfo {
  path: string;
  version: string;
  managed: boolean;
}

export interface RuntimeInfo extends ToolInfo {
  kind: 'deno' | 'node' | string;
}

export interface ToolsStatus {
  ytDlp: ToolInfo | null;
  runtime: RuntimeInfo | null;
  installable: boolean;
}

export type VideoTool = 'yt-dlp' | 'deno';

export interface SearchItem {
  id: string;
  title: string;
  channel: string;
  duration: number | null;
  views: number | null;
  downloaded: boolean;
}

export interface VideoEntry {
  id: string;
  title: string;
  channel: string;
  duration: number | null;
  fps: number | null;
  width: number | null;
  height: number | null;
  file: string;
  path: string;
  bytes: number;
  downloadedAt: number;
}

export interface DownloadProgress {
  id: string;
  downloaded: number | null;
  total: number | null;
  speed: number | null;
  eta: number | null;
}

export interface ToolProgress {
  tool: VideoTool;
  downloaded: number;
  total: number | null;
}

export interface CacheInfo {
  files: number;
  bytes: number;
}

export interface LocalVideo {
  path: string;
  name: string;
  bytes: number;
}

// ---------------------------------------------------------------------------
// 連結與網址

const ID_PATTERN = /^[A-Za-z0-9_-]{11}$/;

export function youtubeUrl(id: string): string {
  return `https://www.youtube.com/watch?v=${id}`;
}

export function thumbnailUrl(id: string): string {
  return `https://i.ytimg.com/vi/${id}/mqdefault.jpg`;
}

/** 從 YouTube 網址或 ID 取出影片 ID；認不出來回傳 null。 */
export function parseYoutubeId(input: string): string | null {
  const text = input.trim();
  if (ID_PATTERN.test(text)) return text;
  try {
    const url = new URL(text);
    const host = url.hostname.replace(/^www\.|^m\./, '');
    let id: string | null = null;
    if (host === 'youtu.be') id = url.pathname.slice(1).split('/')[0];
    else if (host === 'youtube.com' || host === 'music.youtube.com') {
      id = url.searchParams.get('v');
      if (!id) {
        const match = /^\/(?:shorts|live|embed)\/([^/]+)/.exec(url.pathname);
        id = match?.[1] ?? null;
      }
    }
    return id && ID_PATTERN.test(id) ? id : null;
  } catch {
    return null;
  }
}

function finite(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

/** 讀回資料庫或標註檔裡的連結；格式不對回傳 undefined。 */
export function cleanLink(value: unknown): VideoLink | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const raw = value as Record<string, unknown>;
  const offset = finite(raw.offset) ? raw.offset : null;
  const base = {
    title: typeof raw.title === 'string' ? raw.title : '',
    offset,
    ...(typeof raw.channel === 'string' && raw.channel ? { channel: raw.channel } : {}),
    ...(raw.mirror === true ? { mirror: true } : {}),
    ...(finite(raw.fps) && raw.fps > 0 ? { fps: raw.fps } : {}),
  };
  if (raw.kind === 'youtube' && typeof raw.id === 'string' && ID_PATTERN.test(raw.id)) {
    return { kind: 'youtube', id: raw.id, ...base };
  }
  if (raw.kind === 'local' && typeof raw.path === 'string' && raw.path) {
    return { kind: 'local', path: raw.path, ...base };
  }
  return undefined;
}

/** 標註檔裡的影片資訊轉回連結。 */
export function linkFromFile(value: unknown): VideoLink | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const raw = value as Record<string, unknown>;
  const id = typeof raw.url === 'string' ? parseYoutubeId(raw.url) : null;
  if (!id) return undefined;
  return cleanLink({ ...raw, kind: 'youtube', id });
}

/** 連結轉成標註檔的影片資訊；本機影片不匯出（路徑只對自己有意義）。 */
export function linkToFile(link: VideoLink | undefined): AnnotationVideo | undefined {
  if (!link || link.kind !== 'youtube' || !link.id) return undefined;
  return {
    url: youtubeUrl(link.id),
    title: link.title,
    ...(link.channel ? { channel: link.channel } : {}),
    offset: link.offset,
    ...(link.mirror ? { mirror: true } : {}),
  };
}

/** 兩個連結是否指向同一部影片。 */
export function sameVideo(a: VideoLink | undefined | null, b: VideoLink | undefined | null): boolean {
  if (!a || !b || a.kind !== b.kind) return false;
  return a.kind === 'youtube' ? a.id === b.id : a.path === b.path;
}

// ---- 對齊記憶：同一份譜面配同一部影片，換過影片、解除連結或紀錄刪掉重加後，選回來都沿用上次的對齊。

const ALIGN_KEY = 'maimotion.video-align.v1';
const ALIGN_LIMIT = 500;

interface Alignment {
  offset: number;
  mirror?: boolean;
  at: number;
}

function videoKey(link: VideoLink): string | null {
  if (link.kind === 'youtube') return link.id ? `yt:${link.id}` : null;
  return link.path ? `file:${link.path}` : null;
}

function readAlignments(): Record<string, Alignment> {
  try {
    const data: unknown = JSON.parse(localStorage.getItem(ALIGN_KEY) ?? '{}');
    return typeof data === 'object' && data !== null ? (data as Record<string, Alignment>) : {};
  } catch {
    return {};
  }
}

/** 記下這份譜面（原文雜湊）配這部影片的對齊；超過上限時丟掉最舊的。 */
export function rememberAlignment(chartHash: string, link: VideoLink): void {
  const key = videoKey(link);
  if (!key || link.offset === null || !Number.isFinite(link.offset)) return;
  const all = readAlignments();
  const id = `${chartHash}|${key}`;
  const saved = all[id];
  if (saved && saved.offset === link.offset && !!saved.mirror === !!link.mirror) return;
  all[id] = { offset: link.offset, ...(link.mirror ? { mirror: true } : {}), at: Date.now() };
  const ids = Object.keys(all);
  if (ids.length > ALIGN_LIMIT) {
    ids.sort((a, b) => (all[a].at ?? 0) - (all[b].at ?? 0));
    for (const old of ids.slice(0, ids.length - ALIGN_LIMIT)) delete all[old];
  }
  try {
    localStorage.setItem(ALIGN_KEY, JSON.stringify(all));
  } catch {
    // 存不了就只是下次要重新對齊。
  }
}

/** 還沒對齊的連結補上記住的對齊；沒有記錄時原樣回傳。 */
export function recallAlignment(chartHash: string, link: VideoLink): VideoLink {
  const key = videoKey(link);
  if (link.offset !== null || !key) return link;
  const saved = readAlignments()[`${chartHash}|${key}`];
  if (!saved || typeof saved.offset !== 'number' || !Number.isFinite(saved.offset)) return link;
  return { ...link, offset: saved.offset, ...(saved.mirror && link.mirror === undefined ? { mirror: true } : {}) };
}

/** 預設搜尋字串：曲名＋難度＋手元。 */
export function searchQuery(title: string, difficulty: string): string {
  return [title.trim(), difficulty.trim(), 'maimai 手元'].filter(Boolean).join(' ');
}

export function formatDuration(seconds: number | null | undefined): string {
  if (!finite(seconds) || seconds < 0) return '';
  const total = Math.round(seconds);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (!finite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 || value >= 100 ? 0 : 1)} ${units[unit]}`;
}

// ---------------------------------------------------------------------------
// Rust 指令（只有桌面版）

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<T>(command, args);
}

export const videoApi = {
  openWindow: () => call<void>('video_open_window'),
  toolsStatus: (refresh = false) => call<ToolsStatus>('video_tools_status', { refresh }),
  installTool: (tool: VideoTool) => call<ToolsStatus>('video_install_tool', { tool }),
  removeTool: (tool: VideoTool) => call<ToolsStatus>('video_remove_tool', { tool }),
  search: (query: string, limit = 12) => call<SearchItem[]>('video_search', { query, limit }),
  download: (id: string, hint?: { title?: string; channel?: string; duration?: number | null }) =>
    call<VideoEntry>('video_download', { id, hint: hint ?? null }),
  cancel: (id: string) => call<void>('video_cancel_download', { id }),
  get: (id: string) => call<VideoEntry | null>('video_get', { id }),
  list: () => call<VideoEntry[]>('video_list'),
  remove: (id: string) => call<void>('video_delete', { id }),
  cacheInfo: () => call<CacheInfo>('video_cache_info'),
  clearCache: () => call<CacheInfo>('video_clear_cache'),
  openLocal: (path: string) => call<LocalVideo>('video_open_local', { path }),
};

/** 桌面版把本機路徑轉成 WebView 能播放的網址。 */
export async function fileSource(path: string): Promise<string> {
  const { convertFileSrc } = await import('@tauri-apps/api/core');
  return convertFileSrc(path);
}

/** 桌面版的檔案選擇視窗；取消時回傳 null。 */
export async function pickVideoFile(): Promise<string | null> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const picked = await open({
    multiple: false,
    directory: false,
    title: '選擇影片檔',
    filters: [{ name: '影片', extensions: ['mp4', 'm4v', 'webm', 'mov', 'mkv'] }],
  });
  return typeof picked === 'string' ? picked : null;
}

/** 監聽 Rust 發出的事件；回傳取消監聽的函式。瀏覽器預覽不做任何事。 */
export async function listenEvent<T>(name: string, handler: (payload: T) => void): Promise<() => void> {
  if (!isDesktop()) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return await listen<T>(name, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// 視窗之間的訊息

/** 主視窗告訴影片視窗的譜面狀態。 */
export interface ChartState {
  recordId: string | null;
  title: string;
  /** 預設搜尋字串 */
  query: string;
  /** 可以標註（有紀錄、有分析結果、有定位鍵） */
  ready: boolean;
  firstTime: number | null;
  lastTime: number | null;
  /** 主視窗時間軸的範圍（譜面時間）；影片畫面在範圍內時，播放由主視窗帶動 */
  range: { start: number; end: number } | null;
  /** 目前選取的音符 */
  selected: { id: string; time: number; label: string } | null;
  link: VideoLink | null;
  /** 已確認／全部 */
  done: number;
  total: number;
}

export type ToVideo =
  | { type: 'state'; state: ChartState }
  /** 主視窗選了一顆音符：影片跳到它（time 為譜面時間） */
  | { type: 'cue'; time: number; noteId: string }
  /** 主視窗在播放：影片跟著播或停；at 為送出時的 Date.now()，用來扣掉傳遞延遲 */
  | { type: 'playback'; playing: boolean; time: number; rate: number; at: number }
  /** 兩邊共用的播放倍率 */
  | { type: 'rate'; rate: number };

export type ToMain =
  | { type: 'hello' }
  | { type: 'link'; link: VideoLink | null }
  /** 影片畫面對應的譜面時間（影片播放或拖曳時） */
  | { type: 'follow'; time: number }
  /** 在影片視窗按的標註快捷鍵 */
  | { type: 'key'; key: string; shiftKey: boolean }
  | { type: 'step'; direction: 1 | -1 }
  /**
   * 在影片視窗操作播放：從 time 開始播（play，改由主視窗帶動），
   * 或主視窗播放中拖動（seek）、暫停（pause）；time 為譜面時間
   */
  | { type: 'transport'; action: 'seek' | 'pause' | 'play'; time: number }
  /** 在影片視窗改播放倍率 */
  | { type: 'rate'; rate: number };

type Envelope = { from: 'main' | 'video'; message: ToVideo | ToMain };

const EVENT = 'maimotion:video-sync';
const CHANNEL = 'maimotion-video-sync';

/**
 * 建立一端的通道。桌面版用 Tauri 事件（兩個視窗都收得到），
 * 瀏覽器預覽用 BroadcastChannel。自己送出的訊息會被忽略。
 */
export function openChannel<In extends ToVideo | ToMain>(
  side: 'main' | 'video',
  onMessage: (message: In) => void,
): { send: (message: ToVideo | ToMain) => void; close: () => void } {
  const accept = (envelope: Envelope | undefined) => {
    if (envelope && envelope.from !== side) onMessage(envelope.message as In);
  };
  if (isDesktop()) {
    let unlisten: (() => void) | null = null;
    let closed = false;
    const events = import('@tauri-apps/api/event');
    // 自己開始監聽之後才送出：否則對方立刻回覆（例如 hello 的回應）會在監聽前送到而遺失。
    const ready = events.then(async ({ listen }) => {
      const stop = await listen<Envelope>(EVENT, (event) => accept(event.payload));
      if (closed) stop();
      else unlisten = stop;
    });
    return {
      send: (message) => {
        void ready.then(() => events).then(({ emit }) => emit(EVENT, { from: side, message }));
      },
      close: () => {
        closed = true;
        unlisten?.();
      },
    };
  }
  if (typeof BroadcastChannel === 'undefined') return { send: () => {}, close: () => {} };
  const channel = new BroadcastChannel(CHANNEL);
  channel.onmessage = (event: MessageEvent<Envelope>) => accept(event.data);
  return {
    send: (message) => channel.postMessage({ from: side, message }),
    close: () => channel.close(),
  };
}

/** 目前這個頁面是不是影片視窗。 */
export function isVideoWindow(): boolean {
  if (typeof window === 'undefined') return false;
  if (new URLSearchParams(window.location.search).get('view') === 'video') return true;
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } })
    .__TAURI_INTERNALS__;
  return internals?.metadata?.currentWindow?.label === 'video';
}
