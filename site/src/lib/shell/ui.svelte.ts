/**
 * 外框共用的開關狀態：搜尋對話框可以從頂列、首頁、404 頁與 App 外框的底部導覽列打開；
 * 選單抽屜只在一般外框手機寬度的頂列；「目錄」「更多」兩個抽屜只在 App 外框（從主畫面開啟）的底部導覽列。
 * 一次只開一個：打開其中一個時把其他的關掉。
 */
class Ui {
  searchOpen = $state(false);
  /** 搜尋框的預填文字（例如 404 頁帶入網址裡的字）。 */
  searchSeed = $state('');
  /** 一般外框手機寬度的選單抽屜（外觀、下載、GitHub＋章節目錄）。 */
  menuOpen = $state(false);
  /** App 外框的「目錄」抽屜（章節目錄）。 */
  tocOpen = $state(false);
  /** App 外框的「更多」抽屜（外觀、下載桌面版、GitHub、版本）。 */
  moreOpen = $state(false);

  /** 有任何對話框或抽屜開著（頂列這時不收起、安裝提示先藏起來）。 */
  get anyOpen(): boolean {
    return this.searchOpen || this.menuOpen || this.tocOpen || this.moreOpen;
  }

  /** App 外框底部導覽列管的面板（目錄、搜尋、更多）有沒有開著。 */
  get anyPanel(): boolean {
    return this.searchOpen || this.tocOpen || this.moreOpen;
  }

  /** 目前開著的是哪一個面板。 */
  get panel(): 'toc' | 'search' | 'more' | null {
    return this.searchOpen ? 'search' : this.moreOpen ? 'more' : this.tocOpen ? 'toc' : null;
  }

  /**
   * 關掉目錄、搜尋、更多（App 外框底部導覽列的面板）。
   * scrollTop：關掉之後捲回頁面最上面（已經在首頁時再點「首頁」）。要等面板那一筆瀏覽紀錄收掉之後才捲，
   * 不然瀏覽器還原捲動位置會蓋掉它，所以交給 $lib/app/AppPanels.svelte 處理。
   */
  closePanels(scrollTop = false) {
    this.scrollTopAfterClose = scrollTop;
    this.searchOpen = false;
    this.tocOpen = false;
    this.moreOpen = false;
  }

  /** 見 closePanels。 */
  scrollTopAfterClose = false;

  /**
   * 面板關掉的同時要換頁（例如選了搜尋結果，用 goto 換頁）：在關掉之前設 true。
   * App 外框收「面板開著」那一筆紀錄時會跳過，改由換頁取代它（點連結的情況會自動偵測，不必設）。
   */
  navigatingAway = false;

  openSearch(seed = '') {
    this.searchSeed = seed;
    this.menuOpen = false;
    this.tocOpen = false;
    this.moreOpen = false;
    this.searchOpen = true;
  }

  openToc() {
    this.searchOpen = false;
    this.moreOpen = false;
    this.tocOpen = true;
  }

  openMore() {
    this.searchOpen = false;
    this.tocOpen = false;
    this.moreOpen = true;
  }
}

export const ui = new Ui();

/** 快捷鍵顯示：Mac 是 ⌘K，其他是 Ctrl K。伺服器算繪時先當成 Ctrl。 */
export function isMac(): boolean {
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
}
