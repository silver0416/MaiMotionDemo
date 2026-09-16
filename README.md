# MaiMotionDemo

**把 simai 譜面變成可播放的左右手動作動畫。** MaiMotionDemo 是 Windows 桌面展示工具：在 maimai 圓盤上呈現 Tap、Hold、Touch 和 Slide 的接觸與移動軌跡，並比較不同的雙手分配方案，包括 Slide 中途換手。

[下載 Windows x64 單一執行檔](https://github.com/silver0416/MaiMotionDemo/releases/download/v0.1.0/MaiMotionDemo-v0.1.0-windows-x64.exe) · [查看所有版本](https://github.com/silver0416/MaiMotionDemo/releases) · [完整使用手冊](USER_GUIDE.md)

## 下載與啟動

從 GitHub Release 下載 `MaiMotionDemo-v0.1.0-windows-x64.exe`，放在任何可寫入的位置後直接開啟。畫面與分析核心都包含在執行檔裡。

支援 Windows x64。介面使用系統的 Microsoft Edge WebView2；如果 Windows 缺少 WebView2 Runtime，請先安裝 [Microsoft 官方 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)。

## 快速展示

1. 在「編輯」選擇「基本 Tap」範例，按「生成」與「播放」。盤面上粉紅圓形 **L** 是左手、藍色方形 **R** 是右手。
2. 改選「可行交接」，慢放並拖曳時間軸，觀察左手將 Slide 交給右手的區間。
3. 在「方案」比較候選打法及成本；在「參數」調整移動、跨側和換手的偏好，重新生成後比較結果。

「Touch 區」範例會顯示 A／B／C／D／E 落點、Touch Hold 的按住進度與 `f` 煙火。可在「顯示」切換 33 個落點標記與煙火；落點座標由 Rust 分析核心提供。

「手掌覆蓋 Touch」範例讓一隻手掌同時覆蓋 C／B1／E1，另一手按鍵位 7。播放時大圓虛線是手掌覆蓋範圍；在「參數」可調整手掌半徑或關閉，在「顯示」可隱藏範圍，選取 Touch 可查看覆蓋明細。

也可以貼入自己的 simai 譜面本文，例如：

```text
(120){4}1-5[4:3],8,7,6,E
```

程式也接受整份 `maidata.txt`，會自動選擇難度編號最大的 `&inote_n` 並在診斷區顯示選取結果。Majdata 的 `<HS*倍率>` 顯示速度指令可一併貼入；它不改變雙手動作時間。Majdata 譜面可在檔案結尾省略 `E`，反引號間隔按 Majdata 的 128 分音計算。`&first=` 目前只會提示，不會自動套用。

## 功能

- 解析常用 simai 記法：BPM、分割與同時音；Tap、Hold、Touch、Touch Hold；Break、EX、星形與煙火修飾；`- ^ < > v V p q pp qq s z w` Slide 形狀、接續及同頭滑軌。遇到不認得的符號或無效的滑軌端點，畫面會標出錯誤位置。
- 以 Rust 幾何與動作成本規則搜尋最多三個雙手方案，呈現每隻手的移動、接觸時間、Hold 佔用和 Slide 交接區間。
- Slide 名目終點若同時出現下一組接觸，求解器可把最後一小段提早掃完，再以連續軌跡回位，避免要求手在終點瞬間傳送。
- 以 0.25× 至 2× 倍速播放，支援逐時定位、循環片段、音符細節、成本拆解及盤面顯示校準。
- 在本機分析譜面，不需登入或呼叫 AI API。

單次輸入上限為 10,000 個音符、3,600 秒及 4 MB 原文；過大的搜尋也可能因計算預算而停止。詳細語法、操作和例子見 [使用手冊](USER_GUIDE.md)，simai 原始格式見 [simai 說明](https://w.atwiki.jp/simai/pages/1002.html)。

## 如何理解分析結果

這套演算法依譜面幾何、動作時間與設定的成本規則，搜尋可行的左右手分配。較低的成本只代表目前參數下較受演算法偏好；候選搜尋會剪枝，因此不保證全域最佳，也不能將成本換算成人類使用左右手的機率。

演算法將每隻手簡化為一個接觸點。某些 Slide 曲線與 Touch 感應區使用可辨識的近似位置，並非實機軌道座標或官方判定。「未找到可行方案」也不表示玩家無法完成譜面。這些限制會影響方案與成本，請把動畫當作打法討論與資料分析的起點。

本機開發版已能讓一隻手掌覆蓋多個 Touch：除了同時出現的落點，持續中的 Touch Hold 也能在後續鄰近 Touch 到來時擴展成同一掌，例如用一手維持 C 並依序覆蓋 B 區，另一手繼續按鍵。預設以半徑 0.5 的圓形近似手掌，約占四分之一盤面。這不是官方判定或真實手形；Tap 與 Slide 不會自動算入同一掌。畫面上的小型 Touch 多邊形只用來辨識落點，手掌範圍另以大圓虛線表示。公開下載的 v0.1.0 尚未包含本機開發版變更。

## 從原始碼執行

開發與建置已在 Windows、Rust 1.90、Node.js 24 驗證。請先安裝 Rust、Microsoft C++ Build Tools、Node.js 和 WebView2 Runtime，再於專案根目錄使用 PowerShell：

```powershell
npm.cmd ci
npm.cmd run tauri dev
```

建立內嵌介面與 Rust 核心的單一正式執行檔：

```powershell
npm.cmd run tauri build -- --no-bundle
```

產物位於 `src-tauri/target/release/mai-motion-demo.exe`。單獨使用 `npm.cmd run dev` 開啟的瀏覽器畫面只有範例模式；要解析任意輸入，請執行 Tauri 桌面程式。

## 驗證

```powershell
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
npm.cmd run check
```
