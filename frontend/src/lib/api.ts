import type { AnalyzeRequest, AnalyzeResponse } from './types';

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

let counter = 0;

export function nextRequestId(): string {
  counter += 1;
  return `ui-${Date.now().toString(36)}-${counter}`;
}
