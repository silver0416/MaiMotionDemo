export type ToastTone = 'busy' | 'ok' | 'warn' | 'error' | 'info';

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: string;
  tone: ToastTone;
  title: string;
  body?: string;
  /** sticky 的通知要使用者自己關；其餘幾秒後自動消失。 */
  sticky?: boolean;
  action?: ToastAction;
}

const AUTO_DISMISS_MS = 4000;

/** 右下角的狀態通知。同一個 id 再次 show 會原地取代內容，不會疊出第二則。 */
export class Toasts {
  items = $state<Toast[]>([]);

  #timers = new Map<string, ReturnType<typeof setTimeout>>();

  show(toast: Toast): void {
    this.#clearTimer(toast.id);
    const index = this.items.findIndex((item) => item.id === toast.id);
    if (index >= 0) this.items[index] = toast;
    else this.items.push(toast);
    if (!toast.sticky && toast.tone !== 'busy') {
      this.#timers.set(
        toast.id,
        setTimeout(() => this.dismiss(toast.id), AUTO_DISMISS_MS),
      );
    }
  }

  dismiss(id: string): void {
    this.#clearTimer(id);
    this.items = this.items.filter((item) => item.id !== id);
  }

  #clearTimer(id: string): void {
    const timer = this.#timers.get(id);
    if (timer !== undefined) clearTimeout(timer);
    this.#timers.delete(id);
  }
}

export const toasts = new Toasts();
