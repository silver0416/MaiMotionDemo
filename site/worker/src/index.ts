// 文件站的 Worker。靜態檔（HTML、_app/、search/）在這之前就由 assets 回應，不會進到這裡；
// 這裡只收到沒對到檔案的請求：GET /api/search 做語意搜尋、GET /api/release 回最新版本，其他一律交回 assets（它會回 404.html）。

import { release } from "./release.ts";

/** 由 vite.config.ts 依 site/build 的行內 script 算好的 CSP。 */
declare const __DOCS_CSP__: string;

const API_PATH = "/api/search";
const RELEASE_PATH = "/api/release";
const VECTORS_PATH = "/search/vectors.bin";
/** 回傳筆數上限。 */
const TOP_K = 12;
/** 查詢字長度上限（字元）。 */
const MAX_QUERY = 200;
/**
 * 低於這個 cosine 就不回（依 scripts/eval-search.mjs 定；換模型要重新評估）。
 * bge-m3：正確頁最高分的那一塊最低 0.466、P10 0.577；無關查詢的第一名 0.35–0.50。
 */
const THRESHOLDS: Record<string, number> = { "@cf/baai/bge-m3": 0.47 };
const DEFAULT_THRESHOLD = 0.5;
/** 瀏覽器快取 10 分鐘；邊緣（caches.default）快取 7 天，key 含索引 version 與模型，內容一換自然失效。 */
const BROWSER_MAX_AGE = 600;
const EDGE_MAX_AGE = 7 * 24 * 60 * 60;
/** 用戶端帶來的 version 和手上的不同時，最快多久重抓一次向量檔。 */
const RELOAD_INTERVAL_MS = 60_000;

interface Store {
  model: string;
  version: string;
  dim: number;
  ids: string[];
  scales: Float32Array;
  data: Int8Array;
  loadedAt: number;
}

// isolate 的模組變數：同一個 isolate 的後續請求共用，不必每次重抓、重解析 340 kB 的向量檔。
let store: Store | null = null;
let loading: Promise<Store> | null = null;

function parseVectors(buffer: ArrayBuffer): Omit<Store, "loadedAt"> {
  const view = new DataView(buffer);
  const bytes = new Uint8Array(buffer);
  const decoder = new TextDecoder();
  if (decoder.decode(bytes.subarray(0, 4)) !== "MMVS") throw new Error("vectors.bin: bad magic");
  if (view.getUint16(4, true) !== 1 || view.getUint16(6, true) !== 1) throw new Error("vectors.bin: unsupported format");
  const dim = view.getUint32(8, true);
  const count = view.getUint32(12, true);
  let at = 16;
  const modelLength = view.getUint16(at, true);
  const model = decoder.decode(bytes.subarray(at + 2, at + 2 + modelLength));
  at += 2 + modelLength;
  const versionLength = view.getUint16(at, true);
  const version = decoder.decode(bytes.subarray(at + 2, at + 2 + versionLength));
  at += 2 + versionLength;
  const idsLength = view.getUint32(at, true);
  const ids = decoder.decode(bytes.subarray(at + 4, at + 4 + idsLength)).split("\n");
  at += 4 + idsLength;
  at = Math.ceil(at / 4) * 4;
  if (ids.length !== count) throw new Error("vectors.bin: id count mismatch");
  const scales = new Float32Array(buffer, at, count);
  at += count * 4;
  const data = new Int8Array(buffer, at, count * dim);
  return { model, version, dim, ids, scales, data };
}

async function loadStore(env: Env, origin: string, wanted: string | null): Promise<Store> {
  const fresh = store && (!wanted || wanted === store.version || Date.now() - store.loadedAt < RELOAD_INTERVAL_MS);
  if (store && fresh) return store;
  loading ??= (async () => {
    try {
      const response = await env.ASSETS.fetch(new Request(new URL(VECTORS_PATH, origin)));
      if (!response.ok) throw new Error(`vectors.bin: HTTP ${response.status}`);
      store = { ...parseVectors(await response.arrayBuffer()), loadedAt: Date.now() };
      return store;
    } finally {
      loading = null;
    }
  })();
  return loading;
}

function normalizeQuery(raw: string | null): string {
  return (raw ?? "").normalize("NFKC").trim().replace(/\s+/g, " ").slice(0, MAX_QUERY);
}

function json(body: unknown, status: number, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "X-Content-Type-Options": "nosniff",
      ...headers,
    },
  });
}

/** 查詢字 → 單位向量。 */
async function embedQuery(env: Env, model: string, query: string): Promise<Float32Array> {
  const output = (await env.AI.run(model as Parameters<Ai["run"]>[0], { text: [query] } as never)) as {
    data?: number[][];
  };
  const vector = output?.data?.[0];
  if (!Array.isArray(vector) || vector.length === 0) throw new Error("embedding: empty result");
  let sum = 0;
  for (const v of vector) sum += v * v;
  const norm = Math.sqrt(sum) || 1;
  return Float32Array.from(vector, (v) => v / norm);
}

function rank(index: Store, query: Float32Array): { id: string; score: number }[] {
  if (query.length !== index.dim) throw new Error(`embedding: dim ${query.length} ≠ ${index.dim}`);
  const threshold = THRESHOLDS[index.model] ?? DEFAULT_THRESHOLD;
  const scored: { id: string; score: number }[] = [];
  const { dim, data, scales, ids } = index;
  for (let i = 0; i < ids.length; i += 1) {
    let sum = 0;
    const base = i * dim;
    for (let j = 0; j < dim; j += 1) sum += query[j] * data[base + j];
    const score = sum * scales[i];
    if (score >= threshold) scored.push({ id: ids[i], score: Math.round(score * 10_000) / 10_000 });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, TOP_K);
}

async function search(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
  const url = new URL(request.url);
  const query = normalizeQuery(url.searchParams.get("q"));
  if (!query) return json({ ok: false, error: "empty_query" }, 400);

  const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
  const { success } = await env.LIMIT.limit({ key: ip });
  if (!success) return json({ ok: false, error: "rate_limited" }, 429, { "Retry-After": "10" });

  let index: Store;
  try {
    index = await loadStore(env, url.origin, url.searchParams.get("v"));
  } catch (error) {
    console.error("search index unavailable", String(error));
    return json({ ok: false, error: "index_unavailable" }, 503);
  }

  const cache = caches.default;
  const key = new Request(
    `${url.origin}${API_PATH}/__cache?m=${encodeURIComponent(index.model)}&v=${index.version}&q=${encodeURIComponent(query)}`,
  );
  const hit = await cache.match(key);
  if (hit) {
    const body = await hit.text();
    return new Response(body, {
      headers: {
        "Content-Type": "application/json; charset=utf-8",
        "X-Content-Type-Options": "nosniff",
        "Cache-Control": `public, max-age=${BROWSER_MAX_AGE}`,
        "X-Search-Cache": "hit",
      },
    });
  }

  let vector: Float32Array;
  try {
    vector = await embedQuery(env, index.model, query);
  } catch (error) {
    console.error("embedding unavailable", String(error));
    return json({ ok: false, error: "embedding_unavailable" }, 503, { "Cache-Control": "no-store" });
  }

  const body = JSON.stringify({ ok: true, version: index.version, results: rank(index, vector) });
  ctx.waitUntil(
    cache.put(
      key,
      new Response(body, {
        headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": `public, max-age=${EDGE_MAX_AGE}` },
      }),
    ),
  );
  return new Response(body, {
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "X-Content-Type-Options": "nosniff",
      "Cache-Control": `public, max-age=${BROWSER_MAX_AGE}`,
      "X-Search-Cache": "miss",
    },
  });
}

/** Worker 交回 assets 的回應（主要是 404.html）：補上和 _headers 相同的安全標頭。 */
function withSecurityHeaders(response: Response): Response {
  const type = response.headers.get("Content-Type") ?? "";
  if (!type.includes("text/html") || response.headers.has("Content-Security-Policy")) return response;
  const out = new Response(response.body, response);
  out.headers.set("Content-Security-Policy", __DOCS_CSP__);
  out.headers.set("X-Content-Type-Options", "nosniff");
  out.headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  return out;
}

export default {
  async fetch(request, env, ctx): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === RELEASE_PATH) {
      if (request.method !== "GET") {
        return json({ ok: false, error: "method_not_allowed" }, 405, { Allow: "GET" });
      }
      try {
        return await release(request, env, ctx);
      } catch (error) {
        console.error("release failed", String(error));
        return json({ ok: false, error: "release_unavailable" }, 503, { "Cache-Control": "no-store" });
      }
    }
    if (url.pathname === API_PATH) {
      if (request.method !== "GET") {
        return json({ ok: false, error: "method_not_allowed" }, 405, { Allow: "GET" });
      }
      try {
        return await search(request, env, ctx);
      } catch (error) {
        // 不讓任何意外變成未處理的例外（Cloudflare 的錯誤頁）。
        console.error("search failed", String(error));
        return json({ ok: false, error: "embedding_unavailable" }, 503, { "Cache-Control": "no-store" });
      }
    }
    return withSecurityHeaders(await env.ASSETS.fetch(request));
  },
} satisfies ExportedHandler<Env>;
