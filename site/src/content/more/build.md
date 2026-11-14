---
title: 從原始碼建置
description: 自己從原始碼執行或建立正式執行檔需要的工具、指令與常見問題。
order: 5
keywords: [編譯, 原始碼, 開發, 自己建置, compile, build, Rust, cargo, Node.js, npm, Tauri, Vite, 開發版]
---

一般使用直接[下載執行檔](/start/install)就好。這頁給想自己從原始碼執行或建置的人。

## 需要準備什麼

- Rust
- Microsoft C++ Build Tools
- WebView2
- Node.js

開發與建置已在 Windows、Rust 1.90、Node.js 24 上驗證。

原始碼在 GitHub：[silver0416/MaiMotionDemo](https://github.com/silver0416/MaiMotionDemo)。

## 開發模式執行

在專案根目錄的 PowerShell 執行：

```powershell
npm.cmd ci
npm.cmd run tauri dev
```

第一次建置會下載並編譯 Rust 的依賴，需要比較久。

## 建立正式執行檔

```powershell
npm.cmd run tauri build -- --no-bundle
```

完成後的執行檔在 `src-tauri/target/release/mai-motion-demo.exe`。正式版內嵌介面，不需要 Node.js 或 Vite 伺服器；Windows 仍需要 WebView2 Runtime。

## 只開介面預覽

單獨執行 `npm.cmd run dev` 只會開啟介面預覽，**沒有分析核心**：不能解析輸入，「生成」也不能用。畫面上會標示「瀏覽器預覽」。要實際分析譜面，請用 `npm.cmd run tauri dev` 或正式版執行檔。

## PowerShell 執行 npm 報路徑錯誤

改用 `npm.cmd`（本頁的指令都已經這樣寫），避開本機 `npm.ps1` 啟動器的問題。

## 開發版一片空白或無法連線

開發版需要 Vite 的 **1420 port**。用 `npm.cmd run tauri dev` 讓兩者同時啟動，或改用正式版執行檔。

## 瀏覽器裡「生成」按鈕不能用

瀏覽器預覽沒有分析核心，這是預期的行為。請開啟 Tauri 桌面程式。
