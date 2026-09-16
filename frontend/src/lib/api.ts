import type { AnalyzeRequest, AnalyzeResponse, MajdataChartSummary } from './types';

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
