/** 外框共用的開關狀態：搜尋對話框可以從頂列、首頁與 404 頁打開；選單抽屜只在手機寬度的頂列。 */
class Ui {
  searchOpen = $state(false);
  /** 搜尋框的預填文字（例如 404 頁帶入網址裡的字）。 */
  searchSeed = $state('');
  /** 手機寬度的選單抽屜（外觀、下載、GitHub＋章節目錄）。 */
  menuOpen = $state(false);

  openSearch(seed = '') {
    this.searchSeed = seed;
    this.menuOpen = false;
    this.searchOpen = true;
  }
}

export const ui = new Ui();

/** 快捷鍵顯示：Mac 是 ⌘K，其他是 Ctrl K。伺服器算繪時先當成 Ctrl。 */
export function isMac(): boolean {
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
}
