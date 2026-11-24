# MaiMotionDemo 使用手冊

本手冊描述 v1.3.1 的預設參數。

這裡只放快速開始與重點。完整手冊（新手教學、各功能詳解、動畫演示與全文搜尋）在文件站：[maimotiondemo.bapu.dev](https://maimotiondemo.bapu.dev)。

## 快速開始

### 下載與啟動

從 [GitHub Releases](https://github.com/silver0416/MaiMotionDemo/releases/latest) 下載對應平台的檔案，不需安裝：

- Windows：`MaiMotionDemo-vX.Y.Z-windows-x64.exe`，下載後直接開啟。缺少 WebView2 時，請安裝 [Microsoft WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)。
- macOS：`-macos-universal.zip`，解壓後開啟。
- Linux：`-linux-x64.AppImage`（加上執行權限後開啟）或 `.deb`。

macOS 與 Linux 版由 GitHub Actions 自動建置，未簽章：macOS 第一次開啟請在 Finder 按右鍵選「打開」。程式內自動更新目前只支援 Windows，其他平台請自己到 GitHub 下載新版。細節見[下載與啟動](https://maimotiondemo.bapu.dev/start/install)。

### 第一次播放

1. 按左側「新增」。
2. 貼上範例：

   ```text
   (120){4}1,8,2,7,3,6,4,5,E
   ```

3. 按「生成並新增」（或 `Ctrl`+`Enter`）。
4. 按「播放」。
5. 粉紅色的 **L** 是左手，藍色的 **R** 是右手。

接下來可以試試 0.5× 慢放、拖曳時間軸，或切到「方案」比較其他打法，見[第一次播放](https://maimotiondemo.bapu.dev/start/first-play)。

## 想知道更多

| 章節 | 內容 | 常用頁面 |
| --- | --- | --- |
| [開始使用](https://maimotiondemo.bapu.dev/start/install) | 安裝、認識介面，貼上 simai、拖檔案、從 simai Wiki／adxdls／Majdata 搜尋，第一次播放與看懂盤面，以及整理譜面紀錄。 | [下載與啟動](https://maimotiondemo.bapu.dev/start/install)・[介面導覽](https://maimotiondemo.bapu.dev/start/interface)・[新增譜面](https://maimotiondemo.bapu.dev/charts/new)・[搜尋匯入](https://maimotiondemo.bapu.dev/charts/search)・[第一次播放](https://maimotiondemo.bapu.dev/start/first-play)・[看懂盤面](https://maimotiondemo.bapu.dev/start/disc)・[simai 語法](https://maimotiondemo.bapu.dev/charts/simai)・[譜面紀錄](https://maimotiondemo.bapu.dev/charts/records)・[匯出與匯入標記檔](https://maimotiondemo.bapu.dev/charts/bundles) |
| [播放與檢視](https://maimotiondemo.bapu.dev/play/transport) | 播放、循環片段與時間軸標籤，比較方案、查看音符與調整顯示。 | [播放與時間軸](https://maimotiondemo.bapu.dev/play/transport)・[方案](https://maimotiondemo.bapu.dev/play/solutions)・[音符](https://maimotiondemo.bapu.dev/play/notes)・[顯示](https://maimotiondemo.bapu.dev/play/view) |
| [真人手順標註](https://maimotiondemo.bapu.dev/annotate/basics) | 自己標出左右手、記下不同打法、對照手元影片，和模型比一比。 | [逐顆標註](https://maimotiondemo.bapu.dev/annotate/basics)・[用模型預填](https://maimotiondemo.bapu.dev/annotate/prefill)・[打法分支](https://maimotiondemo.bapu.dev/annotate/branches)・[影片同步](https://maimotiondemo.bapu.dev/annotate/video)・[和模型比對](https://maimotiondemo.bapu.dev/annotate/compare) |
| [模型原理](https://maimotiondemo.bapu.dev/model/overview) | 模型怎麼打分數、各評分方式的差別、可以調整的參數，以及略過與沒有方案的原因。 | [模型在想什麼](https://maimotiondemo.bapu.dev/model/overview)・[評分方式](https://maimotiondemo.bapu.dev/model/scoring)・[參數](https://maimotiondemo.bapu.dev/model/params)・[Slide 怎麼打](https://maimotiondemo.bapu.dev/model/slide)・[略過與沒有方案](https://maimotiondemo.bapu.dev/model/skipped) |
| [其他](https://maimotiondemo.bapu.dev/more/faq) | 常見問題、快捷鍵、設定、更新與從原始碼建置。 | [常見問題](https://maimotiondemo.bapu.dev/more/faq)・[快捷鍵](https://maimotiondemo.bapu.dev/more/shortcuts)・[設定](https://maimotiondemo.bapu.dev/more/settings)・[更新](https://maimotiondemo.bapu.dev/more/update) |

## 從原始碼執行

準備 Rust、Microsoft C++ Build Tools、WebView2 及 Node.js，在專案根目錄的 PowerShell 執行：

```powershell
npm.cmd ci
npm.cmd run tauri dev
```

建立正式執行檔：

```powershell
npm.cmd run tauri build -- --no-bundle
```

驗證過的版本、輸出位置與常見問題見[從原始碼建置](https://maimotiondemo.bapu.dev/more/build)。

## 注意

- 結果是依規則推算的打法建議，不代表官方判定或唯一正確打法；分數也不是成功率。
- 「未找到可行方案」不表示譜面無法遊玩，只是目前的模型做不到。
- 分析在本機完成。每次開啟會連到 GitHub 檢查更新；搜尋譜面時會連線 simai Wiki、adxdls 或 Majdata；使用影片同步時會從 GitHub 下載 yt-dlp（與 Deno），並連線 YouTube 搜尋、下載影片，影片只存在本機。影片視窗的「自動預填」第一次使用時會下載姿態偵測模型。
