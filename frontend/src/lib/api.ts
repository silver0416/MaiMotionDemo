import type {
  AnalyzeRequest,
  AnalyzeResponse,
  MajdataChartSummary,
  WikiChartPayload,
  WikiChartType,
  WikiIndexPayload,
  WikiSong,
} from './types';

/**
 * 真正的 Tauri adapter：只有桌面殼層可用。
 * 瀏覽器預覽不會走到這裡，也不會把使用者輸入偽裝成已解析結果。
 */
export function isDesktop(): boolean {
  if (typeof window === 'undefined') return false;
  const w = window as unknown as Record<string, unknown>;
  return '__TAURI_INTERNALS__' in w || '__TAURI__' in w;
}

export async function analyzeChart(request: AnalyzeRequest): Promise<AnalyzeResponse> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<AnalyzeResponse>('analyze_chart', { request });
}

/** 經由 Rust 搜尋 Majdata；前端不直接連線外部網站。失敗時 reject 可讀的字串。 */
export async function searchMajdataCharts(query: string): Promise<MajdataChartSummary[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<MajdataChartSummary[]>('search_majdata_charts', { query });
}

/** 經由 Rust 下載 Majdata 譜面，回傳 maidata.txt 原文。 */
export async function fetchMajdataChart(songId: string): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<string>('fetch_majdata_chart', { songId });
}

/** 經由 Rust 載入 simai Wiki 索引；force=false 時優先用記憶體或 24 小時快取。 */
export async function refreshWikiIndex(force: boolean): Promise<WikiIndexPayload> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<WikiIndexPayload>('wiki_refresh_index', { force });
}

/** 在 Rust 已載入的 Wiki 索引中搜尋（本機，不連 Wiki）；索引未載入時 reject。 */
export async function searchWikiSongs(
  query: string,
  chartType: 'all' | WikiChartType = 'all',
): Promise<WikiSong[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<WikiSong[]>('wiki_search_songs', { query, chartType });
}

/** 經由 Rust 取得 Wiki 某首歌某個難度的 simai 文字譜。 */
export async function fetchWikiChart(
  pageId: number,
  chartType: WikiChartType,
  difficulty: string,
): Promise<WikiChartPayload> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<WikiChartPayload>('wiki_fetch_chart', { pageId, chartType, difficulty });
}

export interface WikiCacheInfo {
  files: number;
  bytes: number;
}

/** simai Wiki 磁碟快取（索引＋歌曲頁）的用量。只有桌面版可用。 */
export async function wikiCacheInfo(): Promise<WikiCacheInfo> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<WikiCacheInfo>('wiki_cache_info');
}

/** 清除 simai Wiki 磁碟快取與 Rust 記憶體中的索引；回傳清除前的用量。 */
export async function clearWikiCache(): Promise<WikiCacheInfo> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<WikiCacheInfo>('wiki_clear_cache');
}

/**
 * 用系統預設瀏覽器開啟外部連結。
 * 桌面版經 opener plugin 開啟（不會讓 WebView 跳頁）；瀏覽器預覽用 window.open。
 */
export async function openExternalUrl(url: string): Promise<void> {
  if (isDesktop()) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
    return;
  }
  window.open(url, '_blank', 'noopener,noreferrer');
}

let versionPromise: Promise<string> | null = null;

/** 桌面版回傳 Tauri 設定的版本；分析快取以此區分核心版本。瀏覽器預覽固定為 preview。 */
export function appVersion(): Promise<string> {
  if (!versionPromise) {
    versionPromise = isDesktop()
      ? import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).catch(() => 'unknown')
      : Promise.resolve('preview');
  }
  return versionPromise;
}

let counter = 0;

export function nextRequestId(): string {
  counter += 1;
  return `ui-${Date.now().toString(36)}-${counter}`;
}
