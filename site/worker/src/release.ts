// GET /api/release：最新版本與四個平台的下載檔（下載頁與首頁用）。
//
// 來源依序：
// 1. caches.default 裡還新鮮的那份（約 10 分鐘）；
// 2. GitHub API releases/latest（未認證、通用 User-Agent、不帶任何使用者資訊）；
// 3. API 失敗或被限流：有舊的就回舊的並標 stale: true（一分鐘後才再試 GitHub）；
// 4. 沒有舊的：抓 github.com/…/releases/latest（不跟隨轉址），從 Location 取 tag，下載網址用檔名規則組出來（沒有大小、SHA-256）；
// 5. 都失敗：503 { ok: false }。
// Workers 對外的 IP 是很多人共用的，未認證的 GitHub API 每個 IP 每小時 60 次，很容易被別人用完，所以第 3、4 步是常態而不是例外。

import {
  LATEST_API_URL,
  LATEST_PAGE_URL,
  parseRelease,
  releaseFromApi,
  releaseFromTag,
  tagFromLocation,
  type ReleaseInfo,
} from "../../src/lib/release/core.ts";

const USER_AGENT = "maimotiondemo-docs-worker";
/** API 拿到的資料新鮮多久。 */
const FRESH_MS = 10 * 60_000;
/** 只從轉址取到 tag（沒有大小）時，早一點再試 API。 */
const PARTIAL_FRESH_MS = 2 * 60_000;
/** GitHub 失敗後，多久內不再去問（先回舊的）。 */
const RETRY_MS = 60_000;
/** 向 GitHub 發一次請求最多等多久。 */
const UPSTREAM_TIMEOUT_MS = 5_000;
/** 邊緣快取保留多久：過了新鮮期仍留著，GitHub 失敗時才有舊的可回。 */
const EDGE_KEEP_S = 7 * 24 * 60 * 60;
/** 瀏覽器快取。 */
const BROWSER_MAX_AGE = 300;
const BROWSER_MAX_AGE_STALE = 60;

/** 存在 caches.default 的那一份（不是對外的格式）。 */
interface Entry {
  info: ReleaseInfo;
  /** 這份資料從 GitHub 拿到的時間。 */
  fetchedAt: number;
  /** 這個時間之前不再去問 GitHub。 */
  retryAt: number;
  stale: boolean;
}

// isolate 內的副本：同一個 isolate 連續的請求不必每次讀 caches.default；同時進來的更新只發一次。
let memory: Entry | null = null;
let refreshing: Promise<Entry | null> | null = null;

function cacheKey(origin: string): Request {
  return new Request(`${origin}/api/release/__cache`);
}

async function readEntry(origin: string): Promise<Entry | null> {
  const hit = await caches.default.match(cacheKey(origin));
  if (!hit) return null;
  try {
    const raw = (await hit.json()) as Partial<Entry>;
    const info = parseRelease(raw.info);
    if (!info || typeof raw.fetchedAt !== "number" || typeof raw.retryAt !== "number") return null;
    return { info, fetchedAt: raw.fetchedAt, retryAt: raw.retryAt, stale: raw.stale === true };
  } catch {
    return null;
  }
}

function writeEntry(origin: string, entry: Entry, ctx: ExecutionContext): void {
  memory = entry;
  ctx.waitUntil(
    caches.default.put(
      cacheKey(origin),
      new Response(JSON.stringify(entry), {
        headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": `public, max-age=${EDGE_KEEP_S}` },
      }),
    ),
  );
}

async function fromApi(): Promise<ReleaseInfo | null> {
  try {
    const response = await fetch(LATEST_API_URL, {
      headers: {
        Accept: "application/vnd.github+json",
        "User-Agent": USER_AGENT,
        "X-GitHub-Api-Version": "2022-11-28",
      },
      signal: AbortSignal.timeout(UPSTREAM_TIMEOUT_MS),
    });
    if (!response.ok) {
      console.warn("github api", response.status, `remaining=${response.headers.get("x-ratelimit-remaining") ?? "?"}`);
      await response.body?.cancel();
      return null;
    }
    const info = releaseFromApi(await response.json());
    if (!info) console.warn("github api: unexpected release format");
    return info;
  } catch (error) {
    console.warn("github api failed", String(error));
    return null;
  }
}

async function fromRedirect(): Promise<ReleaseInfo | null> {
  try {
    const response = await fetch(LATEST_PAGE_URL, {
      redirect: "manual",
      headers: { "User-Agent": USER_AGENT },
      signal: AbortSignal.timeout(UPSTREAM_TIMEOUT_MS),
    });
    await response.body?.cancel();
    const location = response.headers.get("Location");
    if (response.status < 300 || response.status >= 400 || !location) {
      console.warn("github redirect: no location", response.status);
      return null;
    }
    // 只信 github.com 上的轉址。
    const target = new URL(location, LATEST_PAGE_URL);
    if (target.origin !== "https://github.com") return null;
    const tag = tagFromLocation(target.pathname);
    return tag ? releaseFromTag(tag, "redirect") : null;
  } catch (error) {
    console.warn("github redirect failed", String(error));
    return null;
  }
}

/** 向 GitHub 更新；回 null 表示 GitHub 這次完全拿不到、也沒有舊的。 */
async function refresh(origin: string, previous: Entry | null, ctx: ExecutionContext): Promise<Entry | null> {
  const now = Date.now();
  const api = await fromApi();
  if (api) {
    const entry: Entry = { info: api, fetchedAt: now, retryAt: now + FRESH_MS, stale: false };
    writeEntry(origin, entry, ctx);
    return entry;
  }
  // API 不能用：有 API 來的舊資料就先回它（有大小與 SHA-256，比只有 tag 的完整）。
  if (previous && previous.info.source === "api") {
    const entry: Entry = { ...previous, retryAt: now + RETRY_MS, stale: true };
    writeEntry(origin, entry, ctx);
    return entry;
  }
  const partial = await fromRedirect();
  if (partial) {
    const entry: Entry = { info: partial, fetchedAt: now, retryAt: now + PARTIAL_FRESH_MS, stale: false };
    writeEntry(origin, entry, ctx);
    return entry;
  }
  if (previous) {
    const entry: Entry = { ...previous, retryAt: now + RETRY_MS, stale: true };
    writeEntry(origin, entry, ctx);
    return entry;
  }
  return null;
}

function respond(entry: Entry, cache: "hit" | "miss" | "stale"): Response {
  const body: ReleaseInfo = entry.stale ? { ...entry.info, stale: true } : entry.info;
  return new Response(JSON.stringify(body), {
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "X-Content-Type-Options": "nosniff",
      "Cache-Control": `public, max-age=${entry.stale ? BROWSER_MAX_AGE_STALE : BROWSER_MAX_AGE}`,
      "X-Release-Cache": entry.stale ? "stale" : cache,
      "X-Release-Source": entry.info.source,
    },
  });
}

function unavailable(status: number, error: string, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify({ ok: false, error }), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "X-Content-Type-Options": "nosniff",
      "Cache-Control": "no-store",
      ...headers,
    },
  });
}

export async function release(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
  const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
  // 和 /api/search 同一個 rate limit 綁定（每個 IP 每 10 秒 20 次），key 分開算。
  const { success } = await env.LIMIT.limit({ key: `release:${ip}` });
  if (!success) return unavailable(429, "rate_limited", { "Retry-After": "10" });

  const origin = new URL(request.url).origin;
  const now = Date.now();
  let entry = memory && now < memory.retryAt ? memory : await readEntry(origin);
  if (entry && now < entry.retryAt) {
    memory = entry;
    return respond(entry, "hit");
  }

  const previous = entry ?? memory;
  refreshing ??= refresh(origin, previous, ctx).finally(() => {
    refreshing = null;
  });
  entry = await refreshing;
  if (!entry) return unavailable(503, "release_unavailable");
  return respond(entry, "miss");
}
