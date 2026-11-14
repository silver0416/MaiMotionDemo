// src/content/**/*.md 經 mdsvex 轉成 Svelte 元件，並匯出 frontmatter（含建置時加上的 headings）。
declare module '*.md' {
  import type { Component } from 'svelte';
  import type { PageMetadata } from '$lib/nav';

  export const metadata: PageMetadata;
  const component: Component;
  export default component;
}
