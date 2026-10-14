import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { defineConfig, type Plugin } from 'vite';
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

/**
 * onnxruntime-web 的 WebGPU 版會以 new URL('ort-wasm-…wasm', import.meta.url) 參照 25 MB 的 WebAssembly，
 * Vite 建置時會把它複製進 dist（進而包進 exe）。手元影片預填一律以 env.wasm.wasmBinary 提供
 * 使用者手動下載的檔案，所以把這個參照換掉，不打包該檔。
 */
function excludeOrtWasm(): Plugin {
  return {
    name: 'maimotion-exclude-ort-wasm',
    enforce: 'pre',
    transform(code, id) {
      if (!id.includes('onnxruntime-web')) return null;
      const replaced = code.replace(
        /new URL\(\s*["']ort-wasm-simd-threaded[^"']*\.wasm["']\s*,\s*import\.meta\.url\s*\)/g,
        'new URL("about:blank")',
      );
      return replaced === code ? null : { code: replaced, map: null };
    },
  };
}

// 前端原始碼放在 frontend/，建置輸出到專案根目錄的 dist/
// （src-tauri/tauri.conf.json 的 frontendDist 指向 ../dist）。
export default defineConfig({
  root: 'frontend',
  publicDir: false,
  clearScreen: false,
  plugins: [excludeOrtWasm(), svelte()],
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
