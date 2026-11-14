import { PAGES, SITE } from '$lib/nav';
import type { RequestHandler } from './$types';

// 建置時產生 sitemap.xml：首頁、下載頁＋所有內容頁。
export const prerender = true;

export const GET: RequestHandler = () => {
  const urls = ['/', SITE.download, ...PAGES.map((page) => page.url)];
  const body =
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
    urls.map((url) => `  <url><loc>${SITE.origin}${encodeURI(url)}</loc></url>`).join('\n') +
    '\n</urlset>\n';
  return new Response(body, { headers: { 'Content-Type': 'application/xml; charset=utf-8' } });
};
