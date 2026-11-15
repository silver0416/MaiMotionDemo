import { ui } from './ui.svelte';

/**
 * 頂列「往下捲收起、往上捲出現」的共用邏輯（一般外框的手機頂列與 App 外框的頂列共用）：
 * - 捲動位置超過一個頂列高、且這次往下累積超過 TOGGLE_PX 時收起；往上累積超過 TOGGLE_PX 出現；在頂端一定顯示。
 * - 對話框或抽屜開著、鍵盤焦點在頂列裡、換頁之後一定顯示。
 * - 捲動監聽是 passive，用 requestAnimationFrame 節流。
 * `enabled()` 回傳 false 時（例如一般外框的電腦寬度）只更新 scrolled、永遠顯示。
 */

/** 這次捲動方向累積超過這個距離才收起／出現，手指輕微晃動不會讓頂列閃來閃去。 */
const TOGGLE_PX = 8;

export class AutoHide {
  /** 頁面是否已經往下捲：在頂端時頂列和背景同色，捲動後才浮起來。 */
  scrolled = $state(false);
  hidden = $state(false);
  header: HTMLElement | null = null;

  #enabled: () => boolean;
  #lastY = 0;
  /** 同方向累積的捲動距離：往下為正、往上為負，換方向時歸零重算。 */
  #travel = 0;
  #frame = 0;

  constructor(enabled: () => boolean) {
    this.#enabled = enabled;
  }

  /** 捲動位置還在頂端這個距離以內（約一個頂列高）一定顯示。 */
  #reveal(): number {
    return this.header?.offsetHeight ?? 56;
  }

  /** 鍵盤焦點在頂列裡時不收起（看不見的東西不能拿著焦點）。 */
  #holdsKeyboardFocus(): boolean {
    return !!this.header?.querySelector(':focus-visible');
  }

  show = () => {
    this.hidden = false;
    this.#travel = 0;
  };

  /**
   * 焦點進到頂列：立刻出現（不等轉場）。
   * 另外，用 Tab 聚焦時瀏覽器會把元素捲到 html 的 scroll-padding-top（含頂列高度）以下；
   * 頂列是 sticky，永遠在那條線上方，瀏覽器就會把整頁往上捲一大段（而且是在 focusin 之前就捲了）。
   * 鍵盤焦點進來時，在下一次繪製前捲回原位：原位是上一次捲動處理記下的 lastY（這次跳動的捲動事件還沒處理）。
   */
  onFocusIn = () => {
    const header = this.header;
    if (this.hidden && header) {
      header.style.transition = 'none';
      header.classList.remove('is-hidden');
      void header.offsetHeight;
      header.style.transition = '';
    }
    this.show();
    if (!header?.querySelector(':focus-visible')) return;
    const before = this.#lastY;
    const href = location.href;
    requestAnimationFrame(() => {
      if (location.href === href && Math.abs(window.scrollY - before) > 1) {
        window.scrollTo({ top: before, behavior: 'instant' });
        this.#lastY = before;
      }
    });
  };

  #update = () => {
    this.#frame = 0;
    const root = document.documentElement;
    // iOS 回彈時 scrollY 會超出範圍：夾在 0～最底，回彈不算一次捲動。
    const y = Math.min(Math.max(window.scrollY, 0), Math.max(root.scrollHeight - window.innerHeight, 0));
    const delta = y - this.#lastY;
    this.#lastY = y;
    this.scrolled = y > 4;

    if (!this.#enabled() || y <= this.#reveal()) {
      this.show();
      return;
    }
    if (delta === 0) return;
    this.#travel = Math.sign(delta) === Math.sign(this.#travel) ? this.#travel + delta : delta;
    if (!this.hidden && this.#travel > TOGGLE_PX) {
      if (ui.anyOpen || this.#holdsKeyboardFocus()) return;
      this.hidden = true;
      this.#travel = 0;
    } else if (this.hidden && this.#travel < -TOGGLE_PX) {
      this.show();
    }
  };

  schedule = () => {
    if (!this.#frame) this.#frame = requestAnimationFrame(this.#update);
  };

  /** 掛載後呼叫：開始監聽捲動，回傳清除函式。 */
  start(): () => void {
    this.#lastY = Math.max(window.scrollY, 0);
    this.#update();
    window.addEventListener('scroll', this.schedule, { passive: true });
    return () => {
      cancelAnimationFrame(this.#frame);
      this.#frame = 0;
      window.removeEventListener('scroll', this.schedule);
    };
  }

  /** 換頁之後一定顯示；從現在的位置重新起算，跳到錨點的那一下不算「往下捲」。 */
  afterNavigate() {
    this.#lastY = Math.max(window.scrollY, 0);
    this.show();
  }
}
