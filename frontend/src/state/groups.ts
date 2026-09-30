// 分組標記的操作與提示：播放列按鈕、G 快捷鍵與分組列共用。

import { formatClock } from '../lib/format';
import type { NoteGroup } from '../lib/types';
import { annotation } from './annotation.svelte';
import { playback } from './playback.svelte';
import { session } from './session.svelte';
import { toasts } from './toasts.svelte';

const TOAST = 'note-group';

export function groupRange(group: NoteGroup): string {
  return `${formatClock(group.from)}–${formatClock(group.to)}`;
}

/** 第一次記起點，第二次建立分組；結果用提示告訴使用者。 */
export function markGroupAt(time: number): void {
  if (!annotation.groupsAvailable) {
    toasts.show({ id: TOAST, tone: 'info', title: '無法標分組', body: '先從左側開啟譜面紀錄並產生結果。' });
    return;
  }
  const result = annotation.markGroup(time);
  if (!result) return;
  if (result.kind === 'start') {
    toasts.show({
      id: TOAST,
      tone: 'info',
      title: `分組起點 ${formatClock(annotation.groupStart ?? time)}`,
      body: '移到這組最後一顆，再按一次 G（或「結束分組」）。Esc 取消。',
      sticky: true,
    });
  } else if (result.kind === 'too-short') {
    toasts.show({ id: TOAST, tone: 'warn', title: '一組至少要兩顆音符', body: '終點移到別的音符再按一次，或按 Esc 取消。' });
  } else if (result.kind === 'exists' && result.group) {
    toasts.show({ id: TOAST, tone: 'info', title: `已經有這一組（${groupRange(result.group)}）` });
  } else if (result.group) {
    toasts.show({
      id: TOAST,
      tone: 'ok',
      title: `已標分組 ${groupRange(result.group)}`,
      body: `${annotation.groupNoteCount(result.group)} 顆音符。會跟著標註一起匯出。`,
    });
  }
}

export function cancelGroup(): void {
  annotation.cancelGroup();
  toasts.dismiss(TOAST);
}

/** 以這一組為循環片段（前面留一點助跑）並從頭播放。 */
export function playGroup(group: NoteGroup): void {
  const { start, end } = session.bounds;
  playback.setLoopStart(Math.max(start, group.from - 0.5));
  playback.setLoopEnd(Math.min(end, group.to + 0.3));
  if (!playback.loopEnabled) playback.setLoop(true);
  playback.seek(Math.max(start, group.from - 0.5));
}

export function removeGroup(index: number): void {
  const removed = annotation.removeGroup(index);
  if (!removed) return;
  toasts.show({
    id: TOAST,
    tone: 'info',
    title: `已刪除分組 ${groupRange(removed)}`,
    action: { label: '復原', icon: 'rotate-ccw', run: () => annotation.restoreGroup(removed) },
  });
}
