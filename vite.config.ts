import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

function git(args: string): string {
  try {
    return execSync(`git ${args}`, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
  } catch {
    return '';
  }
}

/**
 * 建置資訊在 vite 啟動或建置時固定下來，設定頁與錯誤紀錄用來辨識是哪一份建置。
 * build 號 = commit 數 + 短 hash；工作目錄有未提交修改時加上 -dirty。
 */
function buildInfo() {
  const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')) as {
    version: string;
  };
  const count = git('rev-list --count HEAD');
  const hash = git('rev-parse --short HEAD');
  const dirty = git('status --porcelain') ? '-dirty' : '';
  return {
    version: pkg.version,
    build: count && hash ? `${count}.${hash}${dirty}` : 'unknown',
    builtAt: new Date().toISOString(),
  };
}

const info = buildInfo();

// 前端原始碼放在 frontend/，建置輸出到專案根目錄的 dist/
// （src-tauri/tauri.conf.json 的 frontendDist 指向 ../dist）。
export default defineConfig({
  root: 'frontend',
  publicDir: false,
  clearScreen: false,
  plugins: [svelte()],
  define: {
    __APP_VERSION__: JSON.stringify(info.version),
    __BUILD_ID__: JSON.stringify(info.build),
    __BUILD_TIME__: JSON.stringify(info.builtAt),
  },
  server: {
    port: 1420,
    strictPort: true,
    // fixtures/ 與 resource/ 位於 Vite root 之外，開發伺服器需要放行專案根目錄。
    fs: { allow: ['..'] },
  },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'es2022',
    assetsInlineLimit: 0,
    chunkSizeWarningLimit: 1200,
  },
});
