# MaiMotionDemo

把 simai 譜面變成可播放的左右手動作動畫，在 maimai 圓盤上呈現 Tap、Hold、Touch、Slide 的打法，並比較不同的雙手分配方案。

[下載 Windows x64 執行檔](https://github.com/silver0416/MaiMotionDemo/releases/download/v0.2.3/MaiMotionDemo-v0.2.3-windows-x64.exe) · [所有版本](https://github.com/silver0416/MaiMotionDemo/releases) · [使用手冊](USER_GUIDE.md)

## 使用

1. 下載執行檔後直接開啟，不需安裝。若 Windows 缺少 WebView2，請安裝 [Microsoft WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)。
2. 按左側「新增」貼上 simai 譜面或整份 `maidata.txt`，或按「搜尋譜面」從 simai Wiki、Majdata 匯入。
3. 按「播放」觀看左右手動作；粉紅 **L** 是左手、藍色 **R** 是右手。

範例：

```text
(120){4}1,8,2,7,3,6,4,5,E
```

## 功能

- 支援常用 simai 記法：Tap、Hold、Touch、Touch Hold、Break、EX、煙火與各種 Slide 形狀
- 最多比較三種雙手方案，可在「參數」調整打法偏好後重新生成
- 0.25×–2× 播放、拖曳定位、循環片段、點選音符查看分配
- 譜面紀錄可右鍵查看、編輯或刪除
- 「設定」可查看版本、複製匯入錯誤紀錄回報，以及清除暫存或全部資料

## 注意

分析在本機完成，只有搜尋譜面時會連線 simai Wiki 或 Majdata。結果是依規則推算的打法建議，不代表官方判定或唯一正確打法；「未找到可行方案」也不表示譜面無法遊玩。
