/**
 * 最新版本資料：Worker（/api/release）、建置時快照（vite.config.ts）與前端共用的整理規則。
 * 這個檔只有純函式，不碰 DOM、不發請求，三個地方都能直接 import。
 *
 * 挑檔的規則對照 App（src-tauri/src/update.rs 的 pick_asset）：只收本專案 Release 的下載網址、檔名不含路徑、
 * 大小大於 0；優先完全符合 `MaiMotionDemo-v{版本}-{平台後綴}` 的檔，沒有才找同後綴的；SHA-256 取 API 的 digest。
 */

export const REPO = 'silver0416/MaiMotionDemo';
/** GitHub 的發布頁（所有版本）。 */
export const RELEASES_URL = `https://github.com/${REPO}/releases`;
/** 最新版的 API（未認證，每個 IP 每小時 60 次）。 */
export const LATEST_API_URL = `https://api.github.com/repos/${REPO}/releases/latest`;
/** 最新版的網頁：會 302 轉到 /releases/tag/<tag>，API 被限流時從這裡取 tag。 */
export const LATEST_PAGE_URL = `${RELEASES_URL}/latest`;
/** 只接受這個前綴的下載網址，避免被導去別處下載執行檔。 */
export const ASSET_URL_PREFIX = `${RELEASES_URL}/download/`;

export type Platform = 'windows' | 'macos' | 'linux-appimage' | 'linux-deb';

/** 平台與檔名後綴（repo 的 README：MaiMotionDemo-v{版本}-windows-x64.exe…）。順序就是下載頁清單的順序。 */
export const PLATFORM_SUFFIX: Readonly<Record<Platform, string>> = {
  windows: '-windows-x64.exe',
  macos: '-macos-universal.zip',
  'linux-appimage': '-linux-x64.AppImage',
  'linux-deb': '-linux-x64.deb',
};

export const PLATFORMS = Object.keys(PLATFORM_SUFFIX) as Platform[];

export interface ReleaseAsset {
  platform: Platform;
  name: string;
  /** GitHub 的 browser_download_url。 */
  url: string;
  /** 位元組；只有從 API 拿到的才有。 */
  size?: number;
  /** 小寫十六進位；API 有 digest 才有。 */
  sha256?: string;
}

/** 資料從哪裡來：api＝GitHub API；redirect＝API 不能用時從 /releases/latest 的轉址取 tag；package＝建置時兩者都失敗，用 repo 的 package.json。 */
export type ReleaseSource = 'api' | 'redirect' | 'package';

export interface ReleaseInfo {
  ok: true;
  /** 不含 v 的版本號，例如 1.2.1。 */
  version: string;
  /** Git tag，例如 v1.2.1。 */
  tag: string;
  /** ISO 8601；轉址或 package.json 的後備沒有日期。 */
  publishedAt: string | null;
  /** 這一版在 GitHub 的頁面（更新內容）。 */
  pageUrl: string;
  assets: ReleaseAsset[];
  source: ReleaseSource;
  /** GitHub 暫時拿不到，這是上一次的資料。 */
  stale?: boolean;
}

/** tag 只收 v1.2.3 這種樣子（可帶 -beta.1 之類的後綴），其他一律不信。 */
const TAG_PATTERN = /^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

/** 去掉前導 v，只留 x.y.z 與後綴（和 update.rs 的 normalize_version 一樣只去 v）。 */
export function versionOf(tag: string): string {
  return tag.trim().replace(/^[vV]/, '');
}

function isTag(value: unknown): value is string {
  return typeof value === 'string' && TAG_PATTERN.test(value.trim());
}

function tagPageUrl(tag: string): string {
  return `${RELEASES_URL}/tag/${encodeURIComponent(tag)}`;
}

function isIsoDate(value: unknown): value is string {
  return typeof value === 'string' && value.length <= 40 && !Number.isNaN(Date.parse(value));
}

/** 用檔名規則組出四個下載網址（沒有大小、沒有 SHA-256）。 */
export function assetsFromTag(tag: string): ReleaseAsset[] {
  const version = versionOf(tag);
  return PLATFORMS.map((platform) => {
    const name = `MaiMotionDemo-v${version}${PLATFORM_SUFFIX[platform]}`;
    return { platform, name, url: `${ASSET_URL_PREFIX}${encodeURIComponent(tag)}/${name}` };
  });
}

/** 只有 tag（轉址或 package.json）時的資料。 */
export function releaseFromTag(tag: string, source: Exclude<ReleaseSource, 'api'>): ReleaseInfo | null {
  if (!isTag(tag)) return null;
  const clean = tag.trim();
  return {
    ok: true,
    version: versionOf(clean),
    tag: clean,
    publishedAt: null,
    pageUrl: tagPageUrl(clean),
    assets: assetsFromTag(clean),
    source,
  };
}

/** /releases/latest 轉址的 Location（…/releases/tag/v1.2.1）→ tag。 */
export function tagFromLocation(location: string | null): string | null {
  if (!location) return null;
  const match = /\/releases\/tag\/([^/?#]+)\/?(?:[?#].*)?$/.exec(location);
  if (!match) return null;
  try {
    const tag = decodeURIComponent(match[1]);
    return isTag(tag) ? tag : null;
  } catch {
    return null;
  }
}

interface ApiAsset {
  name?: unknown;
  browser_download_url?: unknown;
  size?: unknown;
  digest?: unknown;
}

function parseApiAsset(raw: ApiAsset): Omit<ReleaseAsset, 'platform'> | null {
  const { name, browser_download_url: url, size, digest } = raw ?? {};
  if (typeof name !== 'string' || typeof url !== 'string' || typeof size !== 'number') return null;
  if (!url.startsWith(ASSET_URL_PREFIX) || !(size > 0) || /[/\\]/.test(name)) return null;
  const hex = typeof digest === 'string' && digest.startsWith('sha256:') ? digest.slice(7) : '';
  const sha256 = /^[0-9a-fA-F]{64}$/.test(hex) ? hex.toLowerCase() : undefined;
  return { name, url, size, ...(sha256 ? { sha256 } : {}) };
}

/** GitHub API 的 releases/latest → 整理後的資料；格式不對（或一個檔都對不上）回 null。 */
export function releaseFromApi(json: unknown): ReleaseInfo | null {
  if (!json || typeof json !== 'object') return null;
  const raw = json as { tag_name?: unknown; published_at?: unknown; html_url?: unknown; assets?: unknown };
  if (!isTag(raw.tag_name)) return null;
  const tag = raw.tag_name.trim();
  const version = versionOf(tag);
  const candidates = (Array.isArray(raw.assets) ? raw.assets : [])
    .map((asset) => parseApiAsset(asset as ApiAsset))
    .filter((asset): asset is NonNullable<typeof asset> => asset !== null);
  const assets: ReleaseAsset[] = [];
  for (const platform of PLATFORMS) {
    const suffix = PLATFORM_SUFFIX[platform];
    const expected = `MaiMotionDemo-v${version}${suffix}`;
    const found = candidates.find((asset) => asset.name === expected) ?? candidates.find((asset) => asset.name.endsWith(suffix));
    if (found) assets.push({ platform, ...found });
  }
  if (assets.length === 0) return null;
  const pageUrl = typeof raw.html_url === 'string' && raw.html_url.startsWith(`${RELEASES_URL}/`) ? raw.html_url : tagPageUrl(tag);
  return {
    ok: true,
    version,
    tag,
    publishedAt: isIsoDate(raw.published_at) ? raw.published_at : null,
    pageUrl,
    assets,
    source: 'api',
  };
}

/**
 * /api/release 的回應（或任何不可信的 JSON）→ 驗證過的資料；不對就回 null。
 * 前端只信這裡放行的網址：一定是本專案 Release 的下載網址與發布頁。
 */
export function parseRelease(json: unknown): ReleaseInfo | null {
  if (!json || typeof json !== 'object') return null;
  const raw = json as Partial<Record<keyof ReleaseInfo, unknown>>;
  if (raw.ok !== true || !isTag(raw.tag) || typeof raw.version !== 'string' || !Array.isArray(raw.assets)) return null;
  if (typeof raw.pageUrl !== 'string' || !raw.pageUrl.startsWith(`${RELEASES_URL}/`)) return null;
  const source: ReleaseSource = raw.source === 'redirect' || raw.source === 'package' ? raw.source : 'api';
  const assets: ReleaseAsset[] = [];
  for (const item of raw.assets as Partial<ReleaseAsset>[]) {
    if (!item || typeof item !== 'object') continue;
    const { platform, name, url, size, sha256 } = item;
    if (typeof platform !== 'string' || !PLATFORMS.includes(platform as Platform)) continue;
    if (typeof name !== 'string' || /[/\\]/.test(name) || typeof url !== 'string' || !url.startsWith(ASSET_URL_PREFIX)) continue;
    if (assets.some((asset) => asset.platform === platform)) continue;
    assets.push({
      platform: platform as Platform,
      name,
      url,
      ...(typeof size === 'number' && size > 0 ? { size } : {}),
      ...(typeof sha256 === 'string' && /^[0-9a-f]{64}$/.test(sha256) ? { sha256 } : {}),
    });
  }
  return {
    ok: true,
    version: raw.version,
    tag: raw.tag.trim(),
    publishedAt: isIsoDate(raw.publishedAt) ? raw.publishedAt : null,
    pageUrl: raw.pageUrl,
    assets: PLATFORMS.flatMap((platform) => assets.filter((asset) => asset.platform === platform)),
    source,
    ...(raw.stale === true ? { stale: true } : {}),
  };
}

/** 發布日期（台灣時間）→ 2026-11-13。前後端算出來一樣，不會有 hydration 差異。 */
export function formatDate(iso: string | null): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  const parts = new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Taipei', year: 'numeric', month: '2-digit', day: '2-digit' }).formatToParts(date);
  const get = (type: string) => parts.find((part) => part.type === type)?.value ?? '';
  return `${get('year')}-${get('month')}-${get('day')}`;
}

/** 檔案大小 → 43.0 MB（1 MB＝1024²，和 Windows 檔案總管一致）。 */
export function formatSize(bytes: number | undefined): string | null {
  if (!bytes || !(bytes > 0)) return null;
  const mb = bytes / 1024 / 1024;
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
