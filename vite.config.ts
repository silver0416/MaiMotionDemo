import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// 前端原始碼放在 frontend/，建置輸出到專案根目錄的 dist/
// （src-tauri/tauri.conf.json 的 frontendDist 指向 ../dist）。
export default defineConfig({
  root: 'frontend',
  publicDir: false,
  clearScreen: false,
  plugins: [svelte()],
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
