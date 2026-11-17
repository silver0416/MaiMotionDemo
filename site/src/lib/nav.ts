/**
 * 章節與頁面清單。章節的 id、名稱、順序固定在這裡；頁面由 src/content/<section>/<slug>.md 的 frontmatter 自動產生，
 * 同一章依 `order` 排（沒寫的排最後，再依檔名）。某一章暫時沒有頁面也沒關係，目錄只是不列出來。
 */

export const SITE = {
  name: 'MaiMotionDemo 使用手冊',
  shortName: 'MaiMotionDemo',
  origin: 'https://maimotiondemo.bapu.dev',
  /** 站內的下載頁（直接連 GitHub 的檔案）；所有版本見 src/lib/release/core.ts 的 RELEASES_URL。 */
  download: '/download',
  github: 'https://github.com/silver0416/MaiMotionDemo',
  /** 手冊對應的 App 版本（建置時讀 repo 根目錄的 package.json）。 */
  appVersion: __APP_VERSION__,
} as const;

export type SectionId = 'intro' | 'quick' | 'start' | 'charts' | 'play' | 'model' | 'annotate' | 'more';

export interface Section {
  id: SectionId;
  title: string;
  /** 首頁章節清單上的一句話。 */
  blurb: string;
}

export const SECTIONS: readonly Section[] = [
  { id: 'intro', title: '認識 MaiMotionDemo', blurb: '它在做什麼、怎麼推算手順，以及為什麼需要你的標註。' },
  { id: 'quick', title: '超簡易上手', blurb: '第一次用？跟著實際畫面一步一步做，只教最基本的。' },
  { id: 'start', title: '開始使用', blurb: '安裝 App，貼上第一段譜面，看兩隻手怎麼動。' },
  { id: 'charts', title: '匯入譜面', blurb: '從 Majdata、simai Wiki 搜尋，或自己貼上 simai。' },
  { id: 'play', title: '播放與檢視', blurb: '播放、逐格與循環片段，看懂盤面上的每個記號。' },
  { id: 'model', title: '模型怎麼分左右手', blurb: '各種打法怎麼比較、為什麼會換手，以及可以調整的偏好。' },
  { id: 'annotate', title: '真人手順標註', blurb: '自己標出左右手，對照影片，和模型的建議比一比。' },
  { id: 'more', title: '其他', blurb: '常見問題、設定與其他說明。' },
];

export interface Heading {
  depth: 2 | 3;
  text: string;
  id: string;
}

/** .md 的 frontmatter（headings 由建置管線自動加上）。 */
export interface PageMetadata {
  title?: string;
  description?: string;
  order?: number;
  keywords?: string[];
  headings?: Heading[];
}

export interface Page {
  section: SectionId;
  slug: string;
  url: string;
  title: string;
  description: string;
  order: number;
  keywords: string[];
  headings: Heading[];
}

// 只取 frontmatter（?meta 由 markdown.js 的 markdownMeta 產生）：直接 import .md 的 metadata 會把每一頁的內容都打包進來。
const modules = import.meta.glob<PageMetadata>('/src/content/**/*.md', { eager: true, query: '?meta', import: 'default' });

const SECTION_IDS = new Set<string>(SECTIONS.map((section) => section.id));

function build(): Page[] {
  const list: Page[] = [];
  for (const [file, meta] of Object.entries(modules)) {
    const match = /^\/src\/content\/([^/]+)\/([^/?]+)\.md(?:\?meta)?$/.exec(file);
    if (!match || !SECTION_IDS.has(match[1])) {
      if (import.meta.env.DEV) console.warn(`[nav] ${file} 不在任何章節底下，不會列出`);
      continue;
    }
    const [, section, slug] = match;
    list.push({
      section: section as SectionId,
      slug,
      url: `/${section}/${slug}`,
      title: meta?.title ?? slug,
      description: meta?.description ?? '',
      order: typeof meta?.order === 'number' ? meta.order : Number.POSITIVE_INFINITY,
      keywords: Array.isArray(meta?.keywords) ? meta.keywords : [],
      headings: Array.isArray(meta?.headings) ? meta.headings : [],
    });
  }
  const sectionOrder = new Map(SECTIONS.map((section, index) => [section.id, index]));
  return list.sort(
    (a, b) =>
      (sectionOrder.get(a.section) ?? 0) - (sectionOrder.get(b.section) ?? 0) ||
      a.order - b.order ||
      a.slug.localeCompare(b.slug),
  );
}

/** 全部頁面，依章節、order 排好。 */
export const PAGES: readonly Page[] = build();

export function pagesIn(section: SectionId): Page[] {
  return PAGES.filter((page) => page.section === section);
}

/** 有頁面的章節（目錄只列這些）。 */
export function sectionsWithPages(): { section: Section; pages: Page[] }[] {
  return SECTIONS.map((section) => ({ section, pages: pagesIn(section.id) })).filter((group) => group.pages.length > 0);
}

export function sectionById(id: string): Section | undefined {
  return SECTIONS.find((section) => section.id === id);
}

export function findPage(section: string, slug: string): Page | undefined {
  return PAGES.find((page) => page.section === section && page.slug === slug);
}

/** 上一頁／下一頁：跨章節依整本手冊的順序。 */
export function neighbors(page: Page): { prev?: Page; next?: Page } {
  const index = PAGES.findIndex((item) => item.url === page.url);
  if (index < 0) return {};
  return { prev: PAGES[index - 1], next: PAGES[index + 1] };
}

/** 去掉結尾斜線，讓 /start/install/ 與 /start/install 都對得到。 */
export function normalizePath(pathname: string): string {
  return pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname;
}
