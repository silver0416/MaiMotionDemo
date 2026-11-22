/**
 * 章節與頁面清單。章節的 id、名稱、順序固定在這裡；頁面由 src/content/<資料夾>/<slug>.md 的 frontmatter 自動產生，
 * 網址是 /<資料夾>/<slug>。一章可以涵蓋好幾個內容資料夾（`dirs`，例如「開始使用」＝start＋charts），
 * 合併章節時網址不變。同一章依 `order` 排，不分資料夾（「開始使用」的 start 與 charts 兩個資料夾共用一組編號，可以穿插）；
 * order 相同或沒寫時才依 `dirs` 的順序、再依檔名（沒寫的排最後）。
 * 某一章暫時沒有頁面也沒關係，目錄只是不列出來。
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

export type SectionId = 'intro' | 'quick' | 'start' | 'play' | 'annotate' | 'model' | 'more';

/**
 * 章節的分群，決定章節標題下劃線的顏色（側欄、選單、首頁一致）：
 * guide＝入門導覽（右手藍），detail＝細節（左手粉），讓讀者看出「從這裡開始是細節」。
 */
export type SectionTone = 'guide' | 'detail';

/** 入門導覽（guide）和後面各章（detail）之間的分隔標示：側欄、選單與首頁章節清單在第一個 detail 章節前面顯示這幾個字。 */
export const MANUAL_LABEL = '完整手冊';

/** 這一章是不是「完整手冊」的第一章（前一章還是入門導覽）：是的話在它前面放分隔標示。 */
export function startsManual(sections: readonly Section[], index: number): boolean {
  return index > 0 && sections[index].tone === 'detail' && sections[index - 1].tone === 'guide';
}

export interface Section {
  id: SectionId;
  title: string;
  /** 首頁章節清單上的一句話。 */
  blurb: string;
  tone: SectionTone;
  /** 這一章涵蓋的內容資料夾（src/content/<資料夾>，也是網址的第一段），依序排列；沒寫就是只有和 id 同名的那一個。 */
  dirs?: readonly string[];
}

// 每一章寫成一行：scripts/build-index.mjs 逐行讀 id、title、dirs（搜尋索引的章節名），不要拆成多行。
export const SECTIONS: readonly Section[] = [
  { id: 'intro', title: '認識 MaiMotionDemo', tone: 'guide', blurb: '它在做什麼、怎麼推算手順，以及為什麼需要你的標註。' },
  { id: 'quick', title: '基礎操作演示', tone: 'guide', blurb: '第一次用？跟著實際畫面一步一步做，只教最基本的。' },
  { id: 'start', title: '開始使用', tone: 'detail', dirs: ['start', 'charts'], blurb: '安裝 App、認識介面、搜尋或貼上譜面，再播放看兩隻手怎麼動。' },
  { id: 'play', title: '播放與檢視', tone: 'detail', blurb: '播放、逐格與循環片段，看懂盤面上的每個記號。' },
  { id: 'annotate', title: '真人手順標註', tone: 'detail', blurb: '自己標出左右手，對照影片，和模型的建議比一比。' },
  { id: 'model', title: '模型原理', tone: 'detail', blurb: '各種打法怎麼比較、為什麼會換手，以及可以調整的偏好。' },
  { id: 'more', title: '其他', tone: 'detail', blurb: '常見問題、設定與其他說明。' },
];

/** 這一章涵蓋的內容資料夾。 */
export function sectionDirs(section: Section): readonly string[] {
  return section.dirs ?? [section.id];
}

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
  /** true：標題旁放會走動的吉祥物（RoamingLogo）。 */
  mascot?: boolean;
  headings?: Heading[];
}

export interface Page {
  /** 所屬的章節。 */
  section: SectionId;
  /** 內容資料夾（網址第一段）；章節涵蓋多個資料夾時和 section 不一定相同。 */
  dir: string;
  slug: string;
  url: string;
  title: string;
  description: string;
  order: number;
  keywords: string[];
  mascot: boolean;
  headings: Heading[];
}

// 只取 frontmatter（?meta 由 markdown.js 的 markdownMeta 產生）：直接 import .md 的 metadata 會把每一頁的內容都打包進來。
const modules = import.meta.glob<PageMetadata>('/src/content/**/*.md', { eager: true, query: '?meta', import: 'default' });

/** 內容資料夾 → 所屬章節、章節的先後（chapter），以及資料夾在整本手冊裡的先後（rank：章節順序，再依章內 dirs 的順序）。 */
const DIRS = new Map<string, { section: SectionId; chapter: number; rank: number }>();
for (const section of SECTIONS) {
  for (const dir of sectionDirs(section)) DIRS.set(dir, { section: section.id, chapter: SECTIONS.indexOf(section), rank: DIRS.size });
}

function build(): Page[] {
  const list: Page[] = [];
  for (const [file, meta] of Object.entries(modules)) {
    const match = /^\/src\/content\/([^/]+)\/([^/?]+)\.md(?:\?meta)?$/.exec(file);
    const owner = match ? DIRS.get(match[1]) : undefined;
    if (!match || !owner) {
      if (import.meta.env.DEV) console.warn(`[nav] ${file} 不在任何章節底下，不會列出`);
      continue;
    }
    const [, dir, slug] = match;
    list.push({
      section: owner.section,
      dir,
      slug,
      url: `/${dir}/${slug}`,
      title: meta?.title ?? slug,
      description: meta?.description ?? '',
      order: typeof meta?.order === 'number' ? meta.order : Number.POSITIVE_INFINITY,
      keywords: Array.isArray(meta?.keywords) ? meta.keywords : [],
      mascot: meta?.mascot === true,
      headings: Array.isArray(meta?.headings) ? meta.headings : [],
    });
  }
  return list.sort(
    (a, b) =>
      (DIRS.get(a.dir)?.chapter ?? 0) - (DIRS.get(b.dir)?.chapter ?? 0) ||
      // Infinity - Infinity 是 NaN：兩頁都沒寫 order 時要當成相等，才會往下比資料夾與檔名。
      (a.order === b.order ? 0 : a.order - b.order) ||
      (DIRS.get(a.dir)?.rank ?? 0) - (DIRS.get(b.dir)?.rank ?? 0) ||
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

/** 依網址的兩段（內容資料夾、slug）找頁面，例如 findPage('charts', 'new') → /charts/new。 */
export function findPage(dir: string, slug: string): Page | undefined {
  return PAGES.find((page) => page.dir === dir && page.slug === slug);
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
