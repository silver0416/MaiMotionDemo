import './styles/fonts.css';
import './styles/app.css';
import { mount } from 'svelte';
import { isVideoWindow } from './lib/video';
import { theme } from './lib/theme.svelte';

// 外觀在掛載前套上，畫面不會先閃一次預設配色。主視窗與影片視窗共用。
theme.start();

// 停用瀏覽器內建的右鍵選單（使用者要求）。有自訂選單的地方（譜面紀錄、資料夾／標籤、時間軸標籤、
// 標註清單、影片清單）各自處理 contextmenu 再開自己的選單，這裡只擋掉預設行為，不影響它們。
// 開發模式按住 Shift 右鍵仍可叫出內建選單（檢查元素用）。
window.addEventListener('contextmenu', (event) => {
  if (import.meta.env.DEV && event.shiftKey) return;
  event.preventDefault();
});

const target = document.getElementById('app');
if (!target) throw new Error('找不到掛載節點 #app');

// 影片同步視窗與主視窗共用同一份網頁，依視窗決定要掛哪個畫面。
if (isVideoWindow()) {
  const { default: VideoApp } = await import('./video/VideoApp.svelte');
  mount(VideoApp, { target });
} else {
  const [{ default: App }, { startSettingsPersistence }] = await Promise.all([
    import('./App.svelte'),
    import('./state/settings.svelte'),
  ]);
  void startSettingsPersistence();

  if (import.meta.env.DEV) {
    // 開發模式專用的測試掛鉤：瀏覽器預覽沒有 Rust 核心，驗證畫面時用它灌入 Rust 實際產生的分析結果。
    // 正式建置時 import.meta.env.DEV 為 false，這段會被移除。
    const [{ annotation }, { playback }, { records }, { session }] = await Promise.all([
      import('./state/annotation.svelte'),
      import('./state/playback.svelte'),
      import('./state/records.svelte'),
      import('./state/session.svelte'),
    ]);
    (window as unknown as Record<string, unknown>).__maimotionDev = { session, records, annotation, playback };
  }

  mount(App, { target });
}
