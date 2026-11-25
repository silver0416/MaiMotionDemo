import { hadPriorData } from './priorData';
import { toasts } from './toasts.svelte';

/**
 * 飛入時間的通知（2026-11-25 使用者指定）：飛入時間改成 0.20～0.40 秒、預設 0.30 秒（約遊戲速度 7.2），
 * Touch 另有自己的進場時間（預設 0.50 秒），更新上來的人也自動換過去（state/settings.svelte.ts 的 VIEW_LINEUP 2）。
 * 舊的預設 0.85 秒已經不在範圍內，沒有開關可以改回去，所以通知上放一顆按鈕，直接帶到「顯示」分頁的這兩個設定。
 *
 * 第一次安裝的人看到的本來就是新的預設，不跳。不論有沒有跳，第一次啟動就記下已處理，之後都不再跳。
 */
const SEEN_KEY = 'maimotion.approach-intro.v1';
const TOAST_ID = 'approach-intro';

export async function showApproachIntroOnce(openSettings: () => void): Promise<void> {
  try {
    if (localStorage.getItem(SEEN_KEY) !== null) return;
    localStorage.setItem(SEEN_KEY, JSON.stringify({ at: Date.now() }));
  } catch {
    // 記不住已處理就不跳，免得每次開啟都跳一次。
    return;
  }
  if (!(await hadPriorData())) return;
  toasts.show({
    id: TOAST_ID,
    tone: 'info',
    title: '飛入時間的預設已更新',
    body: '預設改成約遊戲速度 7，Touch 也多了自己的進場時間；滑桿下標了對應的遊戲速度，可以照平常的設定調整。',
    sticky: true,
    action: {
      label: '調整',
      icon: 'sliders',
      run: () => {
        openSettings();
        toasts.dismiss(TOAST_ID);
      },
    },
  });
}
