# MaiMotionDemo 文件站（maimotiondemo.bapu.dev）

新手教學與各功能詳解。靜態頁（SvelteKit＋mdsvex，全部預先算繪）放在 Cloudflare Worker 的 assets，Worker 本身只接語意搜尋 API。

這份是文件站的開發約定，所有改這個資料夾的人（或 agent）先讀完。

## 目錄

```
site/
  package.json            獨立的套件（不動 repo 根目錄的 package.json）
  vite.config.ts          SvelteKit 3 的設定都在這裡（沒有 svelte.config.js，放了會報錯）；adapter-static
                          別名：$lib → src/lib；$app/(components|lib|state|styles|video)/… → ../frontend/src
                          （直接沿用 App 的 tokens、盤面與工具函式；$app/navigation 等仍是 SvelteKit 自己的）
  markdown.js             Markdown 管線：自動注入 Demo／Callout 的 import、mdsvex、標題錨點、表格 data-label、複製鈕
  src/
    app.html              主題在第一次繪製前套用（不閃色）
    styles/               docs.css：只放文件站自己的版面；tokens 與原語一律來自 App 的 app.css
    routes/               +layout（外框）、首頁、[section]/[slug]（內容頁）、404
    content/<section>/<slug>.md   內容（見「內容格式」）
    lib/
      nav.ts              章節順序與名稱
      shell/              外框元件：側欄、底部分頁列、頁內目錄、主題切換
      search/             搜尋對話框（UI）與 engine.ts（字面比對＋語意結果合併）
      demos/              動畫演示（見「演示」）
  scripts/
    gen-demos.mjs         呼叫 Rust 核心產生盤面演示資料
    build-index.mjs       把內容切塊、算 embedding，產生搜尋索引
  static/                 favicon、robots.txt、search/（索引，建置產物）
  worker/                 cf 專案：cloudflare.config.ts、src/index.ts（/api/search）
```

## 絕對不要做的事

- 不改 `frontend/`、`src/`、`src-tauri/`、`docs/`、repo 根目錄的設定檔。文件站只「讀」它們。
- 不 commit、不 push、不部署（`cf deploy`）。部署由主控在驗收後執行。
- 內容來源只有：`USER_GUIDE.md`、`README.md`、App 實際的畫面文字（`frontend/src`）。`docs/` 底下是只留本機的工作文件，**不可把它們的文字搬上網站**。
- 不寫模型的訓練來源、不寫模型變體的內部 id（yamabuki、entan、konruri…）；畫面上的名稱只有「V4.1」「TemotoAI-2」「TemotoAI-1」與顏色名。
- 不把模型的參數或結果說成官方判定、人類真實機率或唯一正確打法。
- 不用 emoji、漸層、半透明面板、巢狀卡片、左側強調邊框；不引入 UI 套件、CSS framework、動畫庫、字型 CDN。

## 設計

先讀 `.claude/skills/maimotion-ui/SKILL.md` 與它的 `references/`（tokens、components、disc-stage、motion、copy-and-states、checklist）。文件站是 App 的延伸，不是新風格。

- `src/styles/docs.css` 以 `@import` 引入 `frontend/src/styles/app.css`，按鈕、分段、輸入、徽章、提示、浮出選單全部用 App 的 class（`.btn`、`.seg`、`.input`、`.badge`、`.alert`、`.pop`…）。
- 主題：預設跟隨系統，深色＝`classic`（不設 `data-palette`）、淺色＝`studio`。可手動切換，存 localStorage `maimotion.docs.theme`。
- 盤面永遠黑底；左手粉、右手藍、換手琥珀。
- 文件是拿來讀的，內文用閱讀字級：`--fs-prose`（16px）、行高 1.75、欄寬約 46 個中文字（`max-width: 46rem`）。這是文件站唯一新增的字級 token，其他一律用 App 的 token。
- App 的「精簡模式／`.help`」不適用於文件站（文件本身就是說明）。
- 頂列：在頁面頂端時和背景同色、沒有分隔線；往下捲之後變成極輕微的毛玻璃（背景 94% 不透明＋`blur(6px)`）加一點陰影。這是使用者 2026-11-15 指定的例外（仿 react.dev），全站只有這一處用半透明與 `backdrop-filter`，不要搬到別的元件。
- 站名只寫英文「MaiMotionDemo」（圓體 M PLUS Rounded 1c ExtraBold，`.wordmark`），後面不接中文；標誌是不帶底色的向量圖（`Logo.svelte`），環跟著文字顏色。

### 版面與手機版

斷點與做法照 BOTPu admin：

| 寬度 | 版面 |
| --- | --- |
| `< 768px` | 手機：底部分頁列（首頁／目錄／搜尋／更多），高 56px，加 `env(safe-area-inset-bottom)`；「目錄」「更多」「搜尋」開成底部抽屜（`<dialog>`，從下滑入，上緣圓角 `--radius-dialog`；搜尋是全螢幕） |
| `≥ 768px` | 左側欄（章節目錄，sticky，100dvh）＋內容 |
| `≥ 1280px` | 再加右側「本頁內容」目錄（sticky）；較窄時改成內容上方可收合的一塊 |

- `viewport-fit=cover`；固定在底部的東西都加 safe-area。
- `@media (pointer: coarse)`：控制項高度 44px；輸入框字級 `max(16px, …)`（避免 iOS 放大）。
- hover 樣式只寫在 `@media (hover: hover)` 裡。
- 表格外包 `.table-wrap`（container query）：容器寬 `< 560px` 時每列變成一張直排清單，`td` 以 `data-label` 顯示欄名。整頁永遠不能水平捲動。
- 對話框沒有半透明遮罩（`::backdrop { background: none }`），靠邊框與陰影分層。

## 內容格式

檔案：`src/content/<section>/<slug>.md` → 網址 `/<section>/<slug>`。

```md
---
title: 第一次播放
description: 貼上一段譜面，看兩隻手怎麼動。（一句話，會用在搜尋結果與 meta）
order: 2
keywords: [播放, 範例, 新增]   # 選填：使用者可能會打、但內文沒出現的說法
---

內文從這裡開始，不要再寫一次 h1（標題由版面畫）。

## 二級標題

### 三級標題
```

- 章節（`section`）與顯示名稱固定在 `src/lib/nav.ts`：`start` 開始使用、`charts` 匯入譜面、`play` 播放與檢視、`model` 模型怎麼分左右手、`annotate` 真人手順標註、`more` 其他。
- 只用 `##` 與 `###`；每個 `##` 是搜尋索引的一塊，所以一塊講一件事，標題用使用者會問的說法。
- 文案：繁體中文（台灣用語）、全形標點、口語、不討好；按鈕與分頁名稱用「」括起來，和 App 畫面上的字完全一致；鍵盤按鍵用 `<kbd>A</kbd>`；simai 範例用 ```` ```simai ```` 程式碼區塊（版面會加上複製鈕）。
- 提示框：`<Callout tone="info|warn" title="選填">內文</Callout>`。一頁最多兩三個。
- 演示：`<Demo id="swap" />`，可加 `caption="…"`。可用的 id 見下表。不需要寫 import（建置時自動注入）。
- 內部連結用絕對路徑：`[影片同步](/annotate/video)`。

## 演示（`src/lib/demos/`）

`Demo.svelte` 依 `registry.ts` 的 id 延遲載入對應元件（元件清單在 `loaders.ts`，由 `Demo.svelte` 以 `import()` 載入，不要改成靜態 import，否則用到 `<Demo>` 的頁面會在 `<head>` 多載所有演示的 CSS）；元件檔不存在時顯示「演示製作中」的佔位框（只在開發時）。

載入前的預留高度登記在 `registry.ts` 的 `reserve`：盤面演示用一條直線（高 = k × 寬 + b）；示意動畫的高度隨寬度是階梯狀，用 `steps` 分段（`Demo.svelte` 轉成 `@container` 規則）。改了演示的版面、旁白或按鈕，要重量並更新數字，否則載入時版面會跳。

兩種演示，畫面上要標清楚：

- **盤面演示**（徽章「核心實際結果」）：資料由 `scripts/gen-demos.mjs` 呼叫 Rust 核心（`target/release/mai-motion-core.exe`，stdin 是 `AnalyzeRequest` JSON）以 App 的預設參數算出，存成 `src/lib/demos/data/<id>.json`（已精簡、進版控）。前端只播放，不重算左右手。
- **示意動畫**（徽章「示意」）：用來解釋介面操作或概念的手繪動畫，數字是示意用的。

共同規則：進入可視範圍才開始播、離開就暫停；有播放／暫停鈕與重播；`prefers-reduced-motion` 時不自動播，停在最有代表性的一格並保留手動控制；寬度 100%，375px 寬也要能看能按；動畫只用 `transform`、`opacity`、SVG 屬性與既有曲線（`--ease-out`、`--ease-spring`）。

### 盤面演示 id

| id | simai | 用在 |
| --- | --- | --- |
| `tap-alternate` | `(120){4}1,8,2,7,3,6,4,5,E` | 第一次播放 |
| `hold-other-hand` | `(120){4}8h[4:4]/1,2,3,4,E` | Hold 佔住一手 |
| `slide-basic` | `(120){4}1-5[4:3],8,7,6,E` | Slide 基本 |
| `swap` | `(120){4}1-5[4:1]/5-1[4:1],{16},,,1,E` | 兩手互換（Slide 結束前還有一顆 1 要接） |
| `cross` | `(120){4}1-5[4:1]/5-1[4:1],E` | 兩條對穿、後面沒音符：各畫各的、交叉穿過 |
| `slide-long` | `(120){4}8>4[1##3],,,,,,,8,E` | 長 Slide 途中放開、之後由另一手接回 |
| `jack` | `(240){24}1,1,1,1,E` | 同點連打 |
| `abab` | `(240){24}1,2,1,2,1,2,E` | 相鄰兩鍵來回：一隻手滑移 |
| `abab-far` | `(240){24}1,8,1,8,1,8,E` | 跨中線的兩鍵來回：兩手各守一點 |
| `stair` | `(200){16}6,5,4,3,2,1,8,7,E` | 快速階梯 |
| `palm` | `(120){4}Ch[4:2]/B1/E1/7,E` | 手掌覆蓋（C 按住 1 秒，覆蓋看得清楚） |
| `palm-hold` | `(190){4}1h[4:15],,,Ch[1:3],,,,,B1,B8,B2,B7,B3,B6,B4,B5,E` | 按住 C 再蓋整圈 B |
| `wifi` | `(120){4}1w5[4:2],E` | WiFi 雙手分工（L+R滑） |
| `wifi-one-hand` | `(120){4}1w5[4:2],{8},C,E` | 另一手要接 C：一隻手覆蓋三條 |
| `slide-palm` | `(120){4}1-5[4:1]/8h[4:3],{16},B2,E` | 滑行途中順帶 Touch |
| `touch-kinds` | `(120){4}A1f,B3,Cf,D5f,E7,Chf[4:2],E` | Touch 外形與煙火 |
| `shapes` | `(120){2}1-5[4:1],3^6[4:1],5v2[4:1],7q3[4:1],1s5[4:1],8w4[4:1],2V84[4:1],E` | Slide 形狀總覽 |

### 示意動畫 id

| id | 內容 |
| --- | --- |
| `layout-tour` | App 三欄版面的地圖，點各區看說明 |
| `disc-legend` | 盤面上每種圖形代表什麼（音符外形、手、軌跡、標籤、換手菱形） |
| `simai-timing` | `(BPM){分割}` 與逗號怎麼換成時間；Slide 起點後等一拍才出發 |
| `cost-groups` | 三群分數怎麼加總、候選方案怎麼排 |
| `palm-radius` | 手掌半徑拉大拉小，蓋得到哪些 Touch |
| `glide-rule` | 距離與間隔怎麼決定「滑移」還是「抬手」 |
| `loop-markers` | 時間軸：循環片段把手與標籤 |
| `annotate-keys` | 逐顆標註：選音符、按 A／D、Slide 分兩步、自動跳下一步 |
| `branch-lanes` | 打法分支：分出、併入、切換路線 |
| `video-align` | 影片同步：把影片停在第一顆、設為第一顆、之後互相跟隨 |

## 搜尋

字面比對（瀏覽器內、立即）＋語意搜尋（Worker 用 embedding 比 cosine），合併成一份結果。

- `scripts/build-index.mjs`（`npm.cmd run index`）：每頁依 `##` 切塊（`##` 前的導言加上頁面 description 自成一塊；`###` 併在所屬的 `##` 裡，`subs` 記下每個 `###` 在內文的位置，摘要落在那裡時結果直接連到 `###` 的錨點；某個 `##` 超過 900 字才依 `###` 再切）。錨點用 `markdown.js` 同一條 mdsvex 管線算，和頁面上的 id 保證一致。輸出：
  - `static/search/chunks.json`：`{ version, model, dim, chunks: [{ id, url, page, section, headings, text, keywords, subs? }] }`。`text` 是純文字：表格轉成「欄名：值」的句子、`<Demo>` 換成 caption、程式碼保留原文。`version` 是模型＋全部內容的雜湊。
  - `static/search/vectors.bin`：檔頭 `MMVS`、維度、數量、model、version、各塊 id，接著每塊一個 float32 scale 與 int8 向量（格式寫在腳本開頭）。int8 比 float16 小一半，評估集上排名完全相同。
  - embedding 的輸入＝「頁名 › 標題
關鍵字
內文」（最多 1500 字）。用 `cf ai run` 呼叫 Workers AI（會用到帳號額度，量很小），結果依「模型＋輸入」雜湊快取在 `site/.cache/embeddings/`，重跑只算改過的塊；任何一批重試後仍失敗就中止，不寫出半套索引。
- 模型：`@cf/baai/bge-m3`（1024 維）。換模型前先跑 `npm.cmd run eval:search`（`scripts/eval-search.mjs`，查詢集在 `scripts/search-queries.json`：近百筆口語、同義、錯字、症狀式查詢，各附應命中的頁面），比較字面／語意／合併的第 1／前 3／前 5 命中率；門檻要跟著改 `worker/src/index.ts` 的 `THRESHOLDS`。
- `worker/src/index.ts`：`GET /api/search?q=&v=`。查詢正規化（NFKC、去頭尾空白、壓縮空白、最多 200 字，空的回 400）→ rate limit（每個 IP 10 秒 20 次，超過回 429）→ `caches.default`（key 含 model 與索引 version）→ Workers AI 算查詢向量 → 和 `/search/vectors.bin`（isolate 內快取，用戶端帶來的 `v` 不同才重抓）比 cosine → `{ ok: true, version, results: [{ id, score }] }`（前 12 筆，cosine ≥ 0.47）。瀏覽器快取 10 分鐘、邊緣 7 天。AI 失敗回 503 `{ ok: false, error: "embedding_unavailable" }`。其餘路徑一律交給 assets（沒對到檔案時回 `404.html`）。
- `src/lib/search/engine.ts`：字面結果立即出（state `literal`）；停止輸入 250 ms 才問 API（輸入法組字中不問、同一查詢記在記憶體、新查詢會取消舊請求）；回來後合併（state `done`）。排序：標題／關鍵字的字面命中 → 語意命中（依分數；兩邊都中標 `both`）→ 只有內文字面命中的。API 失敗、4 秒逾時、離線或打不到（`vite dev` 沒有 Worker）時 state `offline`，只留字面結果，不報錯。
- 安全標頭與快取：靜態檔不經過 Worker，所以放在 assets 的 `_headers`，由 `worker/vite.config.ts` 在建置時產生：CSP（`script-src` 只允許同源＋各頁行內 script 的 sha256，不開 `'unsafe-inline'`；`style-src` 因 Svelte 的 `style=` 屬性保留 `'unsafe-inline'`）、`X-Content-Type-Options`、`Referrer-Policy`；`/_app/immutable/*` 快取一年、`/search/*` 5 分鐘、HTML 用預設的 `max-age=0, must-revalidate`。同一份 CSP 也由 Worker 加在它回的 HTML（404）上。內容改過一定要先 `npm.cmd run build` 再建 Worker，hash 才會對。

## 指令（PowerShell，在 `site/` 底下）

```powershell
npm.cmd install
npm.cmd run dev          # http://localhost:4330（沒有 Worker：搜尋只有字面結果）
npm.cmd run check        # svelte-check，要 0 errors／0 warnings
npm.cmd run build        # 輸出到 site/build
npm.cmd run demos        # 重新產生盤面演示資料（需要先 cargo build --release --bin mai-motion-core）
npm.cmd run index        # 重新產生搜尋索引（呼叫 Workers AI，有快取）
npm.cmd run eval:search  # 搜尋評估：各模型 × 字面／語意／合併的命中率
npm.cmd run worker:dev   # 先 build；在 worker/ 跑 cf dev：http://localhost:4333，靜態頁＋/api/search（AI 走真的 Workers AI）
npm.cmd run worker:build # 在 worker/ 跑 cf build，輸出到 worker/.cloudflare/output
npm.cmd run worker:preview # 先 worker:build；跑建好的 Worker（http://localhost:4333），會套用 _headers（CSP 等）。worker:dev 不套用 _headers，要驗 CSP 用這個
npm.cmd run build:all    # index → build → worker:build（部署前的完整建置）
npm.cmd run deploy:dry   # cf deploy --dry-run：建置並檢查，不上傳
```

`worker/` 用 `cf` 與 `cloudflare.config.ts`，不用 Wrangler、不要建立 wrangler 設定檔。`worker/package.json` 只是讓 `cf` 認得這是 Vite 專案，套件實際裝在 `site/node_modules`。

部署（主控驗收後才做）：

```powershell
npm.cmd run build:all
cd worker
npx.cmd cf deploy
```

部署會在 Cloudflare 建立或更新：Worker `maimotiondemo-docs`（含靜態檔、observability logs、Workers AI 與 rate limit 綁定）、自訂網域 `maimotiondemo.bapu.dev`（bapu.dev 這個 zone 底下會多一筆由 Workers 管理的 DNS 紀錄與憑證）。不開 workers.dev 與預覽網址；不需要任何 secret。
