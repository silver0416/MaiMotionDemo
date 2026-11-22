---
title: 下載與啟動
description: 下載 Windows、macOS 或 Linux 版，第一次開啟要注意的事。
order: 1
keywords: [安裝, 下載, exe, zip, 解壓縮, AppImage, deb, 打不開, 無法開啟, 執行, WebView2, 簽章, Gatekeeper, 執行權限]
---

<Mascot at="top">

還不清楚這個程式在做什麼，可以先看[認識 MaiMotionDemo](/intro/about)。

</Mascot>

MaiMotionDemo 是免安裝的桌面程式。這頁帶你下載對應平台的檔案，並處理第一次開啟可能遇到的狀況。

## 去哪裡下載

到[下載頁](/download)下載最新版本，它會依你的系統把對應的檔案放在最上面。舊版本與每一版的更新內容在 GitHub 的發布頁：[所有版本](https://github.com/silver0416/MaiMotionDemo/releases)。

各平台的檔名如下，`X.Y.Z` 是版本號：

| 平台 | 檔名 | 說明 |
| --- | --- | --- |
| Windows | `MaiMotionDemo-vX.Y.Z-windows-x64.exe` | 單一執行檔，下載後直接開啟 |
| macOS | `MaiMotionDemo-vX.Y.Z-macos-universal.zip` | 解壓縮後開啟 |
| Linux | `MaiMotionDemo-vX.Y.Z-linux-x64.AppImage` | 加上執行權限後開啟 |
| Linux | `MaiMotionDemo-vX.Y.Z-linux-x64.deb` | Debian／Ubuntu 系統可用的套件 |

分析全部在你的電腦上完成，不需要帳號，也不需要另外裝 Node.js 或其他伺服器。

## Windows：下載後直接開

1. 下載 `-windows-x64.exe`。
2. 放到你想放的資料夾，雙擊開啟。不需要安裝。

Windows 版需要 **WebView2 Runtime**。如果你的電腦缺少它，到 [Microsoft WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) 下載安裝後再開一次。

<Callout tone="info" title="放在哪個資料夾有差">程式內更新會把新版下載到程式所在的資料夾，那裡不能寫入時才改放到「下載」資料夾。</Callout>

更新的細節見[更新](/more/update)。

## macOS：第一次要用右鍵打開

1. 下載 `-macos-universal.zip`，解壓縮。
2. 第一次開啟時，在 Finder 對程式**按右鍵，選「打開」**，再確認一次。

macOS 版沒有簽章，所以第一次要用這個方式開啟。

## Linux：AppImage 或 deb

- **AppImage**：下載後先加上執行權限，再開啟。例如在終端機執行 `chmod +x MaiMotionDemo-*.AppImage`，或在檔案管理員的「內容／權限」裡勾選允許執行。
- **deb**：用系統的套件工具安裝。

## macOS 與 Linux 版的狀態

macOS 與 Linux 版由 GitHub Actions 自動建置，**還沒有在實機上完整測試**。另外，程式內的自動更新目前只支援 Windows，其他平台有新版時要自己到[下載頁](/download)下載。遇到問題時，可以在「設定 → 錯誤紀錄」複製紀錄回報（見[設定](/more/settings)）。

## 開起來之後

畫面分成三欄：左邊是譜面紀錄，中間是盤面與播放列，右邊是資訊分頁。各區在做什麼見[介面導覽](/start/interface)，接著[新增一張譜面](/charts/new)試試看。

想自己從原始碼建置，見[從原始碼建置](/more/build)。
