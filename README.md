# MaiMotionDemo

把 simai 譜面變成可播放的左右手動作動畫，在 maimai 圓盤上呈現 Tap、Hold、Touch、Slide 的打法，並比較不同的雙手分配方案。

[下載 Windows x64 執行檔](https://github.com/silver0416/MaiMotionDemo/releases/download/v0.4.2/MaiMotionDemo-v0.4.2-windows-x64.exe) · [所有版本](https://github.com/silver0416/MaiMotionDemo/releases) · [使用手冊](USER_GUIDE.md)

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
- 三種評分方式：人類動作 V3（預設，兼顧移動、左右分工、連打與折返負荷）、分工優先 V2、舊版比較 V1
- Slide 依實際判定規則推算：可跳區抄近路，一隻手的手掌能同時蓋住相鄰的線條
- 最多比較三種雙手方案，可在「參數」調整打法偏好後重新生成
- 0.25×–2× 播放、拖曳定位、循環片段、點選音符查看分配
- 時間軸可加標籤（按 M 或雙擊時間軸上方），拖曳片段把手設定循環範圍
- 「音符」可一鍵選取目前播放時間最接近的音符
- 譜面紀錄可右鍵查看、編輯或刪除
- 「標註」記錄真人實際的左右手、Slide 換手與備註，可複製分享、匯入合併（「新增」也能直接貼標註檔），並和模型比對
- 「影片同步」開獨立視窗對照 YouTube 手元影片或自己錄的影片：對齊一次後，選音符就自動慢速播放那一段；播放、暫停與速度兩個視窗同步，對齊會照譜面與影片記住
- 每次開啟自動檢查更新，可以直接在程式內下載新版並重新啟動，之後詢問是否刪除舊版
- 「設定」可查看版本與應用程式種類、檢查更新、複製匯入錯誤紀錄回報，以及清除暫存或全部資料

## 注意

分析在本機完成。只有搜尋譜面時會連線 simai Wiki 或 Majdata；使用影片同步時會從 GitHub 下載 yt-dlp（與 Deno），並連線 YouTube 搜尋、下載影片，影片只存在本機。結果是依規則推算的打法建議，不代表官方判定或唯一正確打法；「未找到可行方案」也不表示譜面無法遊玩。
