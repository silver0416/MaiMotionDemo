import { cloudflare } from "@cloudflare/vite-plugin";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

/** SvelteKit 的輸出（先在 site/ 跑 npm run build）。整個資料夾就是這個 Worker 的靜態檔。 */
const SITE_BUILD = fileURLToPath(new URL("../build", import.meta.url));

function htmlFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...htmlFiles(path));
    else if (name.endsWith(".html")) out.push(path);
  }
  return out;
}

/**
 * 每一頁的行內 <script>（SvelteKit 的啟動程式、app.html 套用主題的那段）算 sha256，全部列進 script-src。
 * 不必開 'unsafe-inline'。內容改了、重新 build，hash 會跟著重算。
 */
function scriptHashes(): string[] {
  if (!existsSync(SITE_BUILD)) return [];
  const hashes = new Set<string>();
  for (const file of htmlFiles(SITE_BUILD)) {
    const html = readFileSync(file, "utf8");
    for (const match of html.matchAll(/<script(?![^>]*\bsrc=)([^>]*)>([\s\S]*?)<\/script>/g)) {
      const type = /\btype="([^"]*)"/.exec(match[1])?.[1];
      // 只有會執行的 script 要 hash（application/json 之類的資料不用）。
      if (type && type !== "module" && type !== "text/javascript") continue;
      hashes.add(`'sha256-${createHash("sha256").update(match[2], "utf8").digest("base64")}'`);
    }
  }
  return [...hashes].sort();
}

function contentSecurityPolicy(): string {
  return [
    "default-src 'self'",
    // static.cloudflareinsights.com：Cloudflare 會自動在 HTML 插入 Web Analytics 的 beacon（外部 script），
    // 使用者 2026-11-15 決定放行，才有訪客統計。它把資料送到 cloudflareinsights.com（見 connect-src）。
    `script-src 'self' https://static.cloudflareinsights.com ${scriptHashes().join(" ")}`.trim(),
    // Svelte 的 style="…" 屬性與轉場動畫會寫行內樣式；樣式不能執行程式，保留 'unsafe-inline'。
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self'",
    "connect-src 'self' https://cloudflareinsights.com",
    "media-src 'self' blob:",
    // Web App：service worker（/service-worker.js）與 manifest（/manifest.webmanifest）都是同源。
    "worker-src 'self'",
    "manifest-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
  ].join("; ");
}

/**
 * 產生 assets 的 _headers（Workers 靜態檔支援這個檔）：靜態檔不經過 Worker，安全標頭與快取只能放這裡。
 * 同一份 CSP 也用 define 交給 Worker，套在 Worker 回的 HTML（404 頁）上。
 */
function docsHeaders(): Plugin {
  let csp = "";
  return {
    name: "docs-headers",
    config() {
      csp = contentSecurityPolicy();
      return { define: { __DOCS_CSP__: JSON.stringify(csp) } };
    },
    generateBundle() {
      if (this.environment.name !== "client") return;
      const lines = [
        "/*",
        `  Content-Security-Policy: ${csp}`,
        "  X-Content-Type-Options: nosniff",
        "  Referrer-Policy: strict-origin-when-cross-origin",
        "  Permissions-Policy: camera=(), microphone=(), geolocation=()",
        // 檔名帶雜湊的 JS／CSS／字型：一年、不再驗證。
        "/_app/immutable/*",
        "  Cache-Control: public, max-age=31536000, immutable",
        // 搜尋索引沒有雜湊檔名：短快取，過期後用 ETag 驗證。
        "/search/*",
        "  Cache-Control: public, max-age=300, must-revalidate",
        // service worker 的腳本：每次都向伺服器確認（瀏覽器檢查更新時才拿得到新版）。
        // 同名標頭在多條規則會串接，先拆掉預設值再設（規則要放在較廣的規則後面）。
        "/service-worker.js",
        "  ! Cache-Control",
        "  Cache-Control: no-cache",
        "/manifest.webmanifest",
        "  ! Content-Type",
        "  Content-Type: application/manifest+json",
        "",
      ];
      // HTML 不另外設：Workers 靜態檔預設 public, max-age=0, must-revalidate（每次用 ETag 驗證）。
      this.emitFile({ type: "asset", fileName: "_headers", source: lines.join("\n") });
    },
  };
}

export default defineConfig({
  publicDir: SITE_BUILD,
  plugins: [docsHeaders(), cloudflare()],
  // cf dev（vite dev）與 vite preview 的埠；SvelteKit 自己的 dev server 是 4330。
  server: { port: 4333, strictPort: true },
  preview: { port: 4333, strictPort: true },
});
