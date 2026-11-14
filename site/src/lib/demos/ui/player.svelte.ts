import { untrack } from 'svelte';
import { reducedMotion } from '$app/lib/press';

/**
 * 示意動畫的腳本播放器（只給 src/lib/demos/ui/ 底下的演示用）。
 *
 * - 腳本是一串步驟：每一步有一行旁白（aria-live 會念出來）、要做的事（run），
 *   可以先讓小游標移到某個元素再「按下去」（target），或讓某個鍵帽亮起（key）。
 * - 進入可視範圍才開始播、離開就暫停，回到畫面再接著播。
 * - prefers-reduced-motion：不自動播，直接跳到最後一步的狀態；按「播放」仍可看一次。
 * - 使用者一動手（takeOver），腳本就停，不搶控制權；之後按「播放」或「重播」從頭再看一次。
 * - 所有時間都是畫面上的示意，不是 App 的播放時鐘。
 */

export interface Step {
  /** 這一步在做什麼（一行旁白） */
  say: string;
  /** 這一步要做的事；只改狀態，不讀 DOM（減少動態時會一口氣全部執行） */
  run?: () => void;
  /** 游標先移到這個元素再按下去 */
  target?: () => Element | null | undefined;
  /** run 在游標出發時就執行（拖曳：把手和游標一起移動） */
  early?: boolean;
  /** 亮起的鍵帽 */
  key?: string;
  /** 這一步停留多久（毫秒），預設 1200 */
  hold?: number;
}

export interface PlayerOptions {
  steps: Step[];
  /** 回到開始前的狀態 */
  reset: () => void;
  /** 還沒開始時的旁白 */
  intro: string;
  /** 示範結束時的旁白 */
  outro: string;
}

const HOLD = 1200;
/** 游標移到目標的時間，和 Cursor.svelte 的轉場一致。 */
const TRAVEL = 600;
const LIT = 520;

export class DemoPlayer {
  playing = $state(false);
  done = $state(false);
  /** 使用者已經自己操作，腳本停下來了 */
  manual = $state(false);
  /** 減少動態時一口氣跑完腳本中；元件裡有延遲的動作要直接做完 */
  skipping = $state(false);
  index = $state(-1);
  say = $state('');
  lit = $state<string | null>(null);
  cursor = $state({ x: 0, y: 0, show: false, press: 0 });
  /** 游標的座標原點（Stage 的內容區） */
  root: HTMLElement | null = null;

  readonly steps: Step[];
  #opts: PlayerOptions;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #litTimer: ReturnType<typeof setTimeout> | undefined;
  #pending: (() => void) | null = null;
  #remaining = 0;
  #due = 0;
  #resume = false;

  constructor(opts: PlayerOptions) {
    this.#opts = opts;
    this.steps = opts.steps;
    this.say = opts.intro;
  }

  get total(): number {
    return this.steps.length;
  }

  play(): void {
    if (this.manual || this.done) {
      this.restart();
      return;
    }
    if (this.playing) return;
    this.playing = true;
    if (this.#pending) this.#arm();
    else this.#advance();
  }

  pause(): void {
    if (!this.playing) return;
    this.playing = false;
    if (this.#timer !== undefined) {
      clearTimeout(this.#timer);
      this.#timer = undefined;
      this.#remaining = Math.max(0, this.#due - performance.now());
    }
  }

  toggle(): void {
    if (this.playing) this.pause();
    else this.play();
  }

  /** 回到開頭重新播一次。 */
  restart(): void {
    this.#clear();
    this.#opts.reset();
    this.index = -1;
    this.done = false;
    this.manual = false;
    this.cursor.show = false;
    this.lit = null;
    this.say = this.#opts.intro;
    this.playing = false;
    this.play();
  }

  /** 使用者動手了：停下腳本，之後交給使用者。 */
  takeOver(): void {
    if (this.manual) return;
    this.#clear();
    this.playing = false;
    this.manual = true;
    this.#resume = false;
    this.cursor.show = false;
  }

  /** 直接做完整個腳本，停在最後一步（減少動態時）。 */
  skipToEnd(): void {
    this.#clear();
    this.#opts.reset();
    this.skipping = true;
    for (const step of this.steps) step.run?.();
    this.skipping = false;
    this.index = this.steps.length - 1;
    this.playing = false;
    this.done = true;
    this.cursor.show = false;
    this.say = this.#opts.outro;
  }

  /** 讓鍵帽亮一下（使用者按鍵時也用）。 */
  flash(key: string | undefined): void {
    if (!key) return;
    clearTimeout(this.#litTimer);
    this.lit = key;
    this.#litTimer = setTimeout(() => (this.lit = null), LIT);
  }

  /** 掛在演示外框上：進入可視範圍才播、離開暫停；減少動態時直接顯示結束狀態。 */
  attach(node: HTMLElement): () => void {
    if (reducedMotion()) {
      // attachment 在 effect 裡執行；直接跑完腳本會讀寫同一批狀態而不斷重跑，所以不追蹤。
      untrack(() => this.skipToEnd());
      return () => this.#clear();
    }
    if (typeof IntersectionObserver === 'undefined') {
      return () => this.#clear();
    }
    // 只要演示有一部分進到畫面中間 60% 的範圍就算看得到；比整個演示高的手機畫面也成立。
    const observer = new IntersectionObserver(
      (records) => {
        const visible = records.some((record) => record.isIntersecting);
        if (visible) {
          if (this.#resume) {
            this.#resume = false;
            this.play();
          } else if (this.index < 0 && !this.manual && !this.done && !this.playing) {
            this.play();
          }
        } else if (this.playing) {
          this.pause();
          this.#resume = true;
        }
      },
      { rootMargin: '-20% 0px -20% 0px' },
    );
    observer.observe(node);
    return () => {
      observer.disconnect();
      this.#clear();
    };
  }

  #advance(): void {
    const next = this.index + 1;
    const step = this.steps[next];
    if (!step) {
      this.#finish();
      return;
    }
    this.index = next;
    this.say = step.say;
    const target = step.target?.();
    if (target) {
      this.#point(target);
      if (step.early) step.run?.();
      const travel = reducedMotion() ? 0 : TRAVEL;
      this.#wait(travel, () => {
        this.cursor.press += 1;
        if (!step.early) step.run?.();
        this.flash(step.key);
        this.#wait(step.hold ?? HOLD, () => this.#advance());
      });
      return;
    }
    this.flash(step.key);
    step.run?.();
    this.#wait(step.hold ?? HOLD, () => this.#advance());
  }

  #finish(): void {
    this.#clear();
    this.playing = false;
    this.done = true;
    this.cursor.show = false;
    this.say = this.#opts.outro;
  }

  #point(target: Element): void {
    if (!this.root) return;
    const base = this.root.getBoundingClientRect();
    const rect = target.getBoundingClientRect();
    this.cursor.x = rect.left + rect.width / 2 - base.left;
    this.cursor.y = rect.top + rect.height / 2 - base.top;
    this.cursor.show = true;
  }

  #wait(ms: number, fn: () => void): void {
    this.#pending = fn;
    this.#remaining = ms;
    if (this.playing) this.#arm();
  }

  #arm(): void {
    clearTimeout(this.#timer);
    this.#due = performance.now() + this.#remaining;
    this.#timer = setTimeout(() => {
      const fn = this.#pending;
      this.#pending = null;
      this.#timer = undefined;
      fn?.();
    }, this.#remaining);
  }

  #clear(): void {
    clearTimeout(this.#timer);
    this.#timer = undefined;
    this.#pending = null;
    this.#remaining = 0;
  }
}

/** 使用者的操作：先讓腳本停下，再做事。 */
export function userAction<A extends unknown[]>(player: DemoPlayer, fn: (...args: A) => void): (...args: A) => void {
  return (...args: A) => {
    player.takeOver();
    fn(...args);
  };
}

/** 只處理演示內、沒有按 Ctrl／Alt／⌘ 的按鍵，且焦點不在文字輸入欄。 */
export function demoKey(event: KeyboardEvent): string | null {
  if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey || event.isComposing) return null;
  const target = event.target as HTMLElement | null;
  if (target?.closest('input:not([type=checkbox]):not([type=radio]):not([type=range]), textarea, select, [contenteditable]')) {
    return null;
  }
  return event.key;
}
