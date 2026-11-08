import { dbCount, STORE_RECORDS } from '../lib/db';
import { theme } from '../lib/theme.svelte';
import { toasts } from './toasts.svelte';

/**
 * 精簡模式介紹（2026-11-09 使用者指定）：v1.2.0 起預設精簡模式（說明文字收起來），
 * 從 v1.1.4 以前更新上來的人第一次開啟時，右下角跳一則通知介紹精簡模式，上面有「詳細模式」開關。
 *
 * 舊版沒有記版本，所以用「這次啟動前就有本機資料」判斷是更新上來的：
 * 模組載入時（主視窗掛載前，各元件還沒寫入）已有任何 maimotion.* 的 localStorage，或資料庫裡有譜面紀錄。
 * 不論有沒有跳，第一次啟動就記下已處理；之後都不再跳（v1.2.0 起新裝的人日後更新也不會跳）。
 */
const SEEN_KEY = 'maimotion.concise-intro.v1';

const hadLocalData = (() => {
  try {
    for (let index = 0; index < localStorage.length; index += 1) {
      const key = localStorage.key(index);
      if (key?.startsWith('maimotion.') && key !== SEEN_KEY) return true;
    }
  } catch {
    // 讀不到 localStorage 就只看資料庫。
  }
  return false;
})();

export async function showConciseIntroOnce(): Promise<void> {
  try {
    if (localStorage.getItem(SEEN_KEY) !== null) return;
    localStorage.setItem(SEEN_KEY, JSON.stringify({ at: Date.now() }));
  } catch {
    // 記不住已處理就不跳，免得每次開啟都跳一次。
    return;
  }
  const upgraded = hadLocalData || ((await dbCount(STORE_RECORDS)) ?? 0) > 0;
  if (!upgraded) return;
  toasts.show({
    id: 'concise-intro',
    tone: 'info',
    title: '介面已切換至精簡模式',
    body: '操作說明、欄位提示和圖例先收起來，畫面只留標題、數字與狀態。需要說明時打開詳細模式；之後也可以在「設定 → 外觀」切換。',
    keepBody: true,
    sticky: true,
    toggle: { label: '詳細模式', checked: () => theme.help, set: (on) => theme.setHelp(on) },
  });
}
