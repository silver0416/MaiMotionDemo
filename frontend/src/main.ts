import './styles/app.css';
import { mount } from 'svelte';
import { isVideoWindow } from './lib/video';

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
