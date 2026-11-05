import { fetchReleaseNotes } from '../lib/api';
import type { ReleaseNote } from '../lib/types';

const SPAN_KEY = 'maimotion.release-notes-span.v1';

/**
 * 「查看更新內容」彈窗：從哪一版更新到哪一版，以及向 GitHub 讀到的發布說明。
 * 內容由 Rust 依版本範圍挑好（from 不含、to 含），這裡只負責開關與載入狀態。
 */
class ReleaseNotesState {
  open = $state(false);
  /** 更新前的版本；null 表示不知道，只列目前這一版。 */
  from = $state<string | null>(null);
  /** 目前的版本。 */
  to = $state('');
  loading = $state(false);
  error = $state<string | null>(null);
  notes = $state<ReleaseNote[]>([]);
  #request = 0;

  /** 開啟彈窗；同一段範圍已經讀過就直接顯示，不再連網。 */
  show(from: string | null, to: string): void {
    const loaded = this.from === from && this.to === to && this.notes.length > 0;
    this.from = from;
    this.to = to;
    this.open = true;
    if (!loaded) void this.load();
  }

  close(): void {
    this.open = false;
  }

  async load(): Promise<void> {
    const request = ++this.#request;
    this.loading = true;
    this.error = null;
    try {
      const notes = await fetchReleaseNotes(this.from);
      if (request === this.#request) this.notes = notes;
    } catch (error) {
      if (request !== this.#request) return;
      this.notes = [];
      this.error = typeof error === 'string' ? error : String(error);
    } finally {
      if (request === this.#request) this.loading = false;
    }
  }

  /** 記下這次是從哪一版更新來的，之後從設定也能看同一段更新內容。 */
  remember(from: string | null, to: string): void {
    try {
      localStorage.setItem(SPAN_KEY, JSON.stringify({ from, to }));
    } catch {
      // 存不到時設定頁只列目前這一版。
    }
  }

  /** 目前這一版是從哪一版更新來的（記得的話）。 */
  rememberedFrom(to: string): string | null {
    try {
      const parsed = JSON.parse(localStorage.getItem(SPAN_KEY) ?? 'null') as unknown;
      if (!parsed || typeof parsed !== 'object') return null;
      const span = parsed as { from?: unknown; to?: unknown };
      return span.to === to && typeof span.from === 'string' ? span.from : null;
    } catch {
      return null;
    }
  }
}

export const releaseNotes = new ReleaseNotesState();
