/**
 * 「更新內容」用的精簡 Markdown：只認 GitHub Release 說明實際用到的語法
 * （標題、清單、段落、粗體、行內程式碼、連結，外加分隔線與程式碼區塊），
 * 轉成資料再由 Svelte 輸出，不經過 {@html}。認不得的語法照原文顯示。
 */

export interface Span {
  text: string;
  strong?: boolean;
  code?: boolean;
  /** 只有 http(s) 連結才有；其他連結只留文字。 */
  href?: string;
}

export type Block =
  | { kind: 'heading'; level: number; spans: Span[] }
  | { kind: 'paragraph'; spans: Span[] }
  | { kind: 'list'; ordered: boolean; items: Span[][] }
  | { kind: 'code'; text: string }
  | { kind: 'rule' };

const HEADING = /^(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/;
const RULE = /^([-*_])(?:\s*\1){2,}$/;
const ITEM = /^([-*+]|\d+[.)])\s+(.*)$/;
const LINK = /^\[([^\]]+)\]\(([^()\s]+)\)/;

export function parseMarkdown(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;

  const flushParagraph = () => {
    if (paragraph.length > 0) blocks.push({ kind: 'paragraph', spans: parseInline(paragraph.join(' ')) });
    paragraph = [];
  };
  const flushList = () => {
    if (list) blocks.push({ kind: 'list', ordered: list.ordered, items: list.items.map((item) => parseInline(item)) });
    list = null;
  };

  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    const trimmed = line.trim();
    const fence = /^(`{3,}|~{3,})/.exec(trimmed)?.[1];
    if (fence) {
      flushParagraph();
      flushList();
      const body: string[] = [];
      for (index++; index < lines.length && !lines[index].trim().startsWith(fence); index++) body.push(lines[index]);
      blocks.push({ kind: 'code', text: body.join('\n') });
      continue;
    }
    if (trimmed === '') {
      flushParagraph();
      flushList();
      continue;
    }
    const heading = HEADING.exec(trimmed);
    if (heading) {
      flushParagraph();
      flushList();
      blocks.push({ kind: 'heading', level: heading[1].length, spans: parseInline(heading[2]) });
      continue;
    }
    if (RULE.test(trimmed)) {
      flushParagraph();
      flushList();
      blocks.push({ kind: 'rule' });
      continue;
    }
    const item = ITEM.exec(trimmed);
    if (item) {
      flushParagraph();
      const ordered = /\d/.test(item[1]);
      if (list && list.ordered !== ordered) flushList();
      list ??= { ordered, items: [] };
      list.items.push(item[2]);
      continue;
    }
    // 清單項目縮排的下一行接在同一項後面。
    if (list && /^\s/.test(line)) {
      list.items[list.items.length - 1] += ` ${trimmed}`;
      continue;
    }
    flushList();
    paragraph.push(trimmed.replace(/^>\s?/, ''));
  }
  flushParagraph();
  flushList();
  return blocks;
}

const DOWNLOAD_HEADING = /^(下載|downloads?)$/i;
/** v0.4 以前的說明沒有「下載」標題，執行檔檔名、校驗碼與程式內更新的說明直接寫成段落。 */
const DOWNLOAD_PARAGRAPH = /^Windows x64 單一執行檔|^EXE SHA256|可以直接在程式內更新/;

const plainText = (spans: Span[]) => spans.map((span) => span.text).join('').trim();

/**
 * 程式內看更新內容時拿掉下載說明：「下載」標題底下整段（到下一個同級或更高的標題為止），
 * 以及舊版說明裡的執行檔檔名、程式內更新方式段落。要下載的人按「在 GitHub 查看」。
 */
export function withoutDownloads(blocks: Block[]): Block[] {
  const kept: Block[] = [];
  let skipping: number | null = null;
  for (const block of blocks) {
    if (block.kind === 'heading') {
      if (skipping !== null && block.level > skipping) continue;
      skipping = DOWNLOAD_HEADING.test(plainText(block.spans)) ? block.level : null;
      if (skipping !== null) continue;
    } else if (skipping !== null) {
      continue;
    }
    if (block.kind === 'paragraph' && DOWNLOAD_PARAGRAPH.test(plainText(block.spans))) continue;
    kept.push(block);
  }
  return kept;
}

/** 行內：`程式碼`、**粗體**、[文字](網址)。 */
export function parseInline(text: string, strongOuter = false): Span[] {
  const spans: Span[] = [];
  let strong = strongOuter;
  let plain = '';

  const push = (span: Span) => {
    if (!span.text) return;
    const last = spans[spans.length - 1];
    const plainSpan = (item: Span) => !item.code && !item.href;
    if (last && plainSpan(last) && plainSpan(span) && !!last.strong === !!span.strong) last.text += span.text;
    else spans.push(span);
  };
  const flushPlain = () => {
    push(strong ? { text: plain, strong } : { text: plain });
    plain = '';
  };

  let index = 0;
  while (index < text.length) {
    const rest = text.slice(index);
    if (rest.startsWith('`')) {
      const end = text.indexOf('`', index + 1);
      if (end > index + 1) {
        flushPlain();
        push({ text: text.slice(index + 1, end), code: true, ...(strong ? { strong } : {}) });
        index = end + 1;
        continue;
      }
    }
    // 粗體要有成對的 ** 才算，落單的照原文。
    if (rest.startsWith('**') && (strong || text.indexOf('**', index + 2) > index + 2)) {
      flushPlain();
      strong = !strong;
      index += 2;
      continue;
    }
    const link = rest.startsWith('[') ? LINK.exec(rest) : null;
    if (link) {
      flushPlain();
      const href = /^https?:\/\//i.test(link[2]) ? link[2] : undefined;
      for (const inner of parseInline(link[1], strong)) push(href ? { ...inner, href } : inner);
      index += link[0].length;
      continue;
    }
    plain += text[index];
    index++;
  }
  flushPlain();
  return spans;
}
