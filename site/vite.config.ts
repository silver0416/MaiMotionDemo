import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import { autoImport, markdown, markdownMeta } from './markdown.js';
import {
  LATEST_API_URL,
  LATEST_PAGE_URL,
  parseRelease,
  releaseFromApi,
  releaseFromTag,
  tagFromLocation,
  type ReleaseInfo,
} from './src/lib/release/core.ts';

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url)).replace(/\\/g, '/');

/** 手冊對應的 App 版本：建置時讀 repo 根目錄的 package.json。 */
const appVersion: string = JSON.parse(readFileSync(here('../package.json'), 'utf8')).version;

/**
 * 最新版本的建置時快照（__RELEASE_SNAPSHOT__）：沒有 JS、Worker 掛掉時，下載頁與首頁也有可用的下載連結。
 * 和 Worker 的 /api/release 同一套規則（src/lib/release/core.ts）：GitHub API → /releases/latest 的轉址 → package.json 的版本。
 * 設 DOCS_RELEASE_OFFLINE=1 可跳過網路，直接用 package.json。
 * GitHub 拿到的結果在 .cache/release.json 留 10 分鐘：check、dev、build 連著跑不會一直打未認證的 API（每小時 60 次）。
 */
function releaseSnapshot(): Plugin {
  const headers = { 'User-Agent': 'maimotiondemo-docs-build' };
  const cacheFile = here('./.cache/release.json');
  const CACHE_MS = 10 * 60_000;

  function readCached(): ReleaseInfo | null {
    try {
      if (!existsSync(cacheFile) || Date.now() - statSync(cacheFile).mtimeMs > CACHE_MS) return null;
      return parseRelease(JSON.parse(readFileSync(cacheFile, 'utf8')));
    } catch {
      return null;
    }
  }

  async function fetchSnapshot(): Promise<ReleaseInfo | null> {
    if (process.env.DOCS_RELEASE_OFFLINE === '1') return null;
    const cached = readCached();
    if (cached) return cached;
    const info = await fetchUpstream();
    if (info) {
      mkdirSync(here('./.cache'), { recursive: true });
      writeFileSync(cacheFile, JSON.stringify(info));
    }
    return info;
  }

  async function fetchUpstream(): Promise<ReleaseInfo | null> {
    try {
      const response = await fetch(LATEST_API_URL, {
        headers: { ...headers, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' },
        signal: AbortSignal.timeout(8000),
      });
      if (response.ok) {
        const info = releaseFromApi(await response.json());
        if (info) return info;
      } else {
        console.warn(`[release] GitHub API ${response.status}，改用轉址取 tag`);
      }
    } catch (error) {
      console.warn(`[release] GitHub API 失敗（${String(error)}），改用轉址取 tag`);
    }
    try {
      const response = await fetch(LATEST_PAGE_URL, { headers, redirect: 'manual', signal: AbortSignal.timeout(8000) });
      const tag = tagFromLocation(response.headers.get('Location'));
      if (tag) return releaseFromTag(tag, 'redirect');
    } catch (error) {
      console.warn(`[release] 轉址也失敗（${String(error)}）`);
    }
    return null;
  }
  return {
    name: 'docs-release-snapshot',
    async config() {
      const info = (await fetchSnapshot()) ?? releaseFromTag(`v${appVersion}`, 'package');
      console.log(info ? `[release] 建置時快照：${info.tag}（來源 ${info.source}）` : '[release] 沒有版本資料');
      return { define: { __RELEASE_SNAPSHOT__: JSON.stringify(info) } };
    },
  };
}

/**
 * 路徑別名。SvelteKit 自己也登記了 $app（$app/navigation、$app/state… 的實作），
 * 直接寫在 resolve.alias 會排在它後面而永遠比對不到；改由 order: 'post' 的 config 鉤子回傳，
 * Vite 合併設定時後到的別名排在前面。
 */
function aliases(): Plugin {
  return {
    name: 'docs-aliases',
    config: {
      order: 'post',
      handler: () => ({
        resolve: {
          alias: [
            // App 的 tokens、元件與工具函式（只讀，不改）。只轉 App 自己的子資料夾，
            // SvelteKit 內建的 $app/navigation、$app/state、$app/paths… 不受影響。
            { find: /^\$app\/(components|lib|state|styles|video)\//, replacement: `${here('../frontend/src')}/$1/` },
            // SvelteKit 3 拿掉了內建的 $lib（改推 #lib 子路徑匯入）；文件站沿用 $lib，在這裡自己接回 src/lib。
            { find: /^\$lib\//, replacement: `${here('./src/lib')}/` },
          ],
        },
      }),
    },
  };
}

export default defineConfig({
  plugins: [
    // 目錄用的 frontmatter（xxx.md?meta），見 markdown.js。
    markdownMeta(),
    // SvelteKit 3 不再讀 svelte.config.js（有這個檔會直接報錯），設定都放這裡。
    sveltekit({
      extensions: ['.svelte', '.md'],
      // 順序：先注入 import → mdsvex 轉成 Svelte → 再處理 <script lang="ts"> 與樣式。
      preprocess: [autoImport(), markdown(), vitePreprocess()],
      adapter: adapter({ fallback: '404.html' }),
      // service worker（src/service-worker/）：不自動註冊，由 src/lib/pwa/register.ts 只在手機註冊。
      serviceWorker: { register: false },
      prerender: {
        // 站內連到不存在的頁面或段落（錨點）一律讓建置失敗，避免壞連結上線。
        handleHttpError: ({ status, path, referrer, message }) => {
          if (status === 404) {
            throw new Error(`[prerender] 找不到 ${path}（從 ${referrer ?? '?'} 連過去）`);
          }
          throw new Error(message);
        },
        handleMissingId: 'fail',
      },
    }),
    aliases(),
    releaseSnapshot(),
  ],
  resolve: {
    // App 的檔案在 site/ 外面，強制共用 site/ 的 Svelte，避免出現兩份執行環境。
    dedupe: ['svelte'],
  },
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
    // 緊急開關：PWA_KILL_SWITCH=1 建置出的 service worker 會清空快取並取消註冊，頁面也不再註冊（見 README「Web App（PWA）」）。
    __PWA_KILL_SWITCH__: JSON.stringify(process.env.PWA_KILL_SWITCH === '1'),
  },
  server: {
    port: 4330,
    strictPort: true,
    fs: { allow: ['..'] },
  },
});
