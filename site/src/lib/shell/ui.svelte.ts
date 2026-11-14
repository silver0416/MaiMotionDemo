/** 外框共用的開關狀態：搜尋對話框可以從頂列、底部分頁列、首頁與 404 頁打開。 */
class Ui {
  searchOpen = $state(false);
  /** 搜尋框的預填文字（例如 404 頁帶入網址裡的字）。 */
  searchSeed = $state('');

  openSearch(seed = '') {
    this.searchSeed = seed;
    this.searchOpen = true;
  }
}

export const ui = new Ui();

/** 快捷鍵顯示：Mac 是 ⌘K，其他是 Ctrl K。伺服器算繪時先當成 Ctrl。 */
export function isMac(): boolean {
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
}
