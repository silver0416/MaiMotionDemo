// 「基礎操作演示」的實際畫面：用 CDP 驅動已發布的桌面 App（與文件站自己的頁面），逐步操作、截圖，並量出每一步要點的位置。
//
// 重跑（PowerShell，在 site/ 底下；App 必須沒有在執行）：
//
//   node scripts/capture-quick.mjs --exe <MaiMotionDemo-v…-windows-x64.exe 的路徑> --profile <隔離設定檔資料夾> [--flow annotate --flow export …]
//
// 例：node scripts/capture-quick.mjs --exe ..\.local\MaiMotionDemo-v1.2.5-windows-x64.exe --profile $env:TEMP\mm-quick-profile --flow annotate
// 只重拍下載頁（發新版後要做）：node scripts/capture-quick.mjs --flow download（不用 App，--exe／--profile 可省略）
//
// 流程（依這個順序跑，同一次啟動 App 接續拍；標 fresh 的流程前面若已經跑過別的流程，會關掉 App、用新的空設定檔再啟動一次）：
//   download  文件站的 /download（headless Edge）。版本號來自本機建置的快照：發新版後這一頁要重拍。
//   open      App 剛開啟的空白主畫面（畫面重用 search-home，只量三欄的位置）
//   search    搜尋並載入示範的歌（SONGS.main）的 BASIC
//   annotate  接續 search：切到「標註」分頁，一顆一顆標左右手（照面板上的模型建議按 A／D，不會出現「≠模型」）
//   export    fresh（不接續 annotate）：先載入 SONGS.second、再載入 SONGS.main（清單新的在上，main 排第一筆），
//             在 main 那一筆「⋯」→「選取」、再點 second，停在「匯出標記檔」之前（不開系統存檔視窗、不寫任何檔案）
// --flow 沒給就全部重拍。只拍某幾個時，它們需要的前置流程（needs）會「只操作、不存檔」地先跑一次，
// 不會改到前置流程已經提交的圖與 manifest（例如 --flow annotate 會先空跑 search）。
//
// 產出：
// - static/shots/quick/<flow>-<畫面>.webp：1280×800 的畫面、deviceScaleFactor 2（實際 2560×1600 像素），CDP 直接存 webp，不需要轉檔套件。
// - src/lib/quick/shots/<flow>.json：畫面清單與每一步的座標（1280×800 的 CSS 像素，全部由 getBoundingClientRect 量出，不手打）。
//   旁白文字不在這裡，在 src/lib/quick/flows.ts（改字不用重拍）。
//
// 安全規則（使用者真正的譜面紀錄在 WebView2 的預設設定檔，絕對不能開到、不能拍到）：
// - 一律用 WEBVIEW2_USER_DATA_FOLDER 指到 --profile 這個隔離的空白資料夾（每次啟動 App 用底下一個新的 launch-<n>）；每次執行前整個刪掉重建。
//   資料夾裡會放一個記號檔（.maimotion-capture-profile）：已經存在、但沒有記號檔的資料夾一律拒絕刪除與使用，避免刪錯地方。
//   拍文件站用的 headless Edge 另用系統暫存資料夾裡一個新的設定檔（mm-capture-edge-<時間>），結束時刪掉。
// - 啟動後譜面紀錄數必須是 0，否則立刻停止（代表沒有隔離成功）。
// - App 已經在執行（tasklist 看得到 mai-motion-demo 或 MaiMotionDemo-v…）就不啟動，請先關掉它；這支腳本不會去關別人開的 App。
// - 除錯埠用 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>；跑完（成功或失敗）都會結束這次啟動的程式（連同子程序）。
// - App 用預設狀態（深色、精簡模式），不改任何設定。譜面網站的索引快取由 App 存在自己的資料夾（不在 WebView2 設定檔裡），
//   所以搜尋視窗可能直接顯示「快取」的索引；這支腳本不按「重新整理索引」，不動那份快取。
// - 不按任何會開系統視窗或寫檔的按鈕：下載頁的下載鈕、App 的「匯出標記檔」都只量位置、不按。
// - 文件站：--site 有回應就直接用，沒有就自己在背景開 `npm run dev`（http://localhost:4330），結束時關掉。
//
// 新增一頁：在 FLOWS 加一個流程（用 ctx.frame 拍畫面、ctx.step 記一步），加進 ORDER，再到 src/lib/quick/flows.ts 寫旁白。

import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const SITE = fileURLToPath(new URL('../', import.meta.url));
const SHOTS_DIR = join(SITE, 'static', 'shots', 'quick');
const DATA_DIR = join(SITE, 'src', 'lib', 'quick', 'shots');
const MARKER = '.maimotion-capture-profile';
const VIEWPORT = { width: 1280, height: 800, scale: 2 };
const QUALITY = 78;
const EDGE_PATHS = [
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];

const { values: args } = parseArgs({
  options: {
    exe: { type: 'string' },
    profile: { type: 'string' },
    flow: { type: 'string', multiple: true },
    port: { type: 'string', default: '9351' },
    site: { type: 'string', default: 'http://localhost:4330' },
    edge: { type: 'string' },
  },
});

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function fail(message) {
  console.error(`capture-quick：${message}`);
  process.exit(1);
}

// ---------------------------------------------------------------------------
// 流程（每一頁一個）

/**
 * @typedef {[number, number, number, number]} Rect  x, y, w, h（1280×800 的 CSS 像素）
 * @typedef {{ id: string, frame: string, after?: string, target?: Rect, focus?: Rect, highlight?: Rect, key?: string }} Step
 * @typedef {{
 *   dry: boolean,                     只操作、不存檔（前置流程）
 *   frame: (id: string, opts?: { reuse?: string }) => Promise<void>,   reuse：沿用另一張已提交的圖（例如 'search-home'），不另存
 *   step: (s: Step) => void,
 *   rect: (expr: string) => Promise<Rect>,   expr 可以是一個元素或元素陣列（取聯集）
 *   evalJs: (expr: string) => Promise<any>,
 *   waitFor: (expr: string, ms?: number, every?: number) => Promise<any>,
 *   click: (expr: string) => Promise<void>,
 *   type: (selector: string, text: string) => Promise<void>,
 *   key: (key: string) => Promise<void>,     真的按鍵（CDP Input），交給頁面自己的鍵盤處理
 *   hover: (expr: string) => Promise<void>,  滑鼠移到元素中央（只移動，不按）
 *   move: (x: number, y: number) => Promise<void>,
 *   goto: (path: string) => Promise<void>,   只有 web 流程能用
 *   meta: Record<string, unknown>,  寫進 manifest（不要放本機路徑、時間戳記）
 *   log: Record<string, unknown>,   只印在終端機
 * }} Ctx
 */

/**
 * 示範用的歌：歌名只寫在這裡（旁白與 .md 不寫死歌名）。
 * - main：〈搜尋第一張譜面〉輸入與載入、〈開始標記〉標的那一筆。query 是搜尋框實際輸入的字（畫面上看得到）。
 * - second：〈多選匯出〉清單裡的第二筆（準備步驟，不拍搜尋過程）。
 * title 以索引裡的寫法為準；比對時用 normTitle 忽略全形／半形、空白與符號（☆、～、！）。
 * 搜尋結果同一首歌有 Standard 與 DX 兩筆時挑 PREFERRED_TYPE；找不到、或像是同一首歌因寫法不同分成好幾筆時，停下來列出搜尋結果。
 * 難度一律挑最低的 BASIC。
 */
const SONGS = {
  main: { query: '四月の雨', title: '四月の雨', difficulty: 'BASIC' },
  second: { query: 'かくしん的', title: 'かくしん的☆めたまるふぉ～ぜっ！', difficulty: 'BASIC' },
};
const PREFERRED_TYPE = 'Standard';

/** 某個難度那一列的「載入」鈕的運算式（歌已經點開時）。 */
const loadButton = (difficulty) =>
  `(() => { const row = [...document.querySelectorAll('#search-panel-wiki table.difficulties tbody tr')].find((r) => txt(r.querySelector('th')).toUpperCase().startsWith(${JSON.stringify(difficulty)})); return row && [...row.querySelectorAll('td.action button')].find((b) => txt(b) === '載入'); })()`;
const SEARCH_BUTTON = `btnText('.records-head button', '搜尋譜面')`;
const SONG_ITEMS = `[...document.querySelectorAll('#search-panel-wiki button.item.song')]`;
/** 搜尋結果每一筆：歌名（不含種類徽章）、種類、來源。 */
const SONG_LIST = `${SONG_ITEMS}.map((b) => { const t = b.querySelector('.title'); return { title: [...t.childNodes].filter((n) => n.nodeType === 3).map((n) => n.textContent).join('').trim(), type: txt(t.querySelector('.type-badge')), sources: [...b.querySelectorAll('.source-badge')].map((s) => txt(s)) }; })`;
const songButton = (index) => `${SONG_ITEMS}[${index}]`;
const normTitle = (s) => s.normalize('NFKC').toLowerCase().replace(/[^\p{L}\p{N}]/gu, '');

/** 準備步驟（不拍）：打開「搜尋譜面」、搜尋 song、點那首歌、載入它的難度，等到清單有 count 筆。 */
async function loadSong(c, song, count, log) {
  await c.click(SEARCH_BUTTON);
  await sleep(600);
  await c.waitFor(INDEX_READY, 120000, 500);
  await c.type('#wiki-query', song.query);
  await c.click(`document.querySelector('#search-panel-wiki form.search-bar button[type=submit]')`);
  await c.waitFor(`${SONG_ITEMS}.length > 0 || txt(document.querySelector('#search-panel-wiki')).includes('找不到')`, 30000);
  await sleep(500);
  const before = log.length;
  await c.click(songButton(pickSong(await c.evalJs(SONG_LIST), song, log)));
  log[before].difficulty = song.difficulty;
  await c.waitFor(`!!${loadButton(song.difficulty)}`, 10000);
  await c.click(loadButton(song.difficulty));
  await c.waitFor(LOADED(count), 180000, 500);
}

/** 在搜尋結果裡找出 song 那一筆的位置；不確定時丟錯（訊息附上實際的搜尋結果）。 */
function pickSong(list, song, log) {
  const want = normTitle(song.title);
  const shown = list.map((h) => `${h.title}［${h.type}・${h.sources.join('＋')}］`);
  const exact = list.map((h, i) => ({ ...h, i })).filter((h) => normTitle(h.title) === want);
  const near = list.filter((h) => normTitle(h.title) !== want && (normTitle(h.title).includes(want) || want.includes(normTitle(h.title))));
  if (!exact.length) throw new Error(`搜尋「${song.query}」找不到「${song.title}」。搜尋結果：${shown.join('、') || '（沒有）'}`);
  if (near.length) throw new Error(`「${song.title}」可能因寫法不同分成好幾筆，請人工確認。搜尋結果：${shown.join('、')}`);
  const types = new Set(exact.map((h) => h.type));
  if (types.size !== exact.length) throw new Error(`「${song.title}」同一個種類有好幾筆（可能是兩個來源寫法不同），請人工確認。搜尋結果：${shown.join('、')}`);
  const chosen = exact.find((h) => h.type === PREFERRED_TYPE) ?? exact[0];
  log.push({ query: song.query, results: shown, chosen: `${chosen.title}［${chosen.type}］`, why: exact.length > 1 ? `有 ${[...types].join('／')}，挑 ${chosen.type}` : '只有一筆' });
  return chosen.i;
}
const INDEX_READY = `(() => { const t = txt(document.querySelector('#search-panel-wiki .index-bar')); return !t.includes('載入中') && (t.match(/首/g) || []).length >= 2; })()`;
/** 載入完成：視窗關掉、開啟中的紀錄分析完、可以播放。 */
const LOADED = (count) =>
  `(() => { const a = document.querySelector('.records-body li.item.is-active'); return document.querySelectorAll('.records-body li.item').length === ${count} && a && !document.querySelector('dialog[open]') && !txt(a).includes('分析中') && !document.body.innerText.includes('沒有可播放的方案'); })()`;

/**
 * fresh：要從乾淨的 App 開始（前面跑過別的流程就重新啟動一次，新的空設定檔）。
 * @type {Record<string, { target: 'app' | 'web', needs?: string[], fresh?: boolean, run: (ctx: Ctx) => Promise<void> }>}
 */
const FLOWS = {
  // 第 1 頁〈下載〉：文件站的下載頁，鏡頭拉近到最上面的下載鈕。不按（不真的下載）。
  download: {
    target: 'web',
    async run(c) {
      await c.goto('/download');
      await c.waitFor(`!!document.querySelector('.recommend .btn--primary')`, 30000);
      await sleep(800);
      c.meta.button = await c.evalJs(`txt(document.querySelector('.recommend .btn--primary'))`);
      c.meta.version = await c.evalJs(`txt(document.querySelector('.version-number'))`);
      await c.frame('page');
      c.step({
        id: 'button',
        frame: 'page',
        target: await c.rect(`document.querySelector('.recommend .btn--primary')`),
        focus: await c.rect(`[document.querySelector('.download .head h1'), document.querySelector('.recommend')]`),
      });
    },
  },

  // 第 2 頁〈開啟 App〉：剛開啟的空白主畫面，依序框出三欄。畫面和 search-home 是同一個狀態，直接沿用那張圖。
  // 「連按兩下下載的檔案」那一格是示意畫面，不是截圖，寫在 src/lib/quick/flows.ts（lead）。
  open: {
    target: 'app',
    async run(c) {
      await c.frame('home', { reuse: 'search-home' });
      c.step({ id: 'records', frame: 'home', highlight: await c.rect(`document.querySelector('.records-pane')`) });
      c.step({ id: 'disc', frame: 'home', highlight: await c.rect(`document.querySelector('.stage-column')`) });
      c.step({ id: 'tabs', frame: 'home', highlight: await c.rect(`document.querySelector('.side-column')`) });
    },
  },

  // 第 3 頁〈搜尋第一張譜面〉：搜尋譜面 → 輸入歌名 → 搜尋 → 點歌 → 在 BASIC 旁按「載入」→ 回到主畫面。
  search: {
    target: 'app',
    async run(c) {
      const song = SONGS.main;
      c.log.songs ??= [];

      await c.frame('home');
      const open = { target: await c.rect(SEARCH_BUTTON), focus: await c.rect(`document.querySelector('.records-head')`) };
      await c.click(SEARCH_BUTTON);
      await sleep(600);
      // 第一次開啟會載入兩個網站的索引；等兩邊都好（有快取時立刻就好）。
      // 只拍不存的前置流程（--flow annotate）也會跑到這裡，歌一樣是 SONGS.main。
      c.log.indexAtOpen = await c.evalJs(`txt(document.querySelector('#search-panel-wiki .index-bar'))`);
      await c.waitFor(INDEX_READY, 120000, 500);
      await sleep(300);
      await c.frame('dialog');
      c.step({ id: 'open', frame: 'home', after: 'dialog', ...open });

      const input = `document.querySelector('#wiki-query')`;
      const form = `document.querySelector('#search-panel-wiki form.search-bar')`;
      const typeStep = { target: await c.rect(input), focus: await c.rect(form) };
      await c.type('#wiki-query', song.query);
      await sleep(300);
      c.log.typed = await c.evalJs(`document.querySelector('#wiki-query').value`);
      await c.frame('typed');
      c.step({ id: 'type', frame: 'dialog', after: 'typed', ...typeStep });

      const submit = `document.querySelector('#search-panel-wiki form.search-bar button[type=submit]')`;
      const searchStep = { target: await c.rect(submit), focus: await c.rect(form) };
      await c.click(submit);
      await c.waitFor(`${SONG_ITEMS}.length > 0 || txt(document.querySelector('#search-panel-wiki')).includes('找不到')`, 30000);
      await sleep(500);
      const songIndex = pickSong(await c.evalJs(SONG_LIST), song, c.log.songs);
      const songBtn = songButton(songIndex);
      c.meta.song = { title: c.log.songs.at(-1).chosen, query: song.query, difficulty: song.difficulty };
      await c.frame('results');
      c.step({ id: 'search', frame: 'typed', after: 'results', ...searchStep });

      // 拉近時把點的那一筆整筆放進來（不是整個結果清單），日文歌名才不會被裁掉。
      const songStep = { target: await c.rect(songBtn), focus: await c.rect(songBtn) };
      await c.click(songBtn);
      const load = loadButton(song.difficulty);
      await c.waitFor(`!!${load}`, 10000);
      // 搜尋視窗裡有沒有封面圖（目前的 App 只下載譜面文字，應該是 0）。
      c.meta.coverImages = await c.evalJs(`document.querySelectorAll('dialog[open] img').length`);
      await sleep(500);
      await c.frame('preview');
      c.step({ id: 'song', frame: 'results', after: 'preview', ...songStep });

      const loadStep = { target: await c.rect(load), focus: await c.rect(`document.querySelector('#search-panel-wiki table.difficulties')`) };
      await c.click(load);
      // 下載、分析完成後視窗自己關掉，左邊多一筆紀錄，盤面可以播放。
      await c.waitFor(LOADED(1), 180000, 500);
      await sleep(1200);
      c.log.record = await c.evalJs(`txt(document.querySelector('.records-body li.item.is-active'))`);
      await c.frame('loaded');
      c.step({ id: 'load', frame: 'preview', after: 'loaded', ...loadStep });

      c.step({
        id: 'done',
        frame: 'loaded',
        highlight: await c.rect(`document.querySelector('.records-body li.item.is-active')`),
      });
      const play = await c.rect(`document.querySelector('button.play-btn')`);
      c.step({ id: 'play', frame: 'loaded', highlight: play, focus: play });
    },
  },

  // 第 4 頁〈開始標記〉：接續 search。右側「標註」→ 點第一顆 → 照模型建議按 A／D（標完自動跳下一步，左右手都要出現）→ 看進度數字。
  annotate: {
    target: 'app',
    needs: ['search'],
    async run(c) {
      // 等「分析完成」之類的通知收掉，畫面乾淨一點（等不到也照拍）。
      await c.waitFor(`!document.querySelector('.toast')`, 15000).catch(() => {});
      await c.move(640, 790);
      await sleep(300);
      await c.frame('start');
      const tab = `[...document.querySelectorAll('.tabbar button')].find((b) => txt(b) === '標註')`;
      const tabStep = { target: await c.rect(tab), focus: await c.rect(`document.querySelector('.tabbar')`) };
      await c.click(tab);
      // 清單的第 i 列（從 0 起算；一列是一步，Slide 的起點與滑行是兩列）。
      // 只算音符那幾列（data-step）；篩選選單的「全部」也是 role=option。清單是虛擬捲動，依畫面上的位置排序。
      const OPTIONS = `[...document.querySelectorAll('.lower [role=option][data-step]')].sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top)`;
      const rowAt = (i) => `${OPTIONS}[${i}]`;
      const selectedIndex = `${OPTIONS}.findIndex((o) => o.getAttribute('aria-selected') === 'true')`;
      await c.waitFor(`!!${rowAt(0)}`, 20000);
      await sleep(700);
      c.log.view = await c.evalJs(`txt(document.querySelector('.views [aria-pressed=true]'))`);
      await c.frame('panel');
      c.step({ id: 'tab', frame: 'start', after: 'panel', ...tabStep });

      // 編輯區加上清單前幾列：拉近時看得到「左 L／右 R」與清單上的標記。
      const area = `[document.querySelector('section.editor'), ${rowAt(0)}, ${rowAt(4)}]`;
      const firstStep = { target: await c.rect(rowAt(0)), focus: await c.rect(area) };
      await c.click(rowAt(0));
      await c.evalJs(`(() => { document.activeElement && document.activeElement.blur && document.activeElement.blur(); return true; })()`);
      await sleep(500);
      await c.frame('selected');
      c.step({ id: 'first', frame: 'panel', after: 'selected', ...firstStep });

      // 標的手照面板上的模型建議（編輯區「模型：L／R」那一行，從 DOM 讀）：示範自然的打法，清單也不會出現「≠模型」。
      // 至少標 MIN_KEYS 步，而且左右手都要出現過；最多 MAX_KEYS 步。一步是清單的一列：Slide 的起點與滑行各是一列，
      // 模型建議寫成「L 滑 R」時，起點那一列取前面的手、滑行那一列取「滑」後面的手。建議讀不出單一的手（例如滑行中換手 L→R）就停。
      // 停下時條件還沒滿足就丟錯（不改規則、不換歌；要換難度由人決定）。
      // 實際按的鍵寫進 manifest（step.key），旁白由 flows.ts 依 key 產生。
      const MIN_KEYS = 3;
      const MAX_KEYS = 8;
      const HAND_KEY = { L: 'A', R: 'D' };
      const modelHint = `txt([...document.querySelectorAll('section.editor .muted')].find((s) => txt(s).startsWith('模型：'))).replace('模型：', '').trim()`;
      const pressed = [];
      const hints = [];
      let prev = 'selected';
      while (pressed.length < MAX_KEYS) {
        const hands = new Set(pressed.map((p) => p.hand));
        // 不停在 Slide 中間：起點標了就連滑行一起標完（上面的進度數字算的是整顆音符）。
        const midSlide = !!pressed.at(-1)?.row.includes('起點');
        if (pressed.length >= MIN_KEYS && hands.size === 2 && !midSlide) break;
        const at = await c.evalJs(selectedIndex);
        const hint = await c.evalJs(modelHint);
        const rowText = await c.evalJs(`txt(${rowAt(at)})`);
        const parts = /^([LR])(?: 滑 ([LR]))?$/.exec(hint);
        const hand = parts ? (parts[2] && rowText.includes('滑行') ? parts[2] : parts[1]) : null;
        hints.push(`${rowText} → 模型「${hint}」${hand ? `→ ${hand}` : ''}`);
        if (!hand) {
          c.log.stoppedAt = `第 ${pressed.length + 1} 步（${hints.at(-1)}）的模型建議讀不出單一的手`;
          break;
        }
        const key = HAND_KEY[hand];
        const n = pressed.length + 1;
        await c.key(key.toLowerCase());
        await sleep(600);
        const labeled = await c.evalJs(`txt(${rowAt(at)})`);
        if (labeled.includes('≠模型')) throw new Error(`第 ${n} 顆標完出現「≠模型」：${labeled}`);
        pressed.push({ hand, key, row: labeled, at });
        await c.frame(`key${n}`);
        prev = `key${n}`;
      }
      c.log.hints = hints;
      if (pressed.length < MIN_KEYS || new Set(pressed.map((p) => p.hand)).size < 2) {
        throw new Error(`標了 ${pressed.length} 顆就停了，沒有達到「至少 ${MIN_KEYS} 顆、左右手都出現」：${hints.join('；')}`);
      }
      c.log.keys = pressed.map((p) => `${p.key}（模型 ${p.hand}）：${p.row}`);
      // 按鍵的步驟拉近清單前幾列（看得到目前這一列與標上的 L／R）。座標在按鍵前後不變（清單沒有捲動），標完再量一次。
      const keyFocus = await c.rect(`[${rowAt(0)}, ${rowAt(pressed.at(-1).at + 1)}]`);
      pressed.forEach((p, i) => {
        c.step({ id: `key${i + 1}`, frame: i === 0 ? 'selected' : `key${i}`, after: `key${i + 1}`, key: p.key, focus: keyFocus });
      });
      const counter = `document.querySelector('.side-column [title="已確認／全部音符"]')`;
      c.log.progress = await c.evalJs(`txt(${counter})`);
      c.step({ id: 'progress', frame: prev, highlight: await c.rect(counter), focus: await c.rect(`${counter}.closest('header')`) });
    },
  },

  // 第 5 頁〈多選匯出〉：自己一次乾淨的啟動（fresh）。先載入 SONGS.second、再載入 SONGS.main（不拍），
  // 清單預設新的在上面，所以 main 排第一筆（不動排序選項）。
  // 在 main 那一筆按「⋯」→「選取」→ 再點 second 那一筆 → 框起選取工具列的「匯出標記檔」。不按匯出（會開系統存檔視窗、寫檔）。
  // 點哪一筆用歌名比對決定，不寫死位置；清單順序、選單開在哪一筆、兩筆都選到，任何一項不對就失敗。
  export: {
    target: 'app',
    fresh: true,
    async run(c) {
      const songs = [];
      await loadSong(c, SONGS.second, 1, songs);
      await loadSong(c, SONGS.main, 2, songs);
      c.log.songs = songs;
      c.meta.records = songs.map((s) => ({ title: s.chosen, query: s.query, difficulty: s.difficulty }));
      await c.waitFor(`!document.querySelector('.toast')`, 15000).catch(() => {});
      await sleep(800);

      const ITEMS = `[...document.querySelectorAll('.records-body li.item')]`;
      const item = (i) => `${ITEMS}[${i}]`;
      const texts = await c.evalJs(`${ITEMS}.map((li) => txt(li))`);
      c.log.records = texts;
      const indexOf = (song) => {
        const want = normTitle(song.title);
        const found = texts.map((t, i) => [normTitle(t), i]).filter(([t]) => t.startsWith(want));
        if (found.length !== 1) throw new Error(`紀錄清單裡「${song.title}」有 ${found.length} 筆：${texts.join('／')}`);
        return found[0][1];
      };
      const first = indexOf(SONGS.main);
      const second = indexOf(SONGS.second);
      if (first !== 0) throw new Error(`紀錄清單第一筆不是「${SONGS.main.title}」：${texts.join('／')}`);
      const picked = (i) => `txt(${item(i)}).includes('（已選取）')`;

      // 滑鼠停在 main 那一筆上，「⋯」才看得到（平常隱藏）。
      await c.hover(item(first));
      await sleep(400);
      const more = `${item(first)}.querySelector('button.more')`;
      const list = `[${item(first)}, ${item(second)}]`;
      await c.frame('list');
      const moreStep = { target: await c.rect(more), focus: await c.rect(list) };
      await c.click(more);
      await c.move(130, 720);
      const menuItem = `[...document.querySelectorAll('[role=menu] [role=menuitem], [role=menu] button')].find((b) => txt(b) === '選取')`;
      await c.waitFor(`!!${menuItem}`, 5000);
      if (!(await c.evalJs(`${more}.getAttribute('aria-expanded') === 'true'`))) throw new Error(`選單不是開在「${SONGS.main.title}」那一筆`);
      await sleep(400);
      await c.frame('menu');
      c.step({ id: 'more', frame: 'list', after: 'menu', ...moreStep });

      const selectStep = { target: await c.rect(menuItem), focus: await c.rect(`[document.querySelector('[role=menu]'), ${item(first)}]`) };
      await c.click(menuItem);
      await c.waitFor(`txt(document.querySelector('.selection')).includes('已選 1 筆')`, 5000);
      if (!(await c.evalJs(picked(first)))) throw new Error(`選到的不是「${SONGS.main.title}」`);
      await sleep(600);
      await c.frame('picked1');
      c.step({ id: 'select', frame: 'menu', after: 'picked1', ...selectStep });

      const open2 = `${item(second)}.querySelector('button.open')`;
      const pickStep = { target: await c.rect(open2), focus: await c.rect(`[document.querySelector('.selection'), ${item(second)}]`) };
      await c.click(open2);
      await c.waitFor(`txt(document.querySelector('.selection')).includes('已選 2 筆')`, 5000);
      if (!(await c.evalJs(`${picked(first)} && ${picked(second)}`))) throw new Error('已選 2 筆，但不是這兩首都選到');
      await sleep(500);
      await c.frame('picked2');
      c.step({ id: 'pick', frame: 'picked1', after: 'picked2', ...pickStep });
      c.log.clicked = { menu: texts[first], pick: texts[second] };

      const exportButton = `document.querySelector('.selection button[aria-label="匯出標記檔"]')`;
      c.step({
        id: 'export',
        frame: 'picked2',
        highlight: await c.rect(exportButton),
        focus: await c.rect(`[document.querySelector('.selection'), ${item(first)}, ${item(second)}]`),
      });
    },
  },
};

const ORDER = ['download', 'open', 'search', 'annotate', 'export'];

// ---------------------------------------------------------------------------
// 檢查與準備

const port = Number(args.port);
const wanted = new Set(args.flow?.length ? args.flow : ORDER);
for (const name of wanted) if (!FLOWS[name]) fail(`沒有這個流程：${name}（可用：${ORDER.join('、')}）`);

/** 要跑的流程（含前置），依 ORDER 排；不在 wanted 裡的只空跑。 */
const plan = new Set();
const addWithNeeds = (name) => {
  for (const need of FLOWS[name].needs ?? []) addWithNeeds(need);
  plan.add(name);
};
for (const name of wanted) addWithNeeds(name);
const runList = ORDER.filter((name) => plan.has(name));
const needApp = runList.some((name) => FLOWS[name].target === 'app');
const needWeb = runList.some((name) => FLOWS[name].target === 'web');

// 只拍網頁（--flow download）時不用 App，也不需要 --exe／--profile。
if (needApp && (!args.exe || !args.profile)) {
  fail('用法：node scripts/capture-quick.mjs --exe <App 執行檔> --profile <隔離設定檔資料夾> [--flow search]（只拍下載頁：--flow download）');
}
const exe = needApp ? resolve(args.exe) : null;
const profile = needApp ? resolve(args.profile) : null;
if (exe && !existsSync(exe)) fail(`找不到執行檔：${exe}`);

if (needApp && process.platform === 'win32') {
  const list = execFileSync('tasklist', ['/FO', 'CSV', '/NH'], { encoding: 'utf8' });
  const running = list
    .split(/\r?\n/)
    .map((line) => line.split('","')[0].replace(/^"/, ''))
    .filter((name) => /^mai-motion-demo|^MaiMotionDemo/i.test(name));
  if (running.length) fail(`App 已經在執行（${[...new Set(running)].join('、')}），請先關掉再拍；這支腳本不會關掉它。`);
}

if (profile) {
  if (existsSync(profile)) {
    const entries = readdirSync(profile);
    if (entries.length > 0 && !entries.includes(MARKER)) {
      fail(`${profile} 已經存在而且不是這支腳本建立的（沒有 ${MARKER}），不刪除也不使用。請換一個新的資料夾。`);
    }
    rmSync(profile, { recursive: true, force: true });
  }
  mkdirSync(profile, { recursive: true });
  writeFileSync(join(profile, MARKER), 'capture-quick.mjs 的隔離 WebView2 設定檔，可以整個刪掉。\n');
}
mkdirSync(SHOTS_DIR, { recursive: true });
mkdirSync(DATA_DIR, { recursive: true });

/** App 的版本（從執行檔檔名）；只拍下載頁時用下載頁上的版本號（meta.version）。 */
const appVersion = exe ? (/v(\d+\.\d+\.\d+)/.exec(basename(exe))?.[1] ?? null) : null;

// ---------------------------------------------------------------------------
// 程序與 CDP

/** @type {{ proc: import('node:child_process').ChildProcess, name: string }[]} */
const started = [];

function stopAll() {
  for (const { proc } of started.reverse()) {
    if (proc.exitCode !== null || !proc.pid) continue;
    try {
      if (process.platform === 'win32') execFileSync('taskkill', ['/PID', String(proc.pid), '/T', '/F'], { stdio: 'ignore' });
      else proc.kill();
    } catch {
      proc.kill();
    }
  }
}

const HELPERS = `
  var txt = (el) => ((el && el.textContent) || '').replace(/\\s+/g, ' ').trim();
  var vis = (el) => { const b = el.getBoundingClientRect(); const s = getComputedStyle(el); return b.width > 0 && b.height > 0 && s.visibility !== 'hidden' && s.display !== 'none'; };
  var btnText = (sel, text) => [...document.querySelectorAll(sel)].filter(vis).find((b) => txt(b) === text);
`;

async function connect(cdpPort) {
  let target;
  for (let i = 0; i < 160 && !target; i++) {
    await sleep(250);
    try {
      const list = await (await fetch(`http://127.0.0.1:${cdpPort}/json`)).json();
      target = list.find((t) => t.type === 'page' && !t.url.startsWith('devtools'));
    } catch {
      // 還沒起來
    }
  }
  if (!target) throw new Error(`連不上除錯埠 ${cdpPort}`);
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((ok, no) => {
    ws.addEventListener('open', ok);
    ws.addEventListener('error', no);
  });
  let nextId = 0;
  const pending = new Map();
  const errors = [];
  ws.addEventListener('message', (event) => {
    const m = JSON.parse(event.data);
    if (m.id && pending.has(m.id)) {
      pending.get(m.id)(m);
      pending.delete(m.id);
    }
    if (m.method === 'Runtime.exceptionThrown') errors.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text);
  });
  const send = (method, params = {}) =>
    new Promise((ok) => {
      const n = ++nextId;
      pending.set(n, ok);
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  const evalJs = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: `${HELPERS}\n(${expr})`, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(`頁面內執行失敗：${r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text}`);
    return r.result?.result?.value;
  };
  const waitFor = async (expr, ms = 20000, every = 300) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      const v = await evalJs(expr).catch(() => null);
      if (v) return v;
      await sleep(every);
    }
    throw new Error(`等不到：${expr.slice(0, 120)}`);
  };
  await send('Runtime.enable');
  await send('Page.enable');
  const metrics = await send('Emulation.setDeviceMetricsOverride', {
    width: VIEWPORT.width,
    height: VIEWPORT.height,
    deviceScaleFactor: VIEWPORT.scale,
    mobile: false,
  });
  if (metrics.error) throw new Error(`無法固定畫面尺寸：${metrics.error.message}`);
  return { ws, send, evalJs, waitFor, errors };
}

async function checkViewport(cdp) {
  const actual = await cdp.evalJs(`[innerWidth, innerHeight, devicePixelRatio]`);
  if (actual[0] !== VIEWPORT.width || actual[1] !== VIEWPORT.height || Math.round(actual[2]) !== VIEWPORT.scale) {
    throw new Error(`畫面尺寸不是 ${VIEWPORT.width}×${VIEWPORT.height}@${VIEWPORT.scale}：${actual.join('×')}`);
  }
}

async function siteUp(url) {
  try {
    const r = await fetch(url, { signal: AbortSignal.timeout(5000) });
    return r.ok;
  } catch {
    return false;
  }
}

/** 這次 headless Edge 的設定檔（結束時試著刪掉）。 */
let edgeProfile = '';

async function startWeb() {
  const base = args.site.replace(/\/+$/, '');
  if (!(await siteUp(`${base}/download`))) {
    // 直接用 node 跑 vite（和 `npm run dev` 同一個指令），不經過 npm／cmd：結束時才關得乾淨。
    const vite = join(SITE, 'node_modules', 'vite', 'bin', 'vite.js');
    const dev = spawn(process.execPath, [vite, 'dev', '--port', String(new URL(args.site).port || 4330), '--strictPort'], { cwd: SITE, stdio: 'ignore' });
    started.push({ proc: dev, name: 'dev server' });
    let ok = false;
    for (let i = 0; i < 120 && !ok; i++) {
      await sleep(500);
      ok = await siteUp(`${base}/download`);
    }
    if (!ok) throw new Error(`文件站開不起來：${base}`);
  }
  const edge = args.edge ?? EDGE_PATHS.find((p) => existsSync(p));
  if (!edge) throw new Error('找不到 msedge.exe，請用 --edge 指定');
  const edgePort = port + 1;
  // Edge 的設定檔放在系統暫存資料夾、每次一個新的（Edge 留下的部分檔案有時刪不掉，不能放在 --profile 裡擋住下一次執行）。
  edgeProfile = join(tmpdir(), `mm-capture-edge-${Date.now()}`);
  const browser = spawn(
    edge,
    ['--headless=new', `--remote-debugging-port=${edgePort}`, `--user-data-dir=${edgeProfile}`, '--no-first-run', '--hide-scrollbars', 'about:blank'],
    { stdio: 'ignore' },
  );
  started.push({ proc: browser, name: 'headless Edge' });
  const cdp = await connect(edgePort);
  await cdp.send('Emulation.setEmulatedMedia', {
    features: [
      { name: 'prefers-color-scheme', value: 'dark' },
      { name: 'prefers-reduced-motion', value: 'no-preference' },
    ],
  });
  return { cdp, base };
}

/** 第幾次啟動 App：每次用 --profile 底下一個新的空資料夾（launch-1、launch-2…），不用刪舊的就是全新的設定檔。 */
let launches = 0;

async function startApp() {
  launches += 1;
  const userData = join(profile, `launch-${launches}`);
  mkdirSync(userData, { recursive: true });
  const proc = spawn(exe, [], {
    env: {
      ...process.env,
      WEBVIEW2_USER_DATA_FOLDER: userData,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  started.push({ proc, name: 'App' });
  const cdp = await connect(port);
  await cdp.waitFor(`!!document.querySelector('.records-body')`, 30000);
  const records = await cdp.evalJs(`document.querySelectorAll('.records-body li.item').length`);
  if (records !== 0) throw new Error(`啟動時譜面紀錄數是 ${records}，不是 0：設定檔可能沒有隔離成功，停止。`);
  await sleep(2500);
  await checkViewport(cdp);
  return cdp;
}

/** 關掉這支腳本啟動的 App（連同子程序），給 fresh 流程重新啟動用。 */
async function stopApp(cdp) {
  try {
    cdp.ws.close();
  } catch {
    // 已經關了
  }
  for (const entry of started.filter((s) => s.name === 'App')) {
    if (entry.proc.exitCode === null && entry.proc.pid) {
      try {
        execFileSync('taskkill', ['/PID', String(entry.proc.pid), '/T', '/F'], { stdio: 'ignore' });
      } catch {
        entry.proc.kill();
      }
    }
    started.splice(started.indexOf(entry), 1);
  }
  await sleep(2000);
}

// ---------------------------------------------------------------------------
// 執行

const report = { app: appVersion, ran: runList.map((name) => (wanted.has(name) ? name : `${name}（空跑）`)), flows: {} };
let exitCode = 0;
let web;
let app;

try {
  if (needWeb) web = await startWeb();
  if (needApp) {
    app = await startApp();
    report.dialogsAtStart = await app.evalJs(`[...document.querySelectorAll('dialog[open]')].map((d) => txt(d).slice(0, 200))`);
    report.toastsAtStart = await app.evalJs(`[...document.querySelectorAll('.toast')].map((t) => txt(t).slice(0, 200))`);
    report.palette = await app.evalJs(`[document.documentElement.dataset.palette ?? null, document.documentElement.dataset.help ?? null]`);
  }

  /** 這次啟動的 App 已經跑過流程（fresh 流程要重新啟動一個乾淨的）。 */
  let appUsed = false;
  for (const name of runList) {
    const flow = FLOWS[name];
    const dry = !wanted.has(name);
    if (flow.target === 'app') {
      if (flow.fresh && appUsed) {
        await stopApp(app);
        app = await startApp();
        appUsed = false;
      }
      appUsed = true;
    }
    const cdp = flow.target === 'web' ? web.cdp : app;
    const frames = [];
    /** 整個流程成功才寫檔，失敗時不會留下和 manifest 對不上的圖。 */
    const buffers = new Map();
    const steps = [];
    const meta = {};
    const log = {};
    /** @type {Ctx} */
    const ctx = {
      dry,
      meta,
      log,
      evalJs: cdp.evalJs,
      waitFor: cdp.waitFor,
      async frame(id, opts = {}) {
        if (dry) return;
        if (opts.reuse) {
          const file = `${opts.reuse}.webp`;
          const path = join(SHOTS_DIR, file);
          if (!existsSync(path)) throw new Error(`要沿用的圖不存在：${file}`);
          frames.push({ id, src: `/shots/quick/${file}`, bytes: statSync(path).size, reused: true });
          return;
        }
        const r = await cdp.send('Page.captureScreenshot', { format: 'webp', quality: QUALITY });
        if (!r.result?.data) throw new Error(`截圖失敗：${JSON.stringify(r.error)}`);
        const buffer = Buffer.from(r.result.data, 'base64');
        const file = `${name}-${id}.webp`;
        buffers.set(file, buffer);
        frames.push({ id, src: `/shots/quick/${file}`, bytes: buffer.length });
      },
      step(s) {
        if (!dry) steps.push(s);
      },
      async rect(expr) {
        const r = await cdp.evalJs(`(() => {
          const found = ${expr};
          const list = (Array.isArray(found) ? found : [found]).filter(Boolean);
          if (list.length === 0) return null;
          list[0].scrollIntoView({ block: 'nearest' });
          const boxes = list.map((el) => el.getBoundingClientRect());
          const x = Math.min(...boxes.map((b) => b.left));
          const y = Math.min(...boxes.map((b) => b.top));
          return [x, y, Math.max(...boxes.map((b) => b.right)) - x, Math.max(...boxes.map((b) => b.bottom)) - y];
        })()`);
        if (!r) throw new Error(`找不到元素：${expr.slice(0, 120)}`);
        const [x, y, w, h] = r.map((v) => Math.round(v * 10) / 10);
        if (x < -0.5 || y < -0.5 || x + w > VIEWPORT.width + 0.5 || y + h > VIEWPORT.height + 0.5) {
          throw new Error(`元素超出畫面：${expr.slice(0, 120)} → ${[x, y, w, h].join(', ')}`);
        }
        return [Math.max(0, x), Math.max(0, y), w, h];
      },
      async click(expr) {
        const ok = await cdp.evalJs(`(() => { const el = ${expr}; if (!el) return false; el.click(); return true; })()`);
        if (!ok) throw new Error(`找不到要按的元素：${expr.slice(0, 120)}`);
      },
      async type(selector, text) {
        await cdp.evalJs(`(() => {
          const t = document.querySelector(${JSON.stringify(selector)});
          t.focus();
          const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
          set.call(t, ${JSON.stringify(text)});
          t.dispatchEvent(new Event('input', { bubbles: true }));
          return true;
        })()`);
      },
      async key(key) {
        const code = `Key${key.toUpperCase()}`;
        const keyCode = key.toUpperCase().charCodeAt(0);
        await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, text: key, unmodifiedText: key, windowsVirtualKeyCode: keyCode });
        await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode: keyCode });
      },
      async hover(expr) {
        const [x, y, w, h] = await ctx.rect(expr);
        await ctx.move(x + w / 2, y + h / 2);
      },
      async move(x, y) {
        await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      },
      async goto(path) {
        if (flow.target !== 'web') throw new Error('goto 只有 web 流程能用');
        await cdp.send('Page.navigate', { url: `${web.base}${path}` });
        await sleep(1500);
        await cdp.waitFor(`document.readyState === 'complete'`, 30000);
        await checkViewport(cdp);
      },
    };
    await flow.run(ctx);
    if (dry) {
      report.flows[name] = { dry: true, log };
      continue;
    }
    const frameIds = new Set(frames.map((f) => f.id));
    for (const s of steps) {
      for (const key of ['frame', 'after']) {
        if (s[key] && !frameIds.has(s[key])) throw new Error(`步驟 ${s.id} 的 ${key} 指到不存在的畫面 ${s[key]}`);
      }
    }
    for (const [file, buffer] of buffers) writeFileSync(join(SHOTS_DIR, file), buffer);
    const manifest = {
      flow: name,
      app: appVersion ?? (typeof meta.version === 'string' ? meta.version.replace(/^v/, '') : null),
      viewport: VIEWPORT,
      frames,
      steps,
      meta,
    };
    writeFileSync(join(DATA_DIR, `${name}.json`), `${JSON.stringify(manifest, null, 2)}\n`);
    report.flows[name] = { frames: frames.map((f) => `${f.src} ${(f.bytes / 1024).toFixed(1)} KB${f.reused ? '（沿用）' : ''}`), meta, log };
  }
  report.pageErrors = [...(app?.errors ?? []), ...(web?.cdp.errors ?? [])];
} catch (error) {
  report.error = String(error instanceof Error ? error.message : error);
  exitCode = 1;
} finally {
  for (const cdp of [app, web?.cdp]) {
    try {
      cdp?.ws.close();
    } catch {
      // 已經關了
    }
  }
  stopAll();
  if (edgeProfile) {
    await sleep(1500);
    try {
      rmSync(edgeProfile, { recursive: true, force: true });
    } catch {
      report.leftover = `headless Edge 的暫存設定檔沒刪乾淨，可以之後手動刪：${edgeProfile}`;
    }
  }
}

console.log(JSON.stringify(report, null, 2));
process.exit(exitCode);
