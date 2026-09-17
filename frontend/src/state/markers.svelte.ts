import { cleanMarkers, records, type ChartRecord, type TimelineMarker } from './records.svelte';
import { session } from './session.svelte';
import { toasts } from './toasts.svelte';

const LABEL_MAX = 40;

/**
 * 時間軸標籤：存在目前開啟的譜面紀錄上，換參數重新生成、編輯名稱都會保留。
 * 盤面上的結果必須就是這筆紀錄的原文，才允許新增或修改，避免把標籤掛到別張譜。
 */
export class TimelineMarkers {
  record = $derived.by<ChartRecord | null>(() => {
    const active = records.active;
    const result = session.result;
    return active && result && active.source === result.source ? active : null;
  });

  items = $derived<TimelineMarker[]>(cleanMarkers(this.record?.markers));

  /** 正在改名的標籤；新增後立刻進入改名，Enter 完成。 */
  editingId = $state<string | null>(null);

  get available(): boolean {
    return this.record !== null;
  }

  #write(next: TimelineMarker[]): void {
    const record = this.record;
    if (record) records.setMarkers(record.id, next);
  }

  #nextLabel(): string {
    const used = new Set(this.items.map((item) => item.label));
    let index = this.items.length + 1;
    while (used.has(`標籤 ${index}`)) index += 1;
    return `標籤 ${index}`;
  }

  /** 在指定時間新增標籤並進入改名；同一時間（±0.01 秒）已有標籤就直接改那一個。 */
  add(time: number): TimelineMarker | null {
    if (!this.record) return null;
    const existing = this.items.find((item) => Math.abs(item.time - time) < 0.01);
    if (existing) {
      this.editingId = existing.id;
      return existing;
    }
    const marker: TimelineMarker = {
      id: `mk-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`,
      time,
      label: this.#nextLabel(),
    };
    this.#write([...this.items, marker]);
    this.editingId = marker.id;
    return marker;
  }

  rename(id: string, label: string): void {
    const text = label.trim().slice(0, LABEL_MAX);
    const target = this.items.find((item) => item.id === id);
    if (!target || !text || text === target.label) return;
    this.#write(this.items.map((item) => (item.id === id ? { ...item, label: text } : item)));
  }

  move(id: string, time: number): void {
    this.#write(this.items.map((item) => (item.id === id ? { ...item, time } : item)));
  }

  /** 刪除並提供復原。 */
  remove(id: string): void {
    const target = this.items.find((item) => item.id === id);
    if (!target) return;
    const recordId = this.record?.id;
    if (this.editingId === id) this.editingId = null;
    this.#write(this.items.filter((item) => item.id !== id));
    toasts.show({
      id: 'marker-removed',
      tone: 'info',
      title: `已刪除標籤「${target.label}」`,
      action: {
        label: '復原',
        icon: 'rotate-ccw',
        run: () => {
          toasts.dismiss('marker-removed');
          const record = records.get(recordId ?? null);
          if (!record) return;
          const current = cleanMarkers(record.markers);
          if (current.some((item) => item.id === target.id)) return;
          records.setMarkers(record.id, [...current, target]);
        },
      },
    });
  }

  /** 播放時間之後（不含目前位置）的第一個標籤。 */
  next(time: number): TimelineMarker | null {
    return this.items.find((item) => item.time > time + 0.005) ?? null;
  }

  previous(time: number): TimelineMarker | null {
    return [...this.items].reverse().find((item) => item.time < time - 0.005) ?? null;
  }
}

export const markers = new TimelineMarkers();
