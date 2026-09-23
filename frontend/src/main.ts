import './styles/app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { startSettingsPersistence } from './state/settings.svelte';
import { annotation } from './state/annotation.svelte';
import { playback } from './state/playback.svelte';
import { records } from './state/records.svelte';
import { session } from './state/session.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('找不到掛載節點 #app');

void startSettingsPersistence();

if (import.meta.env.DEV) {
  // 開發模式專用的測試掛鉤：瀏覽器預覽沒有 Rust 核心，驗證畫面時用它灌入 Rust 實際產生的分析結果。
  // 正式建置時 import.meta.env.DEV 為 false，這段會被移除。
  (window as unknown as Record<string, unknown>).__maimotionDev = { session, records, annotation, playback };
}

export default mount(App, { target });
