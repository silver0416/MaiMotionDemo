<script lang="ts">
  import { KIND_LABEL, PART_LABEL, shapeLabel } from '../lib/contract';
  import { noteBadges, noteTarget } from '../lib/notes';
  import { formatClock, formatDelta, formatNumber } from '../lib/format';
  import { playback } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import type { Hand, Note, Solution } from '../lib/types';

  const note = $derived<Note | null>(session.selectedNote);

  function handsOf(solution: Solution, noteId: string): string {
    const list = solution.assignments.filter((item) => item.noteId === noteId);
    if (list.length === 0) return '—';
    const handover = solution.handovers.filter((item) => item.noteId === noteId);
    if (handover.length > 0) {
      return `${handover[0].from}→${handover[handover.length - 1].to}`;
    }
    const hands = [...new Set(list.map((item) => item.hand))] as Hand[];
    return hands.join('/');
  }

  function pick(noteId: string, seek: boolean) {
    session.selectNote(noteId);
    if (!seek) return;
    const target = session.noteById.get(noteId);
    if (target) playback.seek(target.timeSeconds);
  }

  function seekNoteTime(target: Note | null) {
    if (target) playback.seek(target.timeSeconds);
  }

  function seekMotionStart(target: Note | null) {
    if (target && target.motionStart !== null) playback.seek(target.motionStart);
  }

  const badges = $derived(note ? noteBadges(note) : []);
  const assignments = $derived(note ? (session.assignmentsByNote.get(note.id) ?? []) : []);
  const handovers = $derived(note ? (session.handoversByNote.get(note.id) ?? []) : []);
</script>

<div class="stack">
  <section class="section">
    <div class="section-title">
      <span>音符</span>
      {#if note}
        <button class="linkish xsmall" onclick={() => session.selectNote(null)}>取消選取</button>
      {/if}
    </div>

    {#if session.notes.length === 0}
      <p class="small muted">這個結果沒有譜面音符。</p>
    {:else}
      <div class="note-list scroll">
        {#each session.notes as item (item.id)}
          <button
            class="note-row"
            class:is-selected={session.selectedNoteId === item.id}
            onclick={() => pick(item.id, true)}
          >
            <span class="mono">{item.id}</span>
            <span>{KIND_LABEL[item.kind] ?? item.kind}</span>
            <span class="mono">{noteTarget(item)}</span>
            <span class="mono">{formatClock(item.timeSeconds)}</span>
            <span class="mono hand-cell">
              {session.solution ? handsOf(session.solution, item.id) : '—'}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </section>

  {#if note}
    <section class="section">
      <div class="section-title"><span>{note.id} 明細</span></div>
      <dl class="kv">
        <dt>種類</dt>
        <dd>{KIND_LABEL[note.kind] ?? note.kind}</dd>
        <dt>落點</dt>
        <dd>{noteTarget(note)}</dd>
        {#if note.pathId}
          {@const path = session.pathById.get(note.pathId)}
          {#if path}
            <dt>形狀</dt>
            <dd>{shapeLabel(path.shape)} {path.startButton}→{path.endButton}</dd>
          {/if}
        {/if}
        {#if badges.length > 0}
          <dt>修飾</dt>
          <dd>{badges.join('、')}</dd>
        {/if}
        <dt>判定時間</dt>
        <dd>{formatClock(note.timeSeconds)}</dd>
        <dt>結束時間</dt>
        <dd>{formatClock(note.endSeconds)}</dd>
        {#if note.motionStart !== null}
          <dt>移動開始</dt>
          <dd>{formatClock(note.motionStart)}</dd>
        {/if}
        {#if note.motionEnd !== null}
          <dt>移動結束</dt>
          <dd>{formatClock(note.motionEnd)}</dd>
        {/if}
        <dt>盤面座標</dt>
        <dd>{formatNumber(note.position.x, 3)}, {formatNumber(note.position.y, 3)}</dd>
        <dt>原文位置</dt>
        <dd>第 {note.sourceSpan.line} 行 第 {note.sourceSpan.column} 欄</dd>
      </dl>
      <div class="row row-wrap" style="margin-top: var(--space-3)">
        <button class="btn btn--icon" onclick={() => seekNoteTime(note)}>跳到判定時間</button>
        {#if note.motionStart !== null}
          <button class="btn btn--icon" onclick={() => seekMotionStart(note)}>跳到移動開始</button>
        {/if}
      </div>
    </section>

    <section class="section">
      <div class="section-title"><span>目前候選的分配</span></div>
      {#if assignments.length === 0}
        <p class="small muted">這個候選沒有此音符的指派資料。</p>
      {:else}
        <table class="table">
          <thead>
            <tr>
              <th>部分</th>
              <th>手</th>
              <th>區間</th>
            </tr>
          </thead>
          <tbody>
            {#each assignments as assignment, index (index)}
              <tr>
                <td>{PART_LABEL[assignment.part] ?? assignment.part}</td>
                <td>
                  <span
                    class="badge"
                    class:badge--left={assignment.hand === 'L'}
                    class:badge--right={assignment.hand === 'R'}>{assignment.hand}</span
                  >
                </td>
                <td class="mono xsmall">
                  {formatClock(assignment.startSeconds)} – {formatClock(assignment.endSeconds)}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

      {#if handovers.length > 0}
        <p class="small" style="margin-top: var(--space-3)">
          {#each handovers as handover, index (index)}
            {formatClock(handover.startSeconds)} – {formatClock(handover.endSeconds)}
            由 {handover.from} 交給 {handover.to}，重疊期間兩手都在軌道上。
          {/each}
        </p>
      {/if}
    </section>

    {#if session.solutions.length > 1}
      <section class="section">
        <div class="section-title"><span>候選之間的差異</span></div>
        <table class="table">
          <thead>
            <tr>
              <th>候選</th>
              <th>這顆音符</th>
              <th>總成本</th>
            </tr>
          </thead>
          <tbody>
            {#each session.solutions as item, index (item.id)}
              <tr class:is-current={index === session.solutionIndex}>
                <td>
                  <button class="linkish" onclick={() => session.selectSolution(index)}>{item.id}</button>
                </td>
                <td class="mono">{handsOf(item, note.id)}</td>
                <td class="mono xsmall">
                  {formatNumber(item.totalCost)}
                  <span class="muted">
                    {formatDelta(item.totalCost - session.solutions[0].totalCost)}
                  </span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/if}
  {:else}
    <section class="section">
      <p class="small muted">在盤面或清單選一顆音符。</p>
    </section>
  {/if}
</div>

<style>
  .note-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 220px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
    padding: var(--space-1);
  }

  .note-row {
    display: grid;
    grid-template-columns: 46px 52px 54px 1fr 54px;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    font-size: var(--fs-sm);
    text-align: left;
    background: var(--c-panel);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .note-row:hover {
    background: var(--c-control);
  }

  .note-row.is-selected {
    border-color: var(--c-accent);
    background: var(--c-control);
  }

  .hand-cell {
    text-align: right;
    font-weight: 700;
  }

  tr.is-current td {
    background: var(--c-control);
  }
</style>
