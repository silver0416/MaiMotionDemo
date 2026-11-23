import { dbCount, STORE_RECORDS } from '../lib/db';

/**
 * 「這次啟動前本機就有資料」＝不是第一次安裝，是從舊版更新上來的。
 * App 沒有記上一次執行的版本，更新後第一次開啟才要跳的通知（精簡模式介紹、盤面預設顯示）都靠這個判斷。
 *
 * localStorage 在模組載入時就看（主視窗掛載前，各元件與各則通知都還沒寫入自己的記號），
 * 所以第一次安裝時一定是空的；資料庫裡有譜面紀錄也算。
 */
const hadLocalStorage = (() => {
  try {
    for (let index = 0; index < localStorage.length; index += 1) {
      if (localStorage.key(index)?.startsWith('maimotion.')) return true;
    }
  } catch {
    // 讀不到 localStorage 就只看資料庫。
  }
  return false;
})();

export async function hadPriorData(): Promise<boolean> {
  return hadLocalStorage || ((await dbCount(STORE_RECORDS)) ?? 0) > 0;
}
