import type { Component } from 'svelte';

/**
 * 所有演示元件的延遲載入函式（路徑相對於 src/lib/demos/）。
 * 這個模組由 Demo.svelte 以 import() 載入，不要改成靜態 import：
 * SvelteKit 會把頁面「靜態引用的模組」裡第一層 import() 的 CSS 全部放進 <head>（render-blocking），
 * 首頁直接用 <Demo>，若 glob 寫在 Demo.svelte 裡，首頁會多載十幾個用不到的演示 CSS。隔一層之後只在真的載入演示時才抓。
 */
export const loaders = import.meta.glob<{ default: Component<Record<string, unknown>> }>([
  './**/*.svelte',
  '!./Demo.svelte',
]);
