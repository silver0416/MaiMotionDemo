import { error } from '@sveltejs/kit';
import type { Component } from 'svelte';
import { findPage, neighbors, PAGES, sectionById } from '$lib/nav';
import type { EntryGenerator, PageLoad } from './$types';

// 內容（延遲載入：每一頁只帶自己的 .md）。
const contents = import.meta.glob<{ default: Component }>('/src/content/**/*.md');

/** 預先算繪的頁面清單：src/content 底下所有登記在章節裡的 .md。 */
export const entries: EntryGenerator = () => PAGES.map((page) => ({ section: page.section, slug: page.slug }));

export const load: PageLoad = async ({ params }) => {
  const page = findPage(params.section, params.slug);
  const loader = contents[`/src/content/${params.section}/${params.slug}.md`];
  if (!page || !loader) error(404, '找不到這一頁');
  const module = await loader();
  const { prev, next } = neighbors(page);
  return {
    page,
    section: sectionById(page.section)!,
    content: module.default,
    prev,
    next,
  };
};
