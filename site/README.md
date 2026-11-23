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
      nav.ts              章節順序、名稱、分群（tone）與各章涵蓋的內容資料夾（dirs）
      shell/              外框元件：頂列、側欄、選單抽屜、頁內目錄、主題切換
      search/             搜尋對話框（UI）與 engine.ts（字面比對＋語意結果合併）
      demos/              動畫演示（見「演示」）
  scripts/
    gen-demos.mjs         呼叫 Rust 核心產生盤面演示資料
    capture-quick.mjs     驅動桌面 App 拍「基礎操作演示」的實際畫面與座標
    build-index.mjs       把內容切塊、算 embedding，產生搜尋索引
  static/                 favicon、robots.txt、search/（索引，建置產物）
  worker/                 cf 專案：cloudflare.config.ts、src/index.ts（/api/search）
```

## 絕對不要做的事

- 不改 `frontend/`、`src/`、`src-tauri/`、`docs/`、repo 根目錄的設定檔。文件站只「讀」它們。
- 不 commit、不 push、不部署（`cf deploy`）。部署由主控在驗收後執行。
- 內容來源只有：`USER_GUIDE.md`、`README.md`、App 實際的畫面文字（`frontend/src`）。`docs/` 底下是只留本機的工作文件，**不可把它們的文字搬上網站**。
- 不寫模型的訓練來源、不寫模型變體的內部 id（yamabuki、entan、konruri…）；畫面上的名稱只有「V4.3」等基座版本、「TemotoAI-2」「TemotoAI-1」與顏色名。
- 不把模型的參數或結果說成官方判定、人類真實機率或唯一正確打法。
- 不用 emoji、漸層、半透明面板、巢狀卡片、左側強調邊框；不引入 UI 套件、CSS framework、動畫庫、字型 CDN。

## 設計

先讀 `.claude/skills/maimotion-ui/SKILL.md` 與它的 `references/`（tokens、components、disc-stage、motion、copy-and-states、checklist）。文件站是 App 的延伸，不是新風格。

- `src/styles/docs.css` 以 `@import` 引入 `frontend/src/styles/app.css`，按鈕、分段、輸入、徽章、提示、浮出選單全部用 App 的 class（`.btn`、`.seg`、`.input`、`.badge`、`.alert`、`.pop`…）。
- 主題：預設跟隨系統，深色＝`classic`（不設 `data-palette`）、淺色＝`studio`。可手動切換，存 localStorage `maimotion.docs.theme`。
- 盤面永遠黑底；左手粉、右手藍、換手琥珀。
- 文件是拿來讀的，內文用閱讀字級：`--fs-prose`（16px）、行高 1.75、欄寬約 46 個中文字（`max-width: 46rem`）。這是文件站唯一新增的字級 token，其他一律用 App 的 token。
- App 的「精簡模式／`.help`」不適用於文件站（文件本身就是說明）。
- 頂列：在頁面頂端時和背景同色、沒有分隔線；往下捲之後變成極輕微的毛玻璃（背景 94% 不透明＋`blur(6px)`）加一點陰影。這是使用者 2026-11-15 指定的例外（仿 react.dev），全站只有這一處用半透明與 `backdrop-filter`，不要搬到別的元件。App 外框 iOS 版的頂列（`AppTopbar.svelte`）沿用同一個例外，不算新的；Android 版的頂列不用毛玻璃，離開頂端改成不透明的面板色。
- App 外框的底部導覽列與 iOS 的返回圓鈕一律不透明（使用者 2026-11-15 決定：iOS 版照 Apple「降低透明度」時的實心樣子，不做玻璃），用 1px 描邊加陰影分層，**不是**半透明例外。
- 站名只寫英文「MaiMotionDemo」（圓體 M PLUS Rounded 1c ExtraBold，`.wordmark`），後面不接中文；標誌是不帶底色的向量圖（`Logo.svelte`），環跟著文字顏色。
- 章節標題（側欄、選單抽屜、首頁章節清單）比頁面連結高一階：側欄 15px／700、`--c-text-strong`、`--font-display`（觸控螢幕 19px），首頁 19px／700；字的下半部後面墊一條不透明的色帶，像螢光筆劃過（`docs.css` 的 `.section-mark`，用 `::after`，不是漸層也不是左側邊框；顏色和背景色混過，壓在上面的字才看得清楚）。色帶的顏色由 `nav.ts` 的 `Section.tone` 決定：`guide`（入門導覽：認識 MaiMotionDemo、基礎操作演示）用右手藍 `--c-right`，`detail`（其餘各章，細節）用左手粉 `--c-left`，讓讀者看出「從這裡開始是細節」。元件裡不寫死章節 id。入門導覽的最後一章和第一個 `detail` 章節之間放一條分隔標示「完整手冊」（`nav.ts` 的 `MANUAL_LABEL`、`startsManual()`）：側欄與選單是小字加左右各一條 1px 線，首頁章節清單是和「章節」同樣的小標。內容頁上方的章節名、上一頁／下一頁只是文字，不加色帶。中文沒有打包的標題字型（中文走系統字型，圓體只收英文字母），所以章節標題靠字重、字級與字色區別。

### 版面與手機版

斷點與做法照 BOTPu admin：

| 寬度 | 版面 |
| --- | --- |
| `< 768px` | 手機：頂列（標誌＋站名，右邊「搜尋」「選單」兩顆圓鈕），高 `--topbar-h` 加 `env(safe-area-inset-top)`，往下捲時自動收起；「選單」開成底部抽屜（標題「目錄」），「搜尋」是全螢幕。沒有底部分頁列 |
| `≥ 768px` | 頂列常駐（sticky）＋左側欄（章節目錄，sticky，100dvh）＋內容 |
| `≥ 1280px` | 再加右側「本頁內容」目錄（sticky）；較窄時改成內容上方可收合的一塊 |

- 手機頂列（`Topbar.svelte`）：`position: sticky; top: 0`。捲動位置超過一個頂列高、且這次往下累積超過 8px 時用 `transform: translateY(-100%)` 收起；往上累積超過 8px 滑回來；在頂端一定顯示。搜尋或選單開著、鍵盤焦點進到頂列裡、換頁之後也一定顯示（鍵盤焦點在頂列裡時不收起）。捲動監聽是 passive，用 `requestAnimationFrame` 節流；轉場只動 `transform`（220ms、`--ease-out`），減少動態時瞬間完成。在頂端時和背景同色，離開頂端後和桌面一樣是毛玻璃加陰影。
- 選單抽屜（`MenuSheet.svelte`，用 `Sheet`）：最上面一排不跟著捲動的工具（外觀切換、「下載 App」、GitHub；可以安裝時再多一列「加到主畫面」（`$lib/pwa/InstallEntry.svelte`），不能安裝時它自己不顯示），下面是章節目錄（開啟時捲到並聚焦目前這一頁；點了連結就關）。Esc、點框外、關閉鈕都能關，焦點回到「選單」按鈕。
- 對話框與抽屜開著時鎖住整頁捲動（`html` 設 `overflow: hidden`，原本有捲軸就用 `scrollbar-gutter: stable` 保留寬度），關閉後捲動位置不變。
- 錨點：手機的 `scroll-padding-top` 含頂列高度（頂列可能正顯示著），跳到標題時不會被蓋住。
- `viewport-fit=cover`；貼齊螢幕邊緣的東西都加 safe-area（頂列的上緣、頁尾的下緣、抽屜的底部）。
- `@media (pointer: coarse)`：控制項高度 44px；輸入框字級 `max(16px, …)`（避免 iOS 放大）。
- hover 樣式只寫在 `@media (hover: hover)` 裡。
- 表格外包 `.table-wrap`（container query）：容器寬 `< 560px` 時每列變成一張直排清單，`td` 以 `data-label` 顯示欄名。整頁永遠不能水平捲動。
- 對話框沒有半透明遮罩（`::backdrop { background: none }`），靠邊框與陰影分層。

#### App 外框（從手機主畫面開啟）

從主畫面開啟（`display-mode: standalone`，iOS 另看 `navigator.standalone`）時，iOS 與 Android 各換成一套像原生 App 的外框：頂列精簡、底部多一條導覽列。一般瀏覽器裡的外框（上表）完全不變；電腦上安裝成 web app 也不換。

- 判斷只有一份：`src/lib/app/platform.svelte.ts` 的 `platform.app`（`'ios' | 'android' | null`），外框在 `+layout.svelte` 的 onMount 呼叫 `initPlatform()`。`src/app.html` 有一段行內腳本在第一次繪製前用同樣的規則設定 `<html data-app="ios|android">`，**兩邊規則要一起改**。CSS 一律看 `:root[data-app]`，不看視窗寬：兩套外框的元件都在頁面上，由 CSS 決定顯示哪一套，所以第一次繪製就是對的外框、不會閃。
- 預覽：網址加 `?app=ios` 或 `?app=android` 可以在一般瀏覽器（含電腦）看 App 外框，`?app=off` 取消；記在 sessionStorage，只影響這個分頁。電腦上預覽時內容、頂列、底列與抽屜都限在手機欄寬 `--app-col`（560px）並置中。
- 元件：`src/lib/app/AppTopbar.svelte`（頂列）、`TabBar.svelte`（底部導覽列）、`AppTocSheet.svelte`（「目錄」抽屜）、`AppMoreSheet.svelte`（「更多」抽屜）、`AppPanels.svelte`（面板的非 modal 行為，見下）、`panel-ids.ts`（三個面板的 id）。一般外框的 `Topbar`、`MenuSheet`、左側欄在 App 外框下不顯示。頂列「往下捲收起」的規則和手機頂列共用 `src/lib/shell/autohide.svelte.ts`。
- 頂列：只有標誌與站名，不是連結、沒有搜尋與選單；收起與出現和手機頂列一樣，上方加 `env(safe-area-inset-top)`。
  - iOS：標誌與站名靠左（使用者指定，不照 iOS 慣例置中）、高 56；不在首頁時最左邊一顆 44×44 的返回圓鈕，站名接在它右邊（只有 chevron、`aria-label="返回"`），有站內的上一頁就 `history.back()`（有 Navigation API 時看 `navigation.canGoBack`，沒有就用記在 `page.state` 的站內深度），沒有就回首頁。
  - Android：標誌與站名靠左（Material 的 top app bar：高 64、標題 22px），不放返回鈕（交給系統返回手勢）。離開頂端後整條換成不透明的 `--c-panel`（和底列同色，M3 用色調分層、不用陰影），系統狀態列（`theme-color`）跟著換（`theme.setChrome()`）。
- 底部導覽列：首頁（連結）、目錄、搜尋、更多（`<button aria-haspopup="dialog">`），`<nav aria-label="主要導覽">`。選中：首頁在 `/`、目錄在內容頁、更多在 `/download`；抽屜或搜尋開著時改成那一顆。常駐、不隨捲動收起；頁尾下面留 `--tabbar-space`（已含 safe-area），`html` 的 `scroll-padding-bottom` 也是它。已經在首頁再點「首頁」會捲回最上面。
- 面板（目錄、搜尋、更多）開著時底列一直看得到、按得到（使用者 2026-11-15 決定）：
  - 點開著的那一顆就關掉；點另一顆直接切換（兩個面板都不播進出場動畫，`Sheet` 的 `is-instant`）；點「首頁」關掉面板並回首頁（已在首頁就關掉並捲到頂）。選中狀態跟著開著的面板，關掉後回到目前頁面那一顆。
  - 版面：`Sheet` 在 App 外框改成 `position: fixed` 鋪滿視窗的一層，疊在頂列之上、底列之下（`z-index: --z-tabbar − 1`）。iOS 的面板延伸到螢幕底、膠囊浮在上面，面板內容底部留 `--app-panel-pad`（＝`--tabbar-space`）；Android 的面板下緣接在底列上方（`--app-panel-gap`）。搜尋一樣是全螢幕，只是不蓋住底列。
  - 非 modal：`Sheet` 在 App 外框用 `dialog.show()`（不是 `showModal()`，不然底列會失效），modal 原本給的由 `AppPanels.svelte` 補上：面板與底列以外的整頁設 `inert`（從 body 往下走，包含它們的元素往裡走、其他整塊設 inert）、捲動照舊由 `Sheet` 鎖住；Tab 只在「面板 → 底列」之間循環；焦點在底列時 Esc 也會關；關掉後焦點回到打開它的那一顆（從內文用 Ctrl+K 打開就回到原處）；點面板外露出的地方（頂列區域）會關。面板是 `<dialog>`（role dialog、`aria-labelledby` 指標題，沒有 `aria-modal`），底列按鈕有 `aria-expanded` 與 `aria-controls`。
  - 系統返回：打開面板時推一筆淺層紀錄（`goto(…, { shallow: true, state: { appPanel: true } })`），按返回回到前一筆就關面板；從介面關掉時自己 `history.back()` 收掉它。在面板裡點連結或選搜尋結果換頁時，改成「取代」那一筆（`beforeNavigate` 取消後用 `replace: true` 重導），換頁後按返回直接回到原本那一頁。點連結的當下（capture 的 click）與搜尋的 `ui.navigatingAway` 用來判斷「這次關面板是要換頁」，不先收紀錄（SvelteKit 的 `beforeNavigate` 是非同步的，等它來就太晚了）。淺層紀錄不影響返回鈕的「有沒有上一頁」（`navigation.canGoBack` 只看目前位置；備用的站內深度在淺層換頁時不變）。
  - 安裝提示與「有新版本」提示（`$lib/pwa/Notice.svelte`）的位置由 `--app-nav-clearance` 決定：底列上緣再留 8px。
  - iOS：浮在內容上的實心膠囊：高 62、圓角 31、左右離螢幕 21、`bottom: max(12px, calc(env(safe-area-inset-bottom) - 13px))`；底色 `--c-panel`、1px `--app-chrome-edge`（深色是 `--c-border-strong`，浮在黑底盤面上也看得出邊界）、陰影 `--shadow-pop`。圖示 24、標籤 10px／600、間距 1；未選中 `--c-text`，選中 `--app-tab-active`（深色＝`--c-primary`；淺色把主色混 20% `--c-text-strong` 加深，過 4.5:1），背後一塊內縮 3px 的 `--c-control-hover` 膠囊，換選中時以 350ms `--ease-spring` 滑過去。按下整顆縮 0.94。
  - Android：Material 3 的 navigation bar：貼底滿寬、高 64，上緣 1px `--c-border`、底 `--c-panel`。每格：上 6 → 指示器 56×32（`--app-indicator`：主色 24% 混進面板色）→ 間距 4 → 標籤 12／16、500、字距 .5px → 下 6。未選中 `--c-text-dim`，選中 `--c-text-strong`（標籤不加粗）。指示器選中時從中心水平展開（240ms `--ease-spring`、透明度 150ms）。狀態層 hover 8%、按下與聚焦 10%（混成不透明色，只在指示器範圍內）。手勢列 inset 用 Chrome 建議的 `bottom: calc(inset − max-inset)`＋`padding-bottom: max-inset`；`safe-area-max-inset-bottom` 不支援時，巢狀的 `env()` 退回一般的 `safe-area-inset-bottom`。
- 頁面自己的寬螢幕版面（首頁兩欄、內容頁右側「本頁內容」）在 App 外框下由 `docs.css` 蓋回手機的樣子（平板或電腦預覽時才用得到）。
- `prefers-reduced-motion` 時選中膠囊與指示器不動畫、按下不縮；`prefers-contrast: more` 時描邊加粗。

## 內容格式

檔案：`src/content/<資料夾>/<slug>.md` → 網址 `/<資料夾>/<slug>`。一章可以涵蓋多個資料夾（`nav.ts` 的 `dirs`），所以網址的第一段是內容資料夾，不一定是章節 id；合併或調整章節時不搬檔案，網址就不會變。

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

- 章節（`section`）、顯示名稱與順序固定在 `src/lib/nav.ts`：`intro` 認識 MaiMotionDemo、`quick` 基礎操作演示、`start` 開始使用（涵蓋 `content/start/` 與 `content/charts/` 兩個資料夾：兩個資料夾的頁面共用一組 `order`、穿插排列：下載與啟動、介面導覽、新增譜面、搜尋匯入、第一次播放、看懂盤面、simai 語法、譜面紀錄、匯出與匯入標記檔；`/charts/…` 的網址照舊）、`play` 播放與檢視、`annotate` 真人手順標註、`model` 模型原理、`more` 其他。同一章依 `order` 排，不分資料夾（order 相同才依 `dirs` 的順序）；上一頁／下一頁跨章時標題前面加章名，同章（例如 `/start/interface` → `/charts/new`）不加。
- `SECTIONS` 一章寫成一行：`scripts/build-index.mjs` 逐行讀 `id`、`title`、`dirs` 當搜尋索引的章節名。`markdown.js` 依資料夾自動 import 元件的規則（`SECTION_COMPONENTS`、`SECTION_EXTRA_IMPORTS`）的 key 是內容資料夾，同一章的資料夾要一起列（`start` 與 `charts` 都能用 `<Mascot>`）。
- `intro` 是給還不知道這個 App 在做什麼的人的導覽（這是什麼、怎麼推算手順、為什麼需要真人標註、標註原則、五步上手），內容來自合作夥伴的介紹文，只講概念、不放操作細節；第一頁開頭有「想直接上手」的提示連到〈第一次播放〉，`start` 第一頁開頭有一句回指。首頁的主要按鈕連到 `intro` 第一頁，「閱讀手冊」是第二顆按鈕。
- 只用 `##` 與 `###`；每個 `##` 是搜尋索引的一塊，所以一塊講一件事，標題用使用者會問的說法。
- 文案：繁體中文（台灣用語）、全形標點、口語、不討好；按鈕與分頁名稱用「」括起來，和 App 畫面上的字完全一致；鍵盤按鍵用 `<kbd>A</kbd>`；simai 範例用 ```` ```simai ```` 程式碼區塊（版面會加上複製鈕）。
- 提示框：`<Callout tone="info|warn" title="選填">內文</Callout>`。一頁最多兩三個。
- `intro` 章節另有專屬元件（`src/lib/intro/`）：`content/intro/*.md` 會自動 import 這個資料夾裡實際存在的每個元件（`markdown.js` 的 `introImports`，檔名＝標籤名），其他章節不會。
  - `<IntroProgress />`：每頁最上面的導覽進度（章節各頁的編號圓點，頁面清單取自 `nav.ts`）。
  - `<Steps>`＋`<Step icon="…">`：有編號的步驟（`<ol>`）；`<Steps flow>` 是流程圖（圖示之間有連接線、依序淡入）；`<Step key>` 是重點（淡主色的一塊），`layer="…"` 加分層標籤。
  - `<Features>`＋`<Feature icon="…">`：並排的重點（`<ul>`），寬時多欄、窄時一欄且圖示放左邊。
  - `<LinkRows>`＋`<LinkRow href="/…">想知道什麼</LinkRow>`：整列可點的連結，頁名依 `href` 從 `nav.ts` 取。
  - `icon` 是 App `Icon` 或站內 `ExtraIcon` 的名字（`IntroIcon.svelte` 自動選）。
  - **內文一定寫在 .md 裡、在標籤之間前後各空一行**（mdsvex 才會當成 Markdown；搜尋索引會去掉標籤、保留內文）。標籤屬性裡的字不會進索引，只放圖示名、分層標籤這種重述性質的短字。
  - 實驗：`<Mascot>`（標籤與每段話之間都空一行；一段話一個泡泡，像聊天軟體連續好幾則）是用標誌當吉祥物的對話框，只放補充或重述，正文不能只在這裡。要整個拿掉：刪掉 `content/intro/` 裡的 `<Mascot>` 標籤和 `src/lib/intro/Mascot.svelte`，不用改其他地方。
- `quick` 是給完全新手的超簡易版：一步一頁（下載、開啟 App、搜尋第一張譜面、開始標記、多選匯出），每頁只有標題、一句話（description）、一段實際畫面導覽、3～5 條很短的編號步驟、一句連到完整說明的連結與「下一步」。不解釋原理、不列例外、不用表格。`content/quick/*.md` 會自動 import `src/lib/quick/` 裡的元件（和 intro 同一套機制，`markdown.js` 的 `SECTION_COMPONENTS`）：
  - `<QuickTour flow="search" />`：實際畫面導覽（徽章「實際畫面」）。截圖是實際 App 拍的，不是重畫的介面：`scripts/capture-quick.mjs` 用 CDP 驅動已發布的桌面 App（隔離的空白 WebView2 設定檔，啟動時紀錄數必須是 0），逐步操作、截圖（1280×800、deviceScaleFactor 2、webp，放 `static/shots/quick/`），並用 `getBoundingClientRect` 量出每一步要按的位置，存成 `src/lib/quick/shots/<flow>.json`。旁白在 `src/lib/quick/flows.ts`（依步驟 id，改字不用重拍）。重拍方法與安全規則寫在腳本檔頭。
  - 示範用的歌（搜尋與標記用的主角、多選匯出的第二筆）只寫在 `capture-quick.mjs` 的 `SONGS`；換歌只改那裡再重拍 search、annotate、export，旁白與 .md 不寫歌名。
  - 五個流程：`download`（網站自己的 `/download`，網頁模式：用 headless Edge 拍，沒有開著的 dev server 就自己開一個、拍完關掉）、`open`（沿用 `search-home.webp`，不另存）、`search`、`annotate`（App 模式，一次啟動依序拍）、`export`（自己從乾淨的 App 開始：前面跑過別的流程就關掉 App、用新的空設定檔再啟動一次；先載入第二筆的歌、再載入主角，主角自然排在清單第一筆，不動排序選項）。`--flow <名稱>` 可重複，只拍指定的流程；前置流程（`annotate` 要先 `search`）會「空跑」：照樣操作 App，但不存圖、不寫 json。只重拍多選匯出：`--flow export`（不會動 search、annotate 的圖）。`--site`（預設 `http://localhost:4330`）、`--edge`（msedge.exe 路徑）給網頁模式用。`export` 點哪一筆用歌名比對決定，清單第一筆不是主角、選單沒開在主角那一筆、或兩筆沒都選到就失敗；只框出選取工具列的匯出鈕、不按（不開存檔對話框、不寫檔案）。
  - **發新版後下載頁要重拍**（`--flow download`）：截圖裡的版本號與按鈕是拍的時候的下載頁。
  - 步驟的種類由 `ShotStep` 的欄位決定（型別在 `flows.ts`）：`target` 游標飛過去按下（`double` 連按兩下）；`key` 不出現游標、截圖框下方那一列的鍵帽（`Tour.keys`，`Keycap` 的 `still` 版，不蓋在截圖上；有 `keys` 的導覽整段都有這一列，高度不跳）亮一下；〈開始標記〉按哪個鍵由擷取時面板上的模型建議（「模型：L／R」；Slide 的起點與滑行各算一步、不停在 Slide 中間）決定、寫進 manifest，旁白由 `flows.ts` 依 key 產生，不寫死；`highlight` 只框起來不按；有 `after` 就換成那張畫面。拍不到的系統畫面（在檔案總管連按兩下）用 `Tour.lead` 接在最前面：示意畫面（`sketch`，元件在 `src/lib/quick/parts/`、版面在 `src/lib/quick/sketch.ts`），圖說的徽章跟著畫面切換：示意畫面標「示意」，截圖標「實際畫面」。`parts/` 是子資料夾，不會被 .md 自動 import。
  - 旁白由吉祥物講（`src/lib/quick/parts/TourSay.svelte`：泡泡在左、站上的標誌在右）：寬的時候疊在截圖右下角，這一步要按（或框起來）的地方被擋到就改放右上角（新句子出現時決定、同一句中途不跳）；窄的時候（容器寬度 < 560px）改成截圖正下方的一列（鍵帽列在它下面），保留兩行高。換句時舊句子往上淡出（130ms），新句子一個字一個字淡入（整句約 0.25 秒內看得完整）；減少動態時直接換字。`Stage` 以 `narration="hidden"` 把下方那一列的旁白改成視覺隱藏（仍是 aria-live，給輔助技術念），那一列只剩控制鈕。
  - 播放沿用演示的 `DemoPlayer` 與 `Stage`（進入畫面才播、離開暫停、不循環、重播）；另有「上一步／下一步」（`DemoPlayer.seek`）。鏡頭只動截圖那一層的 `transform`，拉近倍率依舞台寬度算（App 的字在畫面上約是原尺寸的 0.95～1.2 倍）；減少動態時不拉近、不飛游標，停在第一步，一步一張整個畫面加框。不是經由 `<Demo>` 延遲載入，所以預先算繪的 HTML 就有第一張圖（沒有 JS 也看得到），高度由截圖的長寬比固定。
  - 每頁最後一句（連到詳細說明）用 intro 的 `<Mascot>` 講（`markdown.js` 的 `SECTION_EXTRA_IMPORTS` 讓 quick 章也能用）；頁尾不放「下一步」按鈕，靠上一頁／下一頁。
- 演示：`<Demo id="swap" />`，可加 `caption="…"`。可用的 id 見下表。不需要寫 import（建置時自動注入）。
- 內部連結用絕對路徑：`[影片同步](/annotate/video)`。

## 演示（`src/lib/demos/`）

`Demo.svelte` 依 `registry.ts` 的 id 延遲載入對應元件（元件清單在 `loaders.ts`，由 `Demo.svelte` 以 `import()` 載入，不要改成靜態 import，否則用到 `<Demo>` 的頁面會在 `<head>` 多載所有演示的 CSS）；元件檔不存在時顯示「演示製作中」的佔位框（只在開發時）。

載入前的預留高度登記在 `registry.ts` 的 `reserve`：盤面演示用一條直線（高 = k × 寬 + b）；示意動畫的高度隨寬度是階梯狀，用 `steps` 分段（`Demo.svelte` 轉成 `@container` 規則）。改了演示的版面、旁白或按鈕，要重量並更新數字，否則載入時版面會跳。

兩種演示，畫面上要標清楚：

- **盤面演示**（徽章「核心實際結果」）：資料由 `scripts/gen-demos.mjs` 呼叫 Rust 核心（`target/release/mai-motion-core.exe`，stdin 是 `AnalyzeRequest` JSON）以 App 的預設參數、評分方式固定基座 V4.3 算出（App 預設的浅葱色 TemotoAI-2 會再修正部分手順；結果和它不同的段落，文件要寫明是 V4.3），存成 `src/lib/demos/data/<id>.json`（已精簡、進版控）。前端只播放，不重算左右手。
- **示意動畫**（徽章「示意」）：用來解釋介面操作或概念的手繪動畫，數字是示意用的。

共同規則：進入可視範圍才開始播、離開就暫停；有播放／暫停鈕與重播；`prefers-reduced-motion` 時不自動播，停在最有代表性的一格並保留手動控制；寬度 100%，375px 寬也要能看能按；動畫只用 `transform`、`opacity`、SVG 屬性與既有曲線（`--ease-out`、`--ease-spring`）。

### 盤面演示 id

| id | simai | 用在 |
| --- | --- | --- |
| `tap-alternate` | `(120){4}1,8,2,7,3,6,4,5,E` | 首頁、這是什麼、第一次播放 |
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

## Web App（PWA）

手機可以把手冊加到主畫面，當成 App 開、離線也能看。程式在 `src/lib/pwa/` 與 `src/service-worker/`；環境判斷（手機、standalone、App 內建瀏覽器）一律讀 `src/lib/app/platform.svelte.ts`。

- **manifest 與圖示**：`static/manifest.webmanifest`（`id`／`start_url`／`scope` 都是 `/`、`display: standalone`、`background_color` 是深色的頁面背景 `#0b0d11`（啟動畫面用）、`theme_color` 是深色的面板色 `#14181e`——Android 上已安裝的 App，系統最底下的導覽區是照 manifest 的 `theme_color` 上色的，設成和底部導覽列同色才接得起來（manifest 沒有淺色／深色兩組的寫法，所以淺色外觀下那一區仍是深色；狀態列不受影響，它跟著頁面的 `<meta name="theme-color">` 變）；捷徑「搜尋」開 `/?search=1`、「下載」開 `/download`）。圖示由 `npm.cmd run icons`（`scripts/gen-icons.mjs`，純 Node、不需要其他套件）依 `Logo.svelte` 的比例畫：`icon-192/512.png`（any，滿版深色底）、`icon-maskable-192/512.png`（圖形縮在中心直徑 80% 的安全圓內）、`apple-touch-icon.png`（180×180、不透明）。改了標誌就重跑，產出進版控。`<link rel="manifest">` 由 `InstallPrompt.svelte` 的 `<svelte:head>` 加上。不設 `apple-mobile-web-app-status-bar-style`（網站有深淺兩色，安裝後改不了）。
- **service worker**（`src/service-worker/index.ts`，型別檢查用同資料夾的 tsconfig，`npm.cmd run check` 會一起跑）：SvelteKit 只在正式建置時打包它，`vite dev` 沒有。設定是 `vite.config.ts` 的 `serviceWorker: { register: false }`，由 `src/lib/pwa/register.ts` 的 `registerPwa()` **只在手機**註冊；桌面瀏覽器不註冊，已有的註冊會取消。
  - 快取規則：`/_app/immutable/*` cache-first；HTML network-first（3 秒沒回應先給快取，有 navigation preload 就用）；`/search/chunks.json` stale-while-revalidate；其他 static 檔 network-first；`/api/*`、`/search/vectors.bin`、跨網域、非 GET 不攔截（離線時搜尋只剩字面結果）。離線又沒快取的頁面回 `/offline`（`static/offline.html`，樣式寫在檔案裡）。快取比對忽略 `Vary`（Workers 靜態檔回 `Vary: Origin`，module script 帶 Origin、預抓不帶）。
  - 一般手機瀏覽器只存讀過的頁面（第一次安裝時把當下這頁補存）。安裝成 App（standalone）後在背景把全部預先算繪的頁面與靜態檔抓進快取（約 180 個檔、2.6 MB；同一版只做一次），並申請持久儲存。
  - 快取名稱帶建置的 `version`；新版啟用時刪掉舊版，改版前讀過的頁面記下來重新抓（App 裡則重新完整預抓）。
  - 改版：新 SW 裝好後先等待。App 裡顯示「有新版本」，點「重新整理」才換新並重新載入；一般瀏覽器直接讓新版接手、不重新載入、不打擾。App 回到前景與換頁時檢查更新（一分鐘最多一次）。
  - `_headers`（`worker/vite.config.ts` 產生）：`/service-worker.js` 是 `Cache-Control: no-cache`、`/manifest.webmanifest` 是 `application/manifest+json`（都先用 `! 標頭` 拆掉再設，同名標頭多條規則會串接）；CSP 有 `worker-src 'self'`、`manifest-src 'self'`。
- **安裝入口**（`InstallEntry.svelte`）：不自動跳出提示。手機（`<html data-os>`，由 `app.html` 在第一次繪製前設定）的首頁把「下載」換成「安裝手冊到手機」，選單抽屜另有「加到主畫面」；已裝成 App 或在電腦上自己不顯示，桌面程式的下載入口在手機上只留在選單裡。點了之後：Android／Chromium 有 `beforeinstallprompt` 就直接叫出系統的安裝視窗，還沒拿到事件（或瀏覽器不支援）就顯示「從瀏覽器選單安裝」的步驟；iOS Safari 與 iOS 上的其他瀏覽器顯示「加入主畫面」步驟；App 內建瀏覽器（LINE、Instagram…）改成「請用瀏覽器開啟」（Android 用 intent 開 Chrome、LINE 加 `openExternalBrowser=1`、iOS 複製連結）。這些畫面在 `InstallPrompt.svelte`。
- **緊急開關**（快取出問題、要讓所有人立刻回到沒有 SW 的狀態）：用環境變數建置一版並部署，

  ```powershell
  $env:PWA_KILL_SWITCH = '1'; npm.cmd run build:all; Remove-Item Env:PWA_KILL_SWITCH
  cd worker; npx.cmd cf deploy
  ```

  這版的頁面一載入就取消 SW 註冊、刪掉所有 `mm-docs-*` 快取，也不再註冊（HTML 是 network-first，連線時一定拿到這版）；瀏覽器若先檢查到新的 SW，那支 SW 也會立刻接手、清空快取、取消註冊並重新載入開著的頁面。實測：第一次載入時取消註冊，舊 SW 仍控制著那一頁，下一次載入就完全乾淨。問題修好後照常建置部署（不設這個變數），手機會重新註冊。瀏覽器最慢約 24 小時內一定會檢查 SW 是否更新（`/service-worker.js` 本身不被快取）。

## 指令（PowerShell，在 `site/` 底下）

```powershell
npm.cmd install
npm.cmd run dev          # http://localhost:4330（沒有 Worker：搜尋只有字面結果）
npm.cmd run check        # svelte-check＋service worker 的 tsc，要 0 errors／0 warnings
npm.cmd run build        # 輸出到 site/build
npm.cmd run demos        # 重新產生盤面演示資料（需要先 cargo build --release --bin mai-motion-core）
npm.cmd run icons        # 重新產生 Web App 圖示（static/icon-*.png、apple-touch-icon.png）
node scripts/capture-quick.mjs --exe <App 執行檔> --profile <新的空資料夾>   # 重拍「基礎操作演示」的實際畫面（App 要先關掉；規則見檔頭；--flow 只拍指定的）
node scripts/capture-quick.mjs --flow download                              # 發新版後重拍下載頁（網頁模式，不用開 App）
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
