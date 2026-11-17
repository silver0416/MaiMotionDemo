// 內容（src/content/**/*.md）的建置管線：自動注入 import、標題錨點、表格、程式碼區塊、外部連結。
// SvelteKit 3 不再讀 svelte.config.js，設定一律經由 vite.config.ts 的 sveltekit(...)；這裡集中 Markdown 相關的部分。
import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { mdsvex } from 'mdsvex';

/** 每個 .md 都能直接用的元件（內容檔不寫 import）。 */
const AUTO_IMPORTS = [
  "import Demo from '$lib/demos/Demo.svelte';",
  "import Callout from '$lib/md/Callout.svelte';",
];

/**
 * 有專用元件資料夾的章節：src/content/<章節>/ 底下的頁面另外自動 import src/lib/<章節>/ 裡的每個元件（檔名就是標籤名）。
 * - intro（認識 MaiMotionDemo）：Steps、LinkRows、Mascot…
 * - quick（基礎操作演示）：QuickTour（實際畫面導覽）
 */
const SECTION_COMPONENTS = ['intro', 'quick'];

/**
 * 借用 intro 的 Mascot（吉祥物對話框）的章節；這些章節不必有自己的元件資料夾。
 * - quick：每頁最後一句由吉祥物講
 * - start：〈下載與啟動〉開頭那句
 */
const MASCOT_IMPORT = "import Mascot from '$lib/intro/Mascot.svelte';";
const SECTION_EXTRA_IMPORTS = {
  quick: [MASCOT_IMPORT],
  start: [MASCOT_IMPORT],
};

/**
 * 每次轉換都重新列出資料夾：新增元件不必改這裡；刪掉某個元件（例如實驗性的 Mascot）也不會留下壞掉的 import。
 * @param {string} filename
 */
function sectionImports(filename) {
  const match = /[\\/]src[\\/]content[\\/]([^\\/]+)[\\/]/.exec(filename);
  const section = match?.[1];
  if (!section) return [];
  const extra = SECTION_EXTRA_IMPORTS[section] ?? [];
  if (!SECTION_COMPONENTS.includes(section)) return extra;
  let files = [];
  try {
    files = readdirSync(fileURLToPath(new URL(`./src/lib/${section}/`, import.meta.url)));
  } catch {
    return extra;
  }
  return [
    ...files
      .filter((file) => /^[A-Z][A-Za-z0-9]*\.svelte$/.test(file))
      .map((file) => `import ${file.slice(0, -'.svelte'.length)} from '$lib/${section}/${file}';`),
    ...extra,
  ];
}

/**
 * markup preprocessor，排在 mdsvex 前面：在 .md 裡加上 Demo／Callout（intro、quick 章節再加各自資料夾的元件）的 import。
 * 已經有 instance <script> 就塞進去（Svelte 不允許兩個），沒有就放在 frontmatter 後面。
 * @returns {import('svelte/compiler').PreprocessorGroup}
 */
export function autoImport() {
  return {
    name: 'docs-auto-import',
    markup({ content, filename }) {
      if (!filename?.endsWith('.md')) return;
      const imports = [...AUTO_IMPORTS, ...sectionImports(filename)].join('\n');
      const instance = /<script(?![^>]*\bmodule\b)(?![^>]*context=["']module["'])[^>]*>/;
      const match = instance.exec(content);
      if (match) {
        const at = match.index + match[0].length;
        return { code: `${content.slice(0, at)}\n${imports}\n${content.slice(at)}` };
      }
      const fm = /^---\r?\n[\s\S]*?\r?\n---\r?\n/.exec(content);
      const at = fm ? fm[0].length : 0;
      return { code: `${content.slice(0, at)}<script>\n${imports}\n</script>\n\n${content.slice(at)}` };
    },
  };
}

// ---------------------------------------------------------------------------
// 共用小工具

/** mdsvex 在 remark 階段已把行內程式碼的 < > { } 換成實體；算標題文字時要換回來。 */
function decodeEntities(text) {
  return text
    .replace(/&#123;/g, '{')
    .replace(/&#125;/g, '}')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

function escapeHtml(text) {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/\{/g, '&#123;')
    .replace(/\}/g, '&#125;')
    .replace(/`/g, '&#96;');
}

/**
 * 標題 → 錨點 id：保留中日文、英數，空白與其他符號換成 -。
 * 例：「兩隻手為什麼會互換？」→「兩隻手為什麼會互換」；「Slide 的 1-5 寫法」→「slide-的-1-5-寫法」。
 */
export function slugify(text) {
  return text
    .normalize('NFKC')
    .toLowerCase()
    .replace(/[^\p{Letter}\p{Number}]+/gu, '-')
    .replace(/^-+|-+$/g, '');
}

function mdastText(node) {
  if (node.type === 'text' || node.type === 'inlineCode') return decodeEntities(node.value ?? '');
  if (node.type === 'html') return '';
  return (node.children ?? []).map(mdastText).join('');
}

function hastText(node) {
  if (node.type === 'text') return node.value ?? '';
  return (node.children ?? []).map(hastText).join('');
}

function walk(node, visit, parent = null, index = 0) {
  visit(node, parent, index);
  const children = node.children;
  if (!children) return;
  for (let i = 0; i < children.length; i += 1) walk(children[i], visit, node, i);
}

function classList(node) {
  const value = node.properties?.className;
  if (Array.isArray(value)) return value;
  if (typeof value === 'string') return value.split(/\s+/);
  return [];
}

// ---------------------------------------------------------------------------
// remark：## ／ ### 加 id，並把標題清單放進 metadata.headings（版面的「本頁內容」與搜尋用）

function remarkHeadings() {
  return (tree, file) => {
    const seen = new Map();
    const headings = [];
    walk(tree, (node) => {
      if (node.type !== 'heading' || (node.depth !== 2 && node.depth !== 3)) return;
      const text = mdastText(node).trim();
      let id = slugify(text) || 'section';
      const count = seen.get(id) ?? 0;
      seen.set(id, count + 1);
      if (count > 0) id = `${id}-${count + 1}`;
      node.data ??= {};
      node.data.hProperties = { ...(node.data.hProperties ?? {}), id };
      headings.push({ depth: node.depth, text, id });
    });
    file.data.fm ??= {};
    file.data.fm.headings = headings;
  };
}

// ---------------------------------------------------------------------------
// rehype：標題錨點、表格、外部連結

/** Lucide「link」，24×24、線寬 2；和 App 的 Icon.svelte 同一套外形。 */
const LINK_ICON = {
  type: 'element',
  tagName: 'svg',
  properties: {
    className: ['icon', 'icon-link'],
    width: 15,
    height: 15,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 2,
    strokeLinecap: 'round',
    strokeLinejoin: 'round',
    ariaHidden: 'true',
    focusable: 'false',
  },
  children: [
    'M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71',
    'M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71',
  ].map((d) => ({ type: 'element', tagName: 'path', properties: { d }, children: [] })),
};

/** 複製成功後換上的打勾（Lucide「check」）。 */
const LINK_CHECK_ICON = {
  ...LINK_ICON,
  properties: { ...LINK_ICON.properties, className: ['icon', 'icon-check'] },
  children: [{ type: 'element', tagName: 'path', properties: { d: 'M20 6 9 17l-5-5' }, children: [] }],
};

function rehypeDocs() {
  return (tree) => {
    walk(tree, (node, parent, index) => {
      if (node.type !== 'element') return;

      // 標題：後面接一顆「複製這一段的連結」（鍵盤可聚焦，hover 才顯示）。
      // 它仍是指向 #id 的連結（沒有 JS、按右鍵複製連結、開新分頁都照常），點擊由 +layout.svelte 接手：
      // 複製完整網址、更新網址列，不捲動頁面。
      if ((node.tagName === 'h2' || node.tagName === 'h3') && node.properties?.id) {
        const id = String(node.properties.id);
        const text = hastText(node).trim();
        node.children.push({
          type: 'element',
          tagName: 'a',
          properties: {
            className: ['heading-anchor'],
            href: `#${id}`,
            ariaLabel: `複製「${text}」這一段的連結`,
            title: '複製連結',
          },
          children: [structuredClone(LINK_ICON), structuredClone(LINK_CHECK_ICON)],
        });
        return;
      }

      // 表格：外包 .table-wrap（容器窄時改直排），每格帶上同欄的欄名。
      if (node.tagName === 'table' && parent && !classList(parent).includes('table-wrap')) {
        node.properties ??= {};
        node.properties.className = [...classList(node), 'table'];
        const labels = [];
        walk(node, (cell) => {
          if (cell.type === 'element' && cell.tagName === 'thead') {
            walk(cell, (th) => {
              if (th.type === 'element' && th.tagName === 'th') labels.push(hastText(th).trim());
            });
          }
        });
        walk(node, (row) => {
          if (row.type !== 'element' || row.tagName !== 'tr') return;
          let column = 0;
          for (const cell of row.children) {
            if (cell.type !== 'element') continue;
            if (cell.tagName === 'td' && labels[column]) {
              cell.properties ??= {};
              cell.properties.dataLabel = labels[column];
            }
            column += 1;
          }
        });
        parent.children[index] = {
          type: 'element',
          tagName: 'div',
          properties: { className: ['table-wrap'] },
          children: [node],
        };
        return;
      }

      // 外部連結：新分頁開，rel="noopener"；畫面上附一段只給輔助技術的提示。
      if (node.tagName === 'a' && typeof node.properties?.href === 'string') {
        const href = node.properties.href;
        if (/^https?:\/\//.test(href)) {
          node.properties.target = '_blank';
          node.properties.rel = ['noopener'];
          node.properties.className = [...classList(node), 'is-external'];
          node.children.push({
            type: 'element',
            tagName: 'span',
            properties: { className: ['sr-only'] },
            children: [{ type: 'text', value: '（在新分頁開啟）' }],
          });
        }
      }
    });
  };
}

// ---------------------------------------------------------------------------
// 程式碼區塊：不上色，等寬字原樣顯示，右上角一顆複製鈕（點擊由內容頁統一處理）。

const COPY_ICON =
  '<svg class="icon icon-copy" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M8 10a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2Z"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>' +
  '<svg class="icon icon-check" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M20 6 9 17l-5-5"/></svg>';

function plainHighlighter(code, lang) {
  const language = (lang ?? '').toLowerCase().replace(/[^a-z0-9-]/g, '');
  // 上方一列放語言名稱與複製鈕，程式碼在下方自己橫向捲動，不會被按鈕蓋住。
  return (
    `<div class="code-block"${language ? ` data-lang="${language}"` : ''}>` +
    `<div class="code-head"><span class="code-lang">${language}</span>` +
    `<button type="button" class="btn btn--round code-copy" aria-label="複製" title="複製">${COPY_ICON}</button></div>` +
    `<pre class="code"><code>${escapeHtml(code)}</code></pre>` +
    `</div>`
  );
}

export function markdown() {
  return mdsvex({
    extensions: ['.md'],
    smartypants: false,
    highlight: { highlighter: plainHighlighter },
    remarkPlugins: [remarkHeadings],
    rehypePlugins: [rehypeDocs],
  });
}

// ---------------------------------------------------------------------------
// `xxx.md?meta`：只取 frontmatter（含 headings），給目錄、sitemap 用。
// 直接 import .md 的 metadata 會把整頁內容一起打包進每一頁（三十幾頁約 400 kB），所以另外開一個只有資料的模組。
// 用同一條 mdsvex 管線算，headings 的 id 和內文裡的錨點保證一致。

const META_PREFIX = '\0docs-meta:';
// 虛擬模組的 id 不能以 .md 結尾，否則 vite-plugin-svelte 會把它當成 Svelte 元件再編譯一次。
const META_SUFFIX = '.meta.js';

/** @returns {import('vite').Plugin} */
export function markdownMeta() {
  const processor = markdown();
  return {
    name: 'docs-markdown-meta',
    enforce: 'pre',
    async resolveId(source, importer) {
      if (!source.endsWith('.md?meta')) return null;
      const resolved = await this.resolve(source.slice(0, -'?meta'.length), importer, { skipSelf: true });
      return resolved ? META_PREFIX + resolved.id + META_SUFFIX : null;
    },
    async load(id) {
      if (!id.startsWith(META_PREFIX)) return null;
      const file = id.slice(META_PREFIX.length, -META_SUFFIX.length);
      this.addWatchFile(file);
      const { readFile } = await import('node:fs/promises');
      const content = await readFile(file, 'utf8');
      const result = await processor.markup?.({ content, filename: file });
      const fm = (result && 'data' in result && result.data?.fm) || {};
      return `export default ${JSON.stringify(fm)};`;
    },
  };
}
