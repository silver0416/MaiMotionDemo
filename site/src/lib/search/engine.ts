/**
 * 手冊搜尋：字面比對（瀏覽器內，立即）＋語意搜尋（/api/search，Worker 用 embedding 比 cosine）。
 * 索引是 /search/chunks.json（scripts/build-index.mjs 產生，每個 ## 一塊）。SearchDialog 只依賴下面這組介面。
 *
 * 狀態：
 * - idle：沒有查詢字。
 * - literal：字面結果已出，語意結果還在路上。
 * - done：全部結果都到了（字面＋語意合併）。
 * - offline：語意搜尋失敗、逾時或離線，只有字面結果。
 *
 * 合併後的順序：
 * 1. 標題或關鍵字的字面命中（某個詞出現在這塊的標題，或出現在頁名／頁面關鍵字而這塊是該頁的導言）；
 * 2. 語意命中，依分數高到低（字面也中的標成 both）；
 * 3. 只有內文字面命中、語意沒選到的。
 */

export interface SearchHit {
  id: string;
  url: string;
  page: string;
  headings: string[];
  snippet: string;
  /** snippet 裡要標示的關鍵字位置（[起, 迄)，UTF-16 索引）。 */
  ranges: [number, number][];
  source: 'literal' | 'semantic' | 'both';
}

export type SearchState = 'idle' | 'literal' | 'done' | 'offline';

/** 索引狀態：missing 表示 chunks.json 還沒建立（或讀不到）。 */
export type IndexStatus = 'loading' | 'ready' | 'missing';

/** /api/search 回傳的一筆語意結果。 */
export interface SemanticResult {
  id: string;
  score: number;
}

interface Chunk {
  id: string;
  url: string;
  page: string;
  section: string;
  headings: string[];
  text: string;
  keywords: string[];
  /** 併進這塊的 ###：在 text 裡從 at 開始。 */
  subs?: { heading: string; anchor: string; at: number }[];
}

interface Prepared extends Chunk {
  nPage: string;
  nHeadings: string;
  nKeywords: string;
  nText: string;
  /** 頁面導言（沒有標題的那一塊）：頁名與頁面關鍵字的命中算在它身上。 */
  intro: boolean;
}

const INDEX_URL = '/search/chunks.json';
const API_URL = '/api/search';
const MAX_HITS = 20;
const SNIPPET_BEFORE = 28;
const SNIPPET_LENGTH = 110;
/** 停止輸入多久後才問語意搜尋。 */
const DEBOUNCE_MS = 250;
/** 語意搜尋逾時。 */
const TIMEOUT_MS = 4000;
/** API 打不到（例如 vite dev 沒有 Worker）之後，這段時間內不再嘗試。 */
const BACKOFF_MS = 30_000;
/** 查詢字長度上限（和 Worker 一致）。 */
const MAX_QUERY = 200;

let index: Prepared[] | null = null;
let byId = new Map<string, Prepared>();
let version = '';
let loading: Promise<IndexStatus> | null = null;
let status: IndexStatus = 'loading';

/** 語意結果的記憶體快取（key 是正規化後的查詢字）。 */
const semanticCache = new Map<string, SemanticResult[]>();
const SEMANTIC_CACHE_SIZE = 100;
let apiDownUntil = 0;

/**
 * 逐字正規化（全形英數轉半形、轉小寫），長度不變：比對到的位置可以直接對回原文。
 */
function normalize(text: string): string {
  let out = '';
  for (const char of text) {
    const folded = char.normalize('NFKC').toLowerCase();
    out += folded.length === char.length ? folded : char.toLowerCase();
  }
  return out;
}

/** 送給 API 的查詢字（Worker 端做同樣的事；也是語意快取的 key）。 */
function apiQuery(query: string): string {
  return query.normalize('NFKC').trim().replace(/\s+/g, ' ').slice(0, MAX_QUERY);
}

const CJK = /[㐀-鿿豈-﫿]/;

/** 查詢字切成詞：空白與常見標點都當分隔。 */
function terms(query: string): string[] {
  return normalize(query)
    .split(/[\s,，。、；;：:？?！!「」『』（）()]+/u)
    .map((term) => term.trim())
    .filter(Boolean);
}

function bigrams(term: string): string[] {
  const chars = [...term];
  const list: string[] = [];
  for (let i = 0; i < chars.length - 1; i += 1) list.push(chars[i] + chars[i + 1]);
  return list;
}

/** 讀索引（只讀一次）。檔案不存在時回 missing，不丟錯。 */
export function ensureIndex(): Promise<IndexStatus> {
  if (loading) return loading;
  loading = (async () => {
    try {
      const response = await fetch(INDEX_URL);
      if (!response.ok) throw new Error(String(response.status));
      const data = (await response.json()) as { version?: string; chunks?: Chunk[] };
      if (!Array.isArray(data.chunks)) throw new Error('格式不符');
      index = data.chunks.map((chunk) => {
        const headings = Array.isArray(chunk.headings) ? chunk.headings : [];
        return {
          ...chunk,
          headings,
          keywords: Array.isArray(chunk.keywords) ? chunk.keywords : [],
          subs: Array.isArray(chunk.subs) ? chunk.subs : undefined,
          nPage: normalize(chunk.page ?? ''),
          nHeadings: normalize(headings.join(' ')),
          nKeywords: normalize((chunk.keywords ?? []).join(' ')),
          nText: normalize(chunk.text ?? ''),
          intro: headings.length === 0,
        };
      });
      byId = new Map(index.map((chunk) => [chunk.id, chunk]));
      version = typeof data.version === 'string' ? data.version : '';
      status = 'ready';
    } catch {
      index = null;
      status = 'missing';
      // 下次打開對話框再試一次（例如開發時剛跑完 npm run index）。
      loading = null;
    }
    return status;
  })();
  return loading;
}

export function indexStatus(): IndexStatus {
  return status;
}

// ---------------------------------------------------------------------------
// 字面比對

interface TermMatch {
  score: number;
  /** 在 nText 裡要標示的片段（字面詞本身，或部分比對時的兩字詞）。 */
  marks: string[];
  /** 這個詞出現在標題，或（導言塊）出現在頁名／頁面關鍵字。 */
  strong: boolean;
  /** 整個詞都有對到（不是退一步的兩字詞比對）。 */
  exact: boolean;
}

function matchTerm(chunk: Prepared, term: string): TermMatch | null {
  let score = 0;
  let strong = false;
  if (chunk.nHeadings.includes(term)) {
    score += 6;
    strong = true;
  }
  if (chunk.nPage.includes(term)) {
    score += 4;
    if (chunk.intro) strong = true;
  }
  if (chunk.nKeywords.includes(term)) {
    score += 4;
    if (chunk.intro) strong = true;
  }
  if (chunk.nText.includes(term)) {
    let count = 0;
    let at = chunk.nText.indexOf(term);
    while (at >= 0 && count < 3) {
      count += 1;
      at = chunk.nText.indexOf(term, at + term.length);
    }
    score += 1 + count;
  }
  if (score > 0) return { score, marks: [term], strong, exact: true };

  // 較長的中文詞找不到整串時，退一步看兩字詞重疊多少（「兩隻手為什麼會互換」→「兩隻」「互換」…）。
  if ([...term].length >= 3 && CJK.test(term)) {
    const grams = bigrams(term);
    const haystack = `${chunk.nHeadings} ${chunk.nPage} ${chunk.nKeywords} ${chunk.nText}`;
    const hit = grams.filter((gram) => haystack.includes(gram));
    const ratio = hit.length / grams.length;
    if (ratio >= 0.5) return { score: ratio * 3, marks: hit, strong: false, exact: false };
  }
  return null;
}

interface LiteralMatch {
  chunk: Prepared;
  score: number;
  marks: string[];
  strong: boolean;
}

function literalMatches(query: string): LiteralMatch[] {
  if (!index) return [];
  const list = terms(query);
  if (list.length === 0) return [];
  const scored: LiteralMatch[] = [];
  for (const chunk of index) {
    let total = 0;
    const marks: string[] = [];
    let ok = true;
    let exact = true;
    let strong = false;
    for (const term of list) {
      const match = matchTerm(chunk, term);
      if (!match) {
        ok = false;
        break;
      }
      total += match.score;
      marks.push(...match.marks);
      exact &&= match.exact;
      strong ||= match.strong;
    }
    if (ok) scored.push({ chunk, score: total, marks, strong: strong && exact });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, MAX_HITS);
}

// ---------------------------------------------------------------------------
// 摘要

/** 位置 at 落在哪個併進來的 ### 裡：回傳顯示用的標題路徑與網址。 */
function locate(chunk: Prepared, at: number): { headings: string[]; url: string } {
  if (!chunk.subs || at < 0) return { headings: chunk.headings, url: chunk.url };
  let sub: { heading: string; anchor: string; at: number } | undefined;
  for (const item of chunk.subs) if (item.at <= at) sub = item;
  if (!sub) return { headings: chunk.headings, url: chunk.url };
  const base = chunk.url.replace(/#.*$/, '');
  return { headings: [...chunk.headings, sub.heading], url: `${base}#${sub.anchor}` };
}

function snippetWindow(chunk: Prepared, first: number): { start: number; snippet: string; prefix: string } {
  const text = chunk.text ?? '';
  const start = first > SNIPPET_BEFORE ? first - SNIPPET_BEFORE : 0;
  const end = Math.min(text.length, start + SNIPPET_LENGTH);
  const prefix = start > 0 ? '…' : '';
  const suffix = end < text.length ? '…' : '';
  return { start, prefix, snippet: prefix + text.slice(start, end).replace(/\s+/g, ' ') + suffix };
}

function literalHit(match: LiteralMatch, source: SearchHit['source']): SearchHit {
  const { chunk, marks } = match;
  let first = -1;
  for (const mark of marks) {
    const at = chunk.nText.indexOf(mark);
    if (at >= 0 && (first < 0 || at < first)) first = at;
  }
  const { snippet, prefix } = snippetWindow(chunk, first);
  // 換掉連續空白會改變長度：重新在摘要上找位置。
  const nBody = normalize(snippet);
  const raw: [number, number][] = [];
  for (const mark of marks) {
    let at = nBody.indexOf(mark, prefix.length);
    while (at >= 0) {
      raw.push([at, at + mark.length]);
      at = nBody.indexOf(mark, at + mark.length);
    }
  }
  raw.sort((a, b) => a[0] - b[0]);
  const ranges: [number, number][] = [];
  for (const range of raw) {
    const last = ranges[ranges.length - 1];
    if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1]);
    else ranges.push([range[0], range[1]]);
  }
  const { headings, url } = locate(chunk, first);
  return { id: chunk.id, url, page: chunk.page, headings, snippet, ranges, source };
}

/** 語意命中、沒有字面位置：取和查詢最像的一句（兩字詞重疊最多），都不像就取開頭。 */
function semanticHit(chunk: Prepared, query: string): SearchHit {
  const grams = new Set<string>();
  for (const term of terms(query)) {
    if (CJK.test(term)) for (const gram of bigrams(term)) grams.add(gram);
    else if (term.length >= 2) grams.add(term);
  }
  let best = 0;
  let bestScore = 0;
  const pattern = /[^。！？\n]+[。！？]?/g;
  for (const match of chunk.nText.matchAll(pattern)) {
    let score = 0;
    for (const gram of grams) if (match[0].includes(gram)) score += 1;
    if (score > bestScore) {
      bestScore = score;
      best = match.index ?? 0;
    }
  }
  // 從句子開頭起算，不往前多帶字。
  const text = chunk.text ?? '';
  const end = Math.min(text.length, best + SNIPPET_LENGTH);
  const snippet = (best > 0 ? '…' : '') + text.slice(best, end).replace(/\s+/g, ' ') + (end < text.length ? '…' : '');
  const { headings, url } = locate(chunk, bestScore > 0 ? best : -1);
  return { id: chunk.id, url, page: chunk.page, headings, snippet, ranges: [], source: 'semantic' };
}

// ---------------------------------------------------------------------------
// 合併

/**
 * 字面結果與語意結果合併成最後的清單。semantic 為 null 表示沒有語意結果（還沒回來或失敗），只排字面。
 * 匯出給 scripts/eval-search.mjs 評估用；介面上只透過 createSearch 使用。
 */
export function combine(query: string, semantic: SemanticResult[] | null): SearchHit[] {
  const literal = literalMatches(query);
  if (!semantic) return literal.map((match) => literalHit(match, 'literal'));

  const literalById = new Map(literal.map((match) => [match.chunk.id, match]));
  const semanticById = new Map(semantic.map((result) => [result.id, result.score]));
  const out: SearchHit[] = [];
  const used = new Set<string>();

  // 1. 標題／關鍵字的字面命中：依字面分數，同分看語意分數。
  const strong = literal
    .filter((match) => match.strong)
    .sort(
      (a, b) =>
        b.score - a.score || (semanticById.get(b.chunk.id) ?? -1) - (semanticById.get(a.chunk.id) ?? -1),
    );
  for (const match of strong) {
    out.push(literalHit(match, semanticById.has(match.chunk.id) ? 'both' : 'literal'));
    used.add(match.chunk.id);
  }

  // 2. 語意命中（Worker 已依分數排好、去掉低於門檻的）。
  for (const result of semantic) {
    if (used.has(result.id)) continue;
    const chunk = byId.get(result.id);
    if (!chunk) continue; // 索引版本不同時，不認得的塊直接略過。
    const match = literalById.get(result.id);
    out.push(match ? literalHit(match, 'both') : semanticHit(chunk, query));
    used.add(result.id);
  }

  // 3. 只有內文字面命中的。
  for (const match of literal) {
    if (used.has(match.chunk.id)) continue;
    out.push(literalHit(match, 'literal'));
    used.add(match.chunk.id);
  }
  return out.slice(0, MAX_HITS);
}

// ---------------------------------------------------------------------------
// 語意搜尋（/api/search）

function remember(key: string, results: SemanticResult[]) {
  if (semanticCache.size >= SEMANTIC_CACHE_SIZE) {
    const oldest = semanticCache.keys().next().value;
    if (oldest !== undefined) semanticCache.delete(oldest);
  }
  semanticCache.set(key, results);
}

function apiAvailable(): boolean {
  if (typeof navigator !== 'undefined' && navigator.onLine === false) return false;
  return Date.now() >= apiDownUntil;
}

/** 問 Worker；失敗、逾時或格式不符都回 null（不丟錯）。被取消時回 'aborted'。 */
async function fetchSemantic(key: string, signal: AbortSignal): Promise<SemanticResult[] | null | 'aborted'> {
  const timeout = new AbortController();
  const timer = setTimeout(() => timeout.abort(), TIMEOUT_MS);
  const onAbort = () => timeout.abort();
  signal.addEventListener('abort', onAbort);
  try {
    const params = new URLSearchParams({ q: key });
    if (version) params.set('v', version);
    const response = await fetch(`${API_URL}?${params}`, {
      signal: timeout.signal,
      headers: { Accept: 'application/json' },
    });
    const type = response.headers.get('content-type') ?? '';
    if (!type.includes('application/json')) {
      // 拿到的不是 API（例如 vite dev 回 404 頁）：一陣子內不再問。
      apiDownUntil = Date.now() + BACKOFF_MS;
      return null;
    }
    const data = (await response.json()) as { ok?: boolean; results?: SemanticResult[] };
    if (!response.ok || !data.ok || !Array.isArray(data.results)) return null;
    return data.results.filter(
      (item): item is SemanticResult => typeof item?.id === 'string' && typeof item?.score === 'number',
    );
  } catch {
    return signal.aborted ? 'aborted' : null;
  } finally {
    clearTimeout(timer);
    signal.removeEventListener('abort', onAbort);
  }
}

/**
 * 建立一個搜尋工作階段：每次 query() 都會（可能多次）呼叫 onUpdate，
 * 較舊的查詢結果晚到時直接丟掉；cancel() 之後不再回呼。
 * 輸入法組字中不要呼叫 query()（SearchDialog 已處理）。
 */
export function createSearch(onUpdate: (hits: SearchHit[], state: SearchState) => void): {
  query(q: string): void;
  cancel(): void;
} {
  let token = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let controller: AbortController | null = null;

  function stop() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    controller?.abort();
    controller = null;
  }

  return {
    query(q: string) {
      const mine = ++token;
      stop();
      if (!q.trim()) {
        onUpdate([], 'idle');
        return;
      }
      void ensureIndex().then((state) => {
        if (mine !== token) return;
        if (state !== 'ready') {
          onUpdate([], 'offline');
          return;
        }
        const key = apiQuery(q);
        const cached = semanticCache.get(key);
        if (cached) {
          onUpdate(combine(q, cached), 'done');
          return;
        }
        if (!apiAvailable()) {
          onUpdate(combine(q, null), 'offline');
          return;
        }
        onUpdate(combine(q, null), 'literal');
        timer = setTimeout(() => {
          timer = undefined;
          const ctrl = new AbortController();
          controller = ctrl;
          void fetchSemantic(key, ctrl.signal).then((results) => {
            if (controller === ctrl) controller = null;
            if (results === 'aborted' || mine !== token) return;
            if (results === null) {
              onUpdate(combine(q, null), 'offline');
              return;
            }
            remember(key, results);
            onUpdate(combine(q, results), 'done');
          });
        }, DEBOUNCE_MS);
      });
    },
    cancel() {
      token += 1;
      stop();
    },
  };
}
