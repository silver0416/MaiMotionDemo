// 分組標記的操作與提示：逐顆清單的右鍵選單、G 快捷鍵與備註頁共用。

import type { Note } from '../lib/types';
import { annotation } from './annotation.svelte';
import { playback } from './playback.svelte';
import { session } from './session.svelte';
import { toasts } from './toasts.svelte';

const TOAST = 'note-group';

/** 第一次記下這組的第一顆，第二次以兩顆之間建立分組；結果用提示告訴使用者。 */
export function markGroupAt(note: Note | null): void {
  if (!annotation.groupsAvailable) {
    toasts.show({ id: TOAST, tone: 'info', title: '無法標分組', body: '先從左側開啟譜面紀錄並產生結果。' });
    return;
  }
  if (!note) {
    toasts.show({ id: TOAST, tone: 'info', title: '先選一顆音符', body: '在清單或盤面點選這組的第一顆，再按 G。' });
    return;
  }
  const result = annotation.markGroup(note);
  if (!result) return;
  if (result.kind === 'start') {
    // 清單上方的浮動提示會說明下一步，不另外跳提示。
    toasts.dismiss(TOAST);
  } else if (result.kind === 'too-short') {
    toasts.show({ id: TOAST, tone: 'warn', title: '一組至少要兩顆音符', body: '選別顆當最後一顆，或按 Esc 取消。' });
  } else if (result.kind === 'exists' && result.index !== undefined) {
    toasts.show({ id: TOAST, tone: 'info', title: `已經有這一組（${annotation.groupName(result.index)}）` });
  } else if (result.index !== undefined) {
    const group = annotation.draft.groups[result.index];
    toasts.show({
      id: TOAST,
      tone: 'ok',
      title: `已標分組 ${annotation.groupName(result.index)}`,
      body: `${group ? annotation.groupNotes(group).length : 0} 顆音符。會跟著標註一起匯出。`,
    });
  }
}

export function cancelGroup(): void {
  annotation.cancelGroup();
  toasts.dismiss(TOAST);
}

/** 以這一組為循環片段（前面留一點助跑）並從頭播放。 */
export function playGroup(index: number): void {
  const span = annotation.groupSpan(index);
  if (!span) return;
  const { start, end } = session.bounds;
  playback.setLoopStart(Math.max(start, span.from - 0.5));
  playback.setLoopEnd(Math.min(end, span.to + 0.3));
  if (!playback.loopEnabled) playback.setLoop(true);
  playback.seek(Math.max(start, span.from - 0.5));
}

function undoableToast(title: string, before: ReturnType<typeof annotation.snapshotGroups>): void {
  toasts.show({
    id: TOAST,
    tone: 'info',
    title,
    action: { label: '復原', icon: 'rotate-ccw', run: () => annotation.restoreGroups(before) },
  });
}

export function removeGroup(index: number): void {
  const name = annotation.groupName(index);
  if (!name) return;
  const before = annotation.snapshotGroups();
  annotation.removeGroup(index);
  undoableToast(`已刪除分組「${name}」`, before);
}

export function removeFromGroup(index: number, note: Note): void {
  const name = annotation.groupName(index);
  if (!name) return;
  const before = annotation.snapshotGroups();
  const dissolved = annotation.removeFromGroup(index, note);
  undoableToast(
    dissolved
      ? `已移出第 ${annotation.noteNumber(note)} 顆；「${name}」剩不到兩顆，整組刪除`
      : `已把第 ${annotation.noteNumber(note)} 顆移出「${name}」`,
    before,
  );
}

export function addToGroup(index: number, note: Note): void {
  annotation.addToGroup(index, note);
  toasts.show({
    id: TOAST,
    tone: 'ok',
    title: `已把第 ${annotation.noteNumber(note)} 顆加入「${annotation.groupName(index)}」`,
  });
}
