# MaiMotionDemo

以 Rust + Tauri + Svelte 製作的 maimai 左右手動作分析 Demo。輸入支援範圍內的 simai 譜面，展示候選打法、觸碰與滑行軌跡，以及中途換手。

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

- 外圈 Tap、Hold、直線／圓弧 Slide 與同時音。
- 依距離、速度、姿態及換手成本搜尋最多三個候選方案。
- 左右手動作、滑行交接、逐時定位、播放倍率及循環片段。
- 成本拆解、音符檢視與盤面校準。

這是幾何啟發式 Demo，尚未完整支援 simai；不代表官方判定或人類唯一正解。Touch、Wifi、Break、EX 與複合滑軌目前會明確回報未支援。

## 驗證

```powershell
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
npm.cmd run check
npm.cmd run build
```

Rust 核心可獨立執行；`cargo run --offline --example fixtures` 可重建前端範例資料。

資源：`resource/maimai.png` 為圓盤背景；`resource/Maimai_notes.png` 為音符形狀參考。
