# MaiMotionDemo

以 Rust + Tauri + Svelte 製作的 maimai 左右手動作分析 Demo。輸入 simai 譜面，展示候選打法、觸碰與滑行軌跡，以及中途換手。

## 開始使用

Windows 開發環境需有 Rust、Microsoft C++ Build Tools、WebView2 與 Node.js。已驗證 Rust 1.90、Node.js 24。

```powershell
npm.cmd ci
npm.cmd run tauri dev
```

打開「可行交接」範例，按播放，再到「方案」查看左右手分工與換手時間。

```powershell
npm.cmd run tauri build -- --no-bundle
```

建置後可直接開啟 `src-tauri/target/release/mai-motion-demo.exe`，不需啟動 Vite。詳細操作、語法及限制見 [使用手冊](USER_GUIDE.md)。

## 功能

- 完整的 simai 譜面語法：Tap、Hold、Touch（A–E 區）、Touch Hold、Break／EX／星形／煙火修飾，以及 `- ^ < > v V p q pp qq s z w` 全部 Slide 形狀、連續與同頭滑軌、無起點滑軌、疑似 EACH 與 `||` 註解。可直接貼上 `maidata.txt`。
- 依距離、速度、姿態及換手成本搜尋最多三個候選方案。
- 左右手動作、滑行交接、逐時定位、播放倍率及循環片段。整首譜面（近千個音符）可在一秒內分析完。
- 成本拆解、音符檢視與盤面校準。

這是幾何啟發式 Demo，不代表官方判定或人類唯一正解。Slide 的 `p` `q` `pp` `qq` `s` `z` 與 Touch 感應區位置是可辨識的近似形狀，不是實機軌道座標。

## 驗證

```powershell
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
npm.cmd run check
npm.cmd run build
```

Rust 核心可獨立執行；`cargo run --offline --example fixtures` 可重建前端範例資料。

資源：`resource/maimai.png` 為圓盤背景；`resource/Maimai_notes.png` 為音符形狀參考。
