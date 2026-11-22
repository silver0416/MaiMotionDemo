// 搜尋索引：把 src/content/**/*.md 切塊、算 embedding，輸出 static/search/chunks.json 與 static/search/vectors.bin。
//
//   npm.cmd run index                 正常建置（用 EMBED_MODEL）
//   node scripts/build-index.mjs --model <id>   換模型（Worker 讀 vectors.bin 檔頭的 model 來算查詢向量，不用另外改；
//                                               但分數門檻是依模型定的，見 worker/src/index.ts 的 THRESHOLDS）
//   node scripts/build-index.mjs --dry          只看切塊結果，不呼叫模型、不寫檔
//
// 切塊：每個 `##` 一塊，`##` 之前的導言（加上頁面的 description）自成一塊。
// `###` 預設併在所屬的 `##` 裡；某個 `##` 的內文超過 SPLIT_CHARS 字時，才依 `###` 再切（每個 `###` 一塊，`###` 前的文字留在 `##` 那塊）。
// 錨點用 markdown.js 同一條 mdsvex 管線算出來（不是自己猜），保證和頁面上的 id 一致。
//
// embedding：`cf ai run <model> --body @檔案` 分批呼叫，結果依「模型＋輸入種類＋輸入文字」的雜湊快取在 .cache/embeddings/，
// 重跑只算改過的塊。任何一批重試後仍失敗就整個中止，不寫出半套索引。
//
// vectors.bin 格式（little-endian）：
//   0  char[4]  "MMVS"
//   4  u16      格式版本（1）
//   6  u16      量化方式（1 = int8，每個向量一個 float32 縮放：v[i] ≈ q[i] * scale）
//   8  u32      維度 dim
//  12  u32      向量數 count（順序與 chunks.json 的 chunks 相同）
//  16  u16      model 字串長度 m；接著 m bytes UTF-8
//      u16      version 字串長度 n；接著 n bytes UTF-8
//      u32      ids 長度 k；接著 k bytes UTF-8（各塊的 id，以 \n 分隔，Worker 回傳結果用）
//      補 0 到 4 的倍數
//      float32[count]      各向量的 scale
//      int8[count * dim]   量化後的向量（量化前已正規化成單位長度）

import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { markdown } from '../markdown.js';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
export const ROOT_DIR = ROOT;
const CONTENT = join(ROOT, 'src', 'content');
const OUT_DIR = join(ROOT, 'static', 'search');
const CACHE_DIR = join(ROOT, '.cache', 'embeddings');

/** 正式使用的模型（依 scripts/eval-search.mjs 的評估結果選定）。Worker 讀 vectors.bin 檔頭的 model，兩邊一定一致。 */
export const EMBED_MODEL = '@cf/baai/bge-m3';

/** `##` 內文超過這個字數、且底下有 `###` 時，才依 `###` 再切。 */
const SPLIT_CHARS = 900;
/** 送進模型的文字上限（字元）。中文約一字一 token，取 EmbeddingGemma 2048 token 上限以下、留足餘裕。 */
const MAX_INPUT_CHARS = 1500;

// ---------------------------------------------------------------------------
// 模型設定：各模型的輸入格式、查詢與文件的前綴、一批的上限。

/** @typedef {'document' | 'query'} Kind */

/**
 * @type {Record<string, { batch: number; body: (texts: string[], kind: Kind) => object; prepare?: (text: string, kind: Kind) => string }>}
 */
export const MODELS = {
  '@cf/baai/bge-m3': {
    batch: 50,
    // BGE-M3 的 dense 檢索不需要指令前綴。
    body: (texts) => ({ text: texts, truncate_inputs: true }),
  },
  '@cf/qwen/qwen3-embedding-0.6b': {
    batch: 32,
    // Qwen3-Embedding：查詢加指令（Workers AI 依 queries／documents 自動套用），文件不加。
    body: (texts, kind) => (kind === 'query' ? { queries: texts, instruction: QWEN_INSTRUCTION } : { documents: texts }),
  },
  '@cf/google/embeddinggemma-300m': {
    batch: 50,
    body: (texts) => ({ text: texts }),
    // EmbeddingGemma 的檢索提示詞（模型卡）：查詢「task: search result | query: 」，文件「title: … | text: 」。
    prepare: (text, kind) => {
      if (kind === 'query') return `task: search result | query: ${text}`;
      const [title, ...rest] = text.split('\n');
      return `title: ${title} | text: ${rest.join('\n')}`;
    },
  },
};

export const QWEN_INSTRUCTION = 'Given a question about the MaiMotionDemo app, retrieve the manual section that answers it';

// ---------------------------------------------------------------------------
// 讀內容

/**
 * 從 src/lib/nav.ts 讀「內容資料夾 → 章節名稱」（避免在這裡再寫一份）。
 * SECTIONS 一章一行：`{ id: '…', title: '…', …, dirs: ['…', '…'], … }`；沒有 dirs 時資料夾就是 id。
 * 一章可以涵蓋多個資料夾（「開始使用」＝start＋charts），兩個資料夾的頁面都記成同一個章節名。
 */
async function sectionTitles() {
  const source = await readFile(join(ROOT, 'src', 'lib', 'nav.ts'), 'utf8');
  const map = new Map();
  for (const [line] of source.matchAll(/^\s*\{\s*id:\s*'[^']+'.*$/gm)) {
    const id = /\bid:\s*'([^']+)'/.exec(line)?.[1];
    const title = /\btitle:\s*'([^']+)'/.exec(line)?.[1];
    if (!id || !title) continue;
    const dirs = /\bdirs:\s*\[([^\]]*)\]/.exec(line)?.[1];
    const folders = dirs ? [...dirs.matchAll(/'([^']+)'/g)].map((match) => match[1]) : [id];
    for (const folder of folders) map.set(folder, title);
  }
  return map;
}

/** 很小的 YAML 子集：key: value 與 key: [a, b]。 */
function parseFrontmatter(raw) {
  const data = {};
  for (const line of raw.split(/\r?\n/)) {
    const match = /^([A-Za-z_]+):\s*(.*)$/.exec(line);
    if (!match) continue;
    let value = match[2].trim();
    if (value.startsWith('[') && value.endsWith(']')) {
      data[match[1]] = value
        .slice(1, -1)
        .split(',')
        .map((item) => item.trim().replace(/^['"]|['"]$/g, ''))
        .filter(Boolean);
    } else {
      value = value.replace(/^['"]|['"]$/g, '');
      data[match[1]] = /^-?\d+(\.\d+)?$/.test(value) ? Number(value) : value;
    }
  }
  return data;
}

async function listMarkdown(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...(await listMarkdown(path)));
    else if (entry.name.endsWith('.md')) out.push(path);
  }
  return out.sort();
}

// ---------------------------------------------------------------------------
// Markdown → 純文字

/** 依未跳脫的 | 切表格列。 */
function splitRow(line) {
  let body = line.trim();
  if (body.startsWith('|')) body = body.slice(1);
  if (body.endsWith('|') && !body.endsWith('\\|')) body = body.slice(0, -1);
  const cells = [];
  let current = '';
  let inCode = false;
  for (let i = 0; i < body.length; i += 1) {
    const char = body[i];
    if (char === '\\' && body[i + 1] === '|') {
      current += '|';
      i += 1;
    } else if (char === '`') {
      inCode = !inCode;
      current += char;
    } else if (char === '|' && !inCode) {
      cells.push(current.trim());
      current = '';
    } else current += char;
  }
  cells.push(current.trim());
  return cells;
}

function decodeEntities(text) {
  return text
    .replace(/&nbsp;/g, ' ')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#123;/g, '{')
    .replace(/&#125;/g, '}')
    .replace(/&amp;/g, '&');
}

/** 一行 Markdown（行內）→ 純文字。行內程式碼原樣保留（只拿掉反引號）。 */
function inline(text) {
  const codes = [];
  // 先把行內程式碼收起來，免得裡面的 <HS*1.5>、* 之類被當成標記處理。
  let out = text.replace(/(`+)\s?([\s\S]*?)\s?\1/g, (_, _ticks, code) => {
    codes.push(code);
    return `\u0000${codes.length - 1}\u0000`;
  });
  out = out
    .replace(/<Demo\b[^>]*?\bcaption="([^"]*)"[^>]*\/?>/g, '$1')
    .replace(/<Demo\b[^>]*\/?>/g, '')
    .replace(/<Callout\b[^>]*?\btitle="([^"]*)"[^>]*>/g, '$1：')
    .replace(/<\/?[A-Za-z][^>]*>/g, '')
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/(\*\*|__)(.+?)\1/g, '$2')
    .replace(/(^|[^\w*])\*(?!\s)(.+?)\*(?!\w)/g, '$1$2')
    .replace(/\\([\\`*_{}[\]()#+\-.!|>])/g, '$1');
  out = decodeEntities(out);
  return out.replace(/\u0000(\d+)\u0000/g, (_, i) => codes[Number(i)]);
}

/** 一段 Markdown（多行）→ 純文字：表格轉成「欄名：值」的句子，程式碼區塊保留原文。 */
export function toPlain(markdownText) {
  const lines = markdownText.split(/\r?\n/);
  const out = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const fence = /^\s*(```+|~~~+)/.exec(line);
    if (fence) {
      const close = fence[1];
      const code = [];
      i += 1;
      while (i < lines.length && !lines[i].trim().startsWith(close)) code.push(lines[i++]);
      if (code.length) out.push(code.join('\n'));
      continue;
    }
    if (/^\s*\|/.test(line) && i + 1 < lines.length && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1])) {
      const labels = splitRow(line).map(inline);
      i += 2;
      while (i < lines.length && /^\s*\|/.test(lines[i])) {
        const cells = splitRow(lines[i]).map(inline);
        const parts = [];
        let lead = '';
        cells.forEach((value, c) => {
          if (!value) return;
          const label = labels[c] ?? '';
          if (!label) {
            // 沒有欄名的欄（例如第一欄是列名）：當成這一句的開頭。
            if (c === 0) lead = value;
            else parts.push(value);
          } else parts.push(`${label}：${value}`);
        });
        out.push(`${lead ? `${lead}——` : ''}${parts.join('；')}。`);
        i += 1;
      }
      i -= 1;
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) continue; // 分隔線
    let text = line
      .replace(/^\s*#{1,6}\s+/, '')
      .replace(/^\s*>\s?/, '')
      .replace(/^\s*(?:[-*+]|\d+[.)])\s+/, '');
    text = inline(text).trim();
    if (text) out.push(text);
  }
  return out.join('\n').replace(/\n{2,}/g, '\n').trim();
}

// ---------------------------------------------------------------------------
// 切塊

/**
 * 不在 src/content 裡、但也要搜得到的頁面（src/routes 底下直接寫的頁）。
 * 每頁一塊：id 就是網址去掉開頭的斜線；text 是頁面上的重點（不必逐字，給字面比對與 embedding 用）。
 * 頁面文字改了，這裡也要跟著改。
 * @type {{ id: string; url: string; page: string; section: string; keywords: string[]; text: string }[]}
 */
const EXTRA_PAGES = [
  {
    id: 'download',
    url: '/download',
    page: '下載',
    section: '下載',
    keywords: ['下載', '安裝檔', '最新版', '最新版本', '新版', 'exe', 'zip', 'AppImage', 'deb', 'Windows', 'macOS', 'Mac', 'Linux', 'SHA-256', '檢查碼'],
    text: [
      '下載 MaiMotionDemo 最新版：Windows（x64 的 exe）、macOS（universal 的 zip）、Linux（AppImage 與 deb）。免安裝的桌面程式，下載後直接開啟，檔案直接從 GitHub 下載。',
      '下載頁會依你的系統把對應的檔案放在最上面；每個平台列出檔名、大小與 SHA-256。手機與平板不能用，請在電腦上下載。',
      '第一次開啟：Windows 需要 WebView2 Runtime；macOS 沒有簽章，第一次要在 Finder 對程式按右鍵選「打開」；AppImage 要先加上執行權限；deb 用系統的套件工具安裝。程式內的自動更新只支援 Windows，其他平台有新版時回到下載頁下載。',
      '舊版本與每一版的更新內容在 GitHub 的發布頁（所有版本）。',
    ].join('\n'),
  },
];

/**
 * subs：併進這塊的 ###（heading、anchor、在 text 裡的起點 at）。
 * @typedef {{ id: string; url: string; page: string; section: string; headings: string[]; text: string; keywords: string[]; subs?: { heading: string; anchor: string; at: number }[] }} Chunk
 */

/** @returns {Promise<Chunk[]>} */
export async function collectChunks() {
  const titles = await sectionTitles();
  const processor = markdown();
  /** @type {Chunk[]} */
  const chunks = [];

  for (const file of await listMarkdown(CONTENT)) {
    const rel = relative(CONTENT, file).replace(/\\/g, '/');
    const match = /^([^/]+)\/([^/]+)\.md$/.exec(rel);
    if (!match || !titles.has(match[1])) {
      console.warn(`[index] 略過 ${rel}（不在任何章節底下）`);
      continue;
    }
    // section：內容資料夾（網址第一段）；章節名用 titles.get(section)。
    const [, section, slug] = match;
    const source = await readFile(file, 'utf8');
    const fm = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(source);
    const meta = fm ? parseFrontmatter(fm[1]) : {};
    const body = fm ? source.slice(fm[0].length) : source;
    const page = String(meta.title ?? slug);
    const keywords = Array.isArray(meta.keywords) ? meta.keywords.map(String) : [];
    const base = `/${section}/${slug}`;

    // 錨點：和頁面同一條 mdsvex 管線。
    const result = await processor.markup({ content: source, filename: file });
    const anchors = (result && 'data' in result && result.data?.fm?.headings) || [];

    // 依標題切行（略過程式碼區塊裡的 #）。
    /** @type {{ depth: number; text: string; lines: string[] }[]} */
    const parts = [{ depth: 0, text: '', lines: [] }];
    let fence = null;
    for (const line of body.split(/\r?\n/)) {
      const f = /^\s*(```+|~~~+)/.exec(line);
      if (fence) {
        if (f && line.trim().startsWith(fence)) fence = null;
      } else if (f) fence = f[1];
      const heading = !fence && !f ? /^(#{2,3})\s+(.+?)\s*#*\s*$/.exec(line) : null;
      if (heading) parts.push({ depth: heading[1].length, text: heading[2], lines: [] });
      else parts[parts.length - 1].lines.push(line);
    }
    const headingParts = parts.slice(1);
    if (headingParts.length !== anchors.length || headingParts.some((part, i) => part.depth !== anchors[i].depth)) {
      throw new Error(`${rel}：標題數量或層級和 mdsvex 算出來的不一致（${headingParts.length} vs ${anchors.length}）`);
    }
    headingParts.forEach((part, i) => Object.assign(part, { anchor: anchors[i].id, title: anchors[i].text }));

    // 導言：description 放第一句。
    const intro = toPlain(parts[0].lines.join('\n'));
    const description = String(meta.description ?? '');
    chunks.push({
      id: `${section}/${slug}`,
      url: base,
      page,
      section: titles.get(section),
      headings: [],
      text: [description, intro].filter(Boolean).join('\n'),
      keywords,
    });

    // 每個 ## 連同底下的 ###。
    for (let i = 0; i < headingParts.length; i += 1) {
      const h2 = headingParts[i];
      if (h2.depth !== 2) continue; // 頁面開頭直接寫 ### 的情況目前沒有；有的話併進導言也不合適，下面一併處理。
      const subs = [];
      let j = i + 1;
      while (j < headingParts.length && headingParts[j].depth === 3) subs.push(headingParts[j++]);
      const own = toPlain(h2.lines.join('\n'));
      const merged = [own, ...subs.map((sub) => `${sub.title}\n${toPlain(sub.lines.join('\n'))}`)].filter(Boolean).join('\n');
      if (subs.length > 0 && merged.length > SPLIT_CHARS) {
        chunks.push({ id: `${section}/${slug}#${h2.anchor}`, url: `${base}#${h2.anchor}`, page, section: titles.get(section), headings: [h2.title], text: own, keywords });
        for (const sub of subs) {
          chunks.push({
            id: `${section}/${slug}#${sub.anchor}`,
            url: `${base}#${sub.anchor}`,
            page,
            section: titles.get(section),
            headings: [h2.title, sub.title],
            text: toPlain(sub.lines.join('\n')),
            keywords,
          });
        }
      } else {
        /** @type {Chunk} */
        const chunk = { id: `${section}/${slug}#${h2.anchor}`, url: `${base}#${h2.anchor}`, page, section: titles.get(section), headings: [h2.title], text: '', keywords };
        // ### 併在這塊裡：記下每個 ### 在 text 裡從哪裡開始，前端顯示摘要時可以把標題路徑與錨點對到 ###。
        const pieces = own ? [own] : [];
        const subsAt = [];
        for (const sub of subs) {
          const at = pieces.length ? pieces.join('\n').length + 1 : 0;
          subsAt.push({ heading: sub.title, anchor: sub.anchor, at });
          pieces.push([sub.title, toPlain(sub.lines.join('\n'))].filter(Boolean).join('\n'));
        }
        chunk.text = pieces.join('\n');
        if (subsAt.length) chunk.subs = subsAt;
        chunks.push(chunk);
      }
    }
    if (headingParts.length > 0 && headingParts[0].depth === 3) {
      throw new Error(`${rel}：第一個標題是 ###，沒有所屬的 ##`);
    }
  }

  // 內容資料夾以外的頁面（src/routes 底下直接寫的頁），每頁一塊。
  for (const extra of EXTRA_PAGES) chunks.push({ ...extra, headings: [] });

  const seen = new Set();
  for (const chunk of chunks) {
    if (seen.has(chunk.id)) throw new Error(`重複的塊 id：${chunk.id}`);
    seen.add(chunk.id);
  }
  return chunks;
}

/** 送進模型的文字：「頁名 › 標題路徑\n關鍵字\n內文」，太長就截斷。 */
export function embeddingInput(chunk) {
  const head = [chunk.page, ...chunk.headings].join(' › ');
  const text = `${head}\n${chunk.keywords.join('、')}\n${chunk.text}`;
  return text.length > MAX_INPUT_CHARS ? text.slice(0, MAX_INPUT_CHARS) : text;
}

// ---------------------------------------------------------------------------
// embedding（cf ai run）與快取

const sha = (text) => createHash('sha256').update(text).digest('hex');
const cacheFile = (model) => join(CACHE_DIR, `${model.replace(/[^a-z0-9.-]+/gi, '_')}.json`);

async function loadCache(model) {
  try {
    return JSON.parse(await readFile(cacheFile(model), 'utf8'));
  } catch {
    return {};
  }
}

async function saveCache(model, cache) {
  await mkdir(CACHE_DIR, { recursive: true });
  const tmp = `${cacheFile(model)}.tmp`;
  await writeFile(tmp, JSON.stringify(cache));
  await rename(tmp, cacheFile(model));
}

const encode = (vector) => Buffer.from(new Float32Array(vector).buffer).toString('base64');
const decode = (text) => {
  const buf = Buffer.from(text, 'base64');
  return Array.from(new Float32Array(buf.buffer, buf.byteOffset, buf.byteLength / 4));
};

function runCf(args) {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    // Windows 的 cf 是 .cmd，要經過 shell；參數都是自己組的（模型 id、暫存檔路徑），逐一加引號。
    const windows = process.platform === 'win32';
    const command = windows ? `cf.cmd ${args.map((arg) => `"${arg}"`).join(' ')}` : 'cf';
    execFile(
      command,
      windows ? [] : args,
      { shell: windows, maxBuffer: 256 * 1024 * 1024, windowsHide: true },
      (error, stdout, stderr) => {
        const ms = performance.now() - start;
        if (error) return reject(new Error(`${error.message}\n${String(stderr).slice(0, 800)}`));
        const at = stdout.indexOf('{');
        try {
          resolve({ json: JSON.parse(stdout.slice(at)), ms });
        } catch {
          reject(new Error(`無法解析 cf 的輸出：${stdout.slice(0, 300)}\n${String(stderr).slice(0, 300)}`));
        }
      },
    );
  });
}

/** 實際呼叫一批；會重試。回傳向量與耗時。 */
export async function callModel(model, texts, kind, attempts = 4) {
  const spec = MODELS[model];
  if (!spec) throw new Error(`不認得的模型 ${model}`);
  const tmpDir = join(ROOT, '.cache', 'tmp');
  await mkdir(tmpDir, { recursive: true });
  const bodyPath = join(tmpDir, `body-${process.pid}-${Math.random().toString(36).slice(2)}.json`);
  await writeFile(bodyPath, JSON.stringify(spec.body(texts, kind)));
  let lastError;
  try {
    for (let attempt = 1; attempt <= attempts; attempt += 1) {
      try {
        const { json, ms } = await runCf(['ai', 'run', model, '--body', `@${bodyPath}`, '-q']);
        const data = json.data ?? json.result?.data;
        if (!Array.isArray(data) || data.length !== texts.length) {
          throw new Error(`回傳的向量數 ${Array.isArray(data) ? data.length : '?'} ≠ 輸入 ${texts.length}：${JSON.stringify(json).slice(0, 300)}`);
        }
        return { vectors: data, ms };
      } catch (error) {
        lastError = error;
        if (attempt < attempts) {
          const wait = 1000 * 2 ** (attempt - 1);
          console.warn(`[index] ${model} 第 ${attempt} 次失敗，${wait / 1000} 秒後重試：${String(error.message).split('\n')[0]}`);
          await new Promise((r) => setTimeout(r, wait));
        }
      }
    }
  } finally {
    await rm(bodyPath, { force: true });
  }
  throw new Error(`${model} 重試 ${attempts} 次仍失敗：${lastError?.message}`);
}

/**
 * 取一組文字的向量（有快取就用快取）。texts 是「還沒加模型前綴」的文字；前綴在這裡依模型加上。
 * @returns {Promise<number[][]>}
 */
export async function embed(model, texts, kind, { log = true } = {}) {
  const spec = MODELS[model];
  if (!spec) throw new Error(`不認得的模型 ${model}（可用：${Object.keys(MODELS).join('、')}）`);
  const inputs = texts.map((text) => (spec.prepare ? spec.prepare(text, kind) : text));
  const extra = model.includes('qwen') && kind === 'query' ? `\n${QWEN_INSTRUCTION}` : '';
  const keys = inputs.map((input) => sha(`${model}\n${kind}\n${input}${extra}`));
  const cache = await loadCache(model);
  const missing = [];
  keys.forEach((key, i) => {
    if (!cache[key] && !missing.includes(i)) missing.push(i);
  });
  if (missing.length && log) console.log(`[index] ${model}：${missing.length}／${texts.length} 筆要重算`);
  for (let at = 0; at < missing.length; at += spec.batch) {
    const batch = missing.slice(at, at + spec.batch);
    const { vectors } = await callModel(model, batch.map((i) => inputs[i]), kind);
    batch.forEach((i, n) => (cache[keys[i]] = encode(vectors[n])));
    await saveCache(model, cache);
  }
  return keys.map((key) => decode(cache[key]));
}

// ---------------------------------------------------------------------------
// 量化與輸出

export function normalizeVector(vector) {
  let sum = 0;
  for (const v of vector) sum += v * v;
  const norm = Math.sqrt(sum) || 1;
  return vector.map((v) => v / norm);
}

/** int8 量化：每個向量各自一個 scale（最大絕對值 / 127）。 */
export function quantize(vector) {
  const unit = normalizeVector(vector);
  let max = 0;
  for (const v of unit) max = Math.max(max, Math.abs(v));
  const scale = max / 127 || 1;
  return { scale, q: Int8Array.from(unit, (v) => Math.max(-127, Math.min(127, Math.round(v / scale)))) };
}

export function encodeVectors(model, version, ids, vectors) {
  const dim = vectors[0]?.length ?? 0;
  if (ids.length !== vectors.length) throw new Error('id 數和向量數不一致');
  const modelBytes = Buffer.from(model, 'utf8');
  const versionBytes = Buffer.from(version, 'utf8');
  const idBytes = Buffer.from(ids.join('\n'), 'utf8');
  let headerLength = 16 + 2 + modelBytes.length + 2 + versionBytes.length + 4 + idBytes.length;
  headerLength = Math.ceil(headerLength / 4) * 4;
  const total = headerLength + vectors.length * 4 + vectors.length * dim;
  const buf = Buffer.alloc(total);
  buf.write('MMVS', 0, 'ascii');
  buf.writeUInt16LE(1, 4);
  buf.writeUInt16LE(1, 6);
  buf.writeUInt32LE(dim, 8);
  buf.writeUInt32LE(vectors.length, 12);
  let at = 16;
  buf.writeUInt16LE(modelBytes.length, at);
  modelBytes.copy(buf, at + 2);
  at += 2 + modelBytes.length;
  buf.writeUInt16LE(versionBytes.length, at);
  versionBytes.copy(buf, at + 2);
  at += 2 + versionBytes.length;
  buf.writeUInt32LE(idBytes.length, at);
  idBytes.copy(buf, at + 4);
  at = headerLength;
  const quantized = vectors.map((vector) => {
    if (vector.length !== dim) throw new Error('向量維度不一致');
    return quantize(vector);
  });
  quantized.forEach(({ scale }, i) => buf.writeFloatLE(scale, at + i * 4));
  at += vectors.length * 4;
  quantized.forEach(({ q }, i) => Buffer.from(q.buffer).copy(buf, at + i * dim));
  return buf;
}

async function writeAtomic(path, data) {
  const tmp = `${path}.tmp`;
  await writeFile(tmp, data);
  await rename(tmp, path);
}

// ---------------------------------------------------------------------------

async function main() {
  const arg = process.argv.indexOf('--model');
  const model = arg > 0 ? process.argv[arg + 1] : EMBED_MODEL;
  const chunks = await collectChunks();
  const version = sha(`${model}\n${JSON.stringify(chunks)}`).slice(0, 16);
  console.log(`[index] ${chunks.length} 塊，模型 ${model}，version ${version}`);
  if (process.argv.includes('--dry')) {
    // 只看切塊結果，不呼叫模型、不寫檔。
    const lengths = chunks.map((chunk) => embeddingInput(chunk).length).sort((a, b) => a - b);
    console.log(`[index] 輸入長度：最短 ${lengths[0]}、中位 ${lengths[lengths.length >> 1]}、最長 ${lengths[lengths.length - 1]}`);
    for (const chunk of chunks) console.log(`  ${chunk.id}  (${chunk.text.length})`);
    return;
  }

  // 先把所有向量拿齊，才開始寫檔：中途失敗時 static/search 保持原樣。
  const vectors = await embed(model, chunks.map(embeddingInput), 'document');

  await mkdir(OUT_DIR, { recursive: true });
  const json = JSON.stringify({ version, model, dim: vectors[0].length, chunks });
  const bin = encodeVectors(model, version, chunks.map((chunk) => chunk.id), vectors);
  await writeAtomic(join(OUT_DIR, 'chunks.json'), json);
  await writeAtomic(join(OUT_DIR, 'vectors.bin'), bin);
  console.log(
    `[index] 寫出 static/search/chunks.json（${(Buffer.byteLength(json) / 1024).toFixed(1)} kB）、vectors.bin（${(bin.length / 1024).toFixed(1)} kB，${vectors[0].length} 維 int8）`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(`[index] 失敗，沒有寫出索引：${error.message}`);
    process.exit(1);
  });
}
