import { hadPriorData } from './priorData';
import { toasts } from './toasts.svelte';
import { view } from './view.svelte';

/**
 * 盤面預設顯示的通知（2026-11-23 使用者指定）：預設改成「照遊戲上色」打開、「打擊效果」打開、「顯示盤面格線」關掉，
 * 更新上來的人也自動換過去（state/settings.svelte.ts 的 VIEW_LINEUP）。
 * 更新後第一次開啟時，右下角多跳一則通知，上面直接有這三個開關，不喜歡可以當場改回去。
 *
 * 第一次安裝的人看到的本來就是新的預設，不跳。不論有沒有跳，第一次啟動就記下已處理，之後都不再跳。
 * 日後預設值再換代、又要通知時，換一個新的 SEEN_KEY（v2…）。
 */
const SEEN_KEY = 'maimotion.disc-defaults-intro.v1';

export async function showDiscDefaultsIntroOnce(): Promise<void> {
  try {
    if (localStorage.getItem(SEEN_KEY) !== null) return;
    localStorage.setItem(SEEN_KEY, JSON.stringify({ at: Date.now() }));
  } catch {
    // 記不住已處理就不跳，免得每次開啟都跳一次。
    return;
  }
  if (!(await hadPriorData())) return;
  toasts.show({
    id: 'disc-defaults-intro',
    tone: 'info',
    title: '盤面的預設顯示已更新',
    body: '音符改用遊戲的配色，判定時會放出光暈，盤面格線先收起來。之後也可以在「顯示」分頁調整。',
    sticky: true,
    toggles: [
      { label: '照遊戲上色', checked: () => view.noteColors, set: (on) => (view.noteColors = on) },
      { label: '打擊效果', checked: () => view.hitEffects, set: (on) => (view.hitEffects = on) },
      { label: '顯示盤面格線', checked: () => view.showBoardGrid, set: (on) => (view.showBoardGrid = on) },
    ],
  });
}
