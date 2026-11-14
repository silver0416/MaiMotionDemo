import { bindings, defineConfig } from "cf/config";

// 文件站：靜態頁（SvelteKit 的輸出 site/build，經 vite.config.ts 的 publicDir 帶進來）＋語意搜尋 API。
// 靜態檔不經過 Worker（不算 Worker 請求）；Worker 只收到沒對到檔案的請求：/api/search 與 404。
export default defineConfig({
  worker: {
    name: "maimotiondemo-docs",
    compatibilityDate: "2026-10-20",
    entrypoint: "./src/index.ts",
    domains: ["maimotiondemo.bapu.dev"],
    // 只走自訂網域：caches.default 在 workers.dev 上不生效，也不需要多一個公開網址。
    workersDev: false,
    previewUrls: false,
    assets: {
      // /start/install 會對到 start/install.html；沒對到的交給 Worker，Worker 再交回 assets 時回 404.html（狀態 404）。
      notFoundHandling: "404-page",
    },
    observability: {
      enabled: true,
      // /api/search?q= 的查詢字不必留在紀錄裡。
      redactQueryString: true,
      logs: { enabled: true, invocationLogs: true },
    },
    env: {
      ASSETS: bindings.assets(),
      // Workers AI 沒有本機模擬：本機開發也連到真的 Workers AI（會用到帳號額度）。
      AI: bindings.ai({ dev: { remote: true } }),
      // 每個來源 IP 每 10 秒 20 次。輸入停 250 ms 才查一次，正常打字遠低於這個量。
      LIMIT: bindings.rateLimit({ namespace: "4331", simple: { limit: 20, period: 10 } }),
    },
  },
});
