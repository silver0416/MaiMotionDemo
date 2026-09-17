<script lang="ts">
  import Icon from './Icon.svelte';
  import {
    HAND_LABEL,
    KIND_LABEL,
    PART_LABEL,
    isLegacySolution,
    isV2Solution,
    isV3Solution,
    shapeLabel,
  } from '../lib/contract';
  import { noteBadges, noteTarget } from '../lib/notes';
  import { PALM_APPROX_HINT, coveredTargets, palmSeconds } from '../lib/palm';
  import { TOUCH_AREA_PLACE } from '../lib/touch';
  import { formatClock, formatDelta, formatNumber } from '../lib/format';
  import { playback } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import type { Hand, Note, Solution } from '../lib/types';

  const note = $derived<Note | null>(session.selectedNote);
  /** 依第一名的評分版本決定比較欄；V2／V3 沒有總成本，不算與第一名的差值。 */
  const firstSolution = $derived<Solution | null>(session.solutions[0] ?? null);
  const compareHeader = $derived(
    !firstSolution || isLegacySolution(firstSolution)
      ? '總成本'
      : isV3Solution(firstSolution)
        ? '效率／姿態／負荷'
        : '分工／負擔',
  );

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

  function holdSeconds(target: Note): number {
    return Math.max(target.endSeconds - target.timeSeconds, 0);
  }

  /** 清單第二行只講長度與煙火，其他修飾留在明細，短音符仍保持一行。 */
  function rowFlags(item: Note): string[] {
    const out: string[] = [];
    if (item.kind === 'hold' || item.kind === 'touchHold') {
      out.push(`持續 ${formatNumber(holdSeconds(item), 2)} 秒`);
    }
    if (item.modifiers.fireworks) out.push('煙火');
    if (session.palmByNoteId.has(item.id)) out.push('一掌覆蓋');
    return out;
  }

  const badges = $derived(note ? noteBadges(note) : []);
  const assignments = $derived(note ? (session.assignmentsByNote.get(note.id) ?? []) : []);
  const handovers = $derived(note ? (session.handoversByNote.get(note.id) ?? []) : []);
  /** 覆蓋這顆音符的手掌動作，直接取核心結果，不在前端重算覆蓋組。 */
  const palm = $derived(note ? (session.palmByNoteId.get(note.id) ?? null) : null);
</script>

<div class="stack">
  <section class="section">
    <div class="section-title">
      <span>音符</span>
      {#if note}
        <button class="linkish xsmall" onclick={() => session.selectNote(null)}><Icon name="x" size={12} />取消選取</button>
      {/if}
    </div>

    {#if session.notes.length === 0}
      <p class="small muted">這個結果沒有譜面音符。</p>
    {:else}
      <div class="note-list scroll">
        {#each session.notes as item (item.id)}
          {@const flags = rowFlags(item)}
          <button
            class="note-row"
            class:is-selected={session.selectedNoteId === item.id}
            onclick={() => pick(item.id, true)}
          >
            <span class="cell-id mono">{item.id}</span>
            <span class="cell-target mono">{noteTarget(item)}</span>
            <span class="cell-kind">{KIND_LABEL[item.kind] ?? item.kind}</span>
            <span class="cell-time mono">{formatClock(item.timeSeconds)}</span>
            <span class="cell-hand mono">
              {session.solution ? handsOf(session.solution, item.id) : '—'}
            </span>
            {#if flags.length > 0}
              <span class="cell-flags">{flags.join('・')}</span>
            {/if}
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
        {#if note.touchArea}
          <dt>感應區</dt>
          <dd>{TOUCH_AREA_PLACE[note.touchArea] ?? note.touchArea}</dd>
        {/if}
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
        {#if note.kind === 'hold' || note.kind === 'touchHold'}
          <dt>持續時間</dt>
          <dd>{formatNumber(holdSeconds(note), 2)} 秒</dd>
        {/if}
        {#if note.modifiers.fireworks}
          <dt>煙火</dt>
          <dd>判定時間在落點擴散一次</dd>
        {/if}
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
      {#if note.touchArea === 'C'}
        <p class="small muted" style="margin-top: var(--space-2)">
          simai 的 C、C1、C2 在核心合併為同一個 C，共用中央這一個落點。
        </p>
      {/if}
      <div class="row row-wrap" style="margin-top: var(--space-3)">
        <button class="btn btn--icon" onclick={() => seekNoteTime(note)}><Icon name="clock" />跳到判定時間</button>
        {#if note.motionStart !== null}
          <button class="btn btn--icon" onclick={() => seekMotionStart(note)}><Icon name="to-start" />跳到移動開始</button>
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
            {#if handover.swap}
              {formatClock(handover.startSeconds)} 兩手在同一點碰頭，由 {handover.from} 換成 {handover.to}；
              互換不需要移動，因此沒有重疊時間。
            {:else}
              {formatClock(handover.startSeconds)} – {formatClock(handover.endSeconds)}
              由 {handover.from} 交給 {handover.to}，重疊期間兩手都在軌道上。
            {/if}
          {/each}
        </p>
      {/if}
    </section>

    {#if palm}
      <section class="section">
        <div class="section-title"><span>一掌覆蓋</span></div>
        <dl class="kv">
          <dt>手</dt>
          <dd>
            <span
              class="badge"
              class:badge--left={palm.hand === 'L'}
              class:badge--right={palm.hand === 'R'}>{palm.hand}</span
            >
            {HAND_LABEL[palm.hand]}
          </dd>
          <dt>覆蓋落點</dt>
          <dd>{coveredTargets(palm, session.noteById).join('・')}</dd>
          <dt>覆蓋音符</dt>
          <dd>{palm.coveredNoteIds.join('、')}</dd>
          <dt>覆蓋區間</dt>
          <dd>{formatClock(palm.startSeconds)} – {formatClock(palm.endSeconds)}</dd>
          <dt>時長</dt>
          <dd>{formatNumber(palmSeconds(palm), 2)} 秒</dd>
          <dt>掌心</dt>
          <dd>{formatNumber(palm.center.x, 3)}, {formatNumber(palm.center.y, 3)}</dd>
          <dt>半徑</dt>
          <dd>{formatNumber(palm.radius, 2)}</dd>
        </dl>
        <p class="small muted" style="margin-top: var(--space-2)">
          這一掌佔用到其中最晚釋放的時刻，期間該手不接別處。{PALM_APPROX_HINT}
        </p>
        <div class="row row-wrap" style="margin-top: var(--space-3)">
          <button class="btn btn--icon" onclick={() => playback.seek(palm.startSeconds)}>
            <Icon name="to-start" />
            跳到覆蓋開始
          </button>
          <button class="btn btn--icon" onclick={() => playback.seek(palm.endSeconds)}>
            <Icon name="to-end" />
            跳到覆蓋結束
          </button>
        </div>
      </section>
    {/if}

    {#if session.solutions.length > 1}
      <section class="section">
        <div class="section-title"><span>候選之間的差異</span></div>
        <table class="table">
          <thead>
            <tr>
              <th>候選</th>
              <th>這顆音符</th>
              <th>{compareHeader}</th>
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
                  {#if isV3Solution(item)}
                    {formatNumber(item.score.movement, 2)}／{formatNumber(item.score.posture, 2)}／{formatNumber(
                      item.score.fatigue,
                      2,
                    )}
                  {:else if isV2Solution(item)}
                    {formatNumber(item.score.intuition, 2)}／{formatNumber(item.score.strain, 2)}
                  {:else if isLegacySolution(item) && isLegacySolution(session.solutions[0])}
                    {formatNumber(item.totalCost)}
                    <span class="muted">
                      {formatDelta(item.totalCost - session.solutions[0].totalCost)}
                    </span>
                  {/if}
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
    grid-template-columns: 38px 46px minmax(0, 1fr) auto 44px;
    align-items: center;
    gap: 1px var(--space-1);
    padding: var(--space-1) var(--space-2);
    font-size: var(--fs-sm);
    text-align: left;
    background: var(--c-panel);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .cell-id {
    font-size: var(--fs-xs);
    color: var(--c-text-dim);
  }

  .cell-target {
    font-weight: 700;
  }

  .cell-kind,
  .cell-time {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .cell-kind {
    color: var(--c-text-dim);
  }

  /* 只有 Hold／Touch Hold、煙火與一掌覆蓋會多出這一行，一般音符仍是單行。 */
  .cell-flags {
    grid-column: 2 / -1;
    font-size: var(--fs-xs);
    color: var(--c-text-dim);
    line-height: var(--lh-tight);
  }

  .note-row:hover {
    background: var(--c-control);
  }

  .note-row.is-selected {
    border-color: var(--c-accent);
    background: var(--c-control);
  }

  .cell-hand {
    text-align: right;
    font-weight: 700;
  }

  tr.is-current td {
    background: var(--c-control);
  }
</style>
