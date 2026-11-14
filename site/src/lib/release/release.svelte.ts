import { parseRelease, type ReleaseInfo } from './core';

/**
 * 最新版本（首頁與下載頁共用一份）。
 * 先用建置時的快照（預先算繪的 HTML 裡就有連結）；掛載後呼叫 refresh() 向 /api/release 要最新的。
 * vite dev 沒有 Worker、離線、逾時或回應格式不對時安靜沿用快照，不報錯。
 */
const API_URL = '/api/release';
const TIMEOUT_MS = 6000;

/** 比較 x.y.z 的數字部分（後綴不管）。 */
function compareVersions(a: string, b: string): number {
  const parts = (value: string) => value.split(/[-+]/)[0].split('.').map((part) => Number(part) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i += 1) {
    const diff = (x[i] ?? 0) - (y[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

class LatestRelease {
  info = $state<ReleaseInfo | null>(__RELEASE_SNAPSHOT__);
  /** 已經換成 Worker 回的資料。 */
  live = $state(false);
  #started = false;

  /** 只問一次（換頁回來不重問）。 */
  refresh(): void {
    if (this.#started || typeof fetch === 'undefined') return;
    this.#started = true;
    void this.#load();
  }

  async #load(): Promise<void> {
    try {
      const response = await fetch(API_URL, { headers: { Accept: 'application/json' }, signal: AbortSignal.timeout(TIMEOUT_MS) });
      if (!response.ok) return;
      const info = parseRelease(await response.json());
      if (!info || info.assets.length === 0) return;
      const current = this.info;
      // Worker 回的是 GitHub 失敗時的舊資料、而且比建置時的快照還舊：留著快照。
      if (current && compareVersions(info.version, current.version) < 0) return;
      // 同一版但 Worker 只拿到 tag（沒有大小、SHA-256）：用快照補上。
      if (current && current.tag === info.tag) {
        info.assets = info.assets.map((asset) => {
          const known = current.assets.find((item) => item.name === asset.name);
          return { ...asset, size: asset.size ?? known?.size, sha256: asset.sha256 ?? known?.sha256 };
        });
        info.publishedAt ??= current.publishedAt;
      }
      this.info = info;
      this.live = true;
    } catch {
      // 沿用快照。
    }
  }
}

export const latestRelease = new LatestRelease();
