<script lang="ts">
  import { MODE_LABEL } from '../lib/contract';
  import { formatClock } from '../lib/format';
  import { sampleWithStarts } from '../lib/motion';
  import { playback, RATES } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import { view } from '../state/view.svelte';

  const bounds = $derived(session.bounds);
  const span = $derived(Math.max(bounds.end - bounds.start, 1e-6));
  const time = $derived(playback.time);

  function ratio(value: number): number {
    return ((value - bounds.start) / span) * 100;
  }

  const leftState = $derived.by(() => {
    const solution = session.solution;
    return solution ? sampleWithStarts(solution.leftSegments, session.leftStarts, time) : null;
  });
  const rightState = $derived.by(() => {
    const solution = session.solution;
    return solution ? sampleWithStarts(solution.rightSegments, session.rightStarts, time) : null;
  });

  interface Tick {
    id: string;
    left: number;
    hand: string;
    kind: string;
    label: string;
  }

  const ticks = $derived.by<Tick[]>(() =>
    session.notes.map((note) => {
      const assignments = session.assignmentsByNote.get(note.id) ?? [];
      const hands = [...new Set(assignments.map((item) => item.hand))];
      return {
        id: note.id,
        left: ratio(note.timeSeconds),
        hand: hands.length === 1 ? hands[0] : hands.length > 1 ? 'B' : 'N',
        kind: note.kind,
        label: `${note.id} ${formatClock(note.timeSeconds)}`,
      };
    }),
  );

  const handoverTicks = $derived.by(() =>
    (session.solution?.handovers ?? []).map((handover, index) => ({
      id: `${handover.noteId}-${index}`,
      left: ratio(handover.startSeconds),
      width: Math.max(0.6, ratio(handover.endSeconds) - ratio(handover.startSeconds)),
      seconds: handover.startSeconds,
    })),
  );

  function seekFromEvent(event: MouseEvent) {
    const target = event.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const fraction = (event.clientX - rect.left) / Math.max(rect.width, 1);
    playback.seek(bounds.start + fraction * span);
  }
</script>

<div class="transport">
  <div class="row row-wrap">
    <button class="btn btn--primary" onclick={() => playback.toggle()} disabled={!session.result}>
      {playback.playing ? '暫停' : '播放'}
    </button>
    <button class="btn" onclick={() => playback.reset()} disabled={!session.result}>重置</button>
    <button
      class="btn btn--icon"
      onclick={() => playback.nudge(-0.1)}
      disabled={!session.result}
      aria-label="後退 0.1 秒">−0.1s</button
    >
    <button
      class="btn btn--icon"
      onclick={() => playback.nudge(0.1)}
      disabled={!session.result}
      aria-label="前進 0.1 秒">+0.1s</button
    >

    <span class="clock mono">{formatClock(time)}</span>
    <span class="muted small mono">／ {formatClock(bounds.end)}</span>

    <span class="spacer"></span>

    <span class="small muted">播放倍率</span>
    <div class="row" role="group" aria-label="播放倍率">
      {#each RATES as rate (rate)}
        <button
          class="btn btn--icon"
          class:is-active={playback.rate === rate}
          aria-pressed={playback.rate === rate}
          onclick={() => playback.setRate(rate)}>{rate}×</button
        >
      {/each}
    </div>
  </div>

  <div class="timeline">
    <!-- 標記條：滑鼠點一下即可定位。鍵盤與輔助技術改用下方同功能的時間滑桿，
         因此這裡刻意不再提供第二個可聚焦控制。 -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ticks" onclick={seekFromEvent} aria-hidden="true">
      {#if playback.loopEnabled}
        <div
          class="loop-band"
          style={`left:${ratio(playback.loopStart)}%;width:${Math.max(0.5, ratio(playback.loopEnd) - ratio(playback.loopStart))}%`}
        ></div>
      {/if}
      {#if bounds.start < 0}
        <div class="zero-mark" style={`left:${ratio(0)}%`}></div>
      {/if}
      {#each ticks as tick (tick.id)}
        <div
          class="tick"
          class:tick--left={tick.hand === 'L'}
          class:tick--right={tick.hand === 'R'}
          class:tick--both={tick.hand === 'B'}
          class:tick--slide={tick.kind === 'slide'}
          class:tick--hold={tick.kind === 'hold'}
          style={`left:${tick.left}%`}
          title={tick.label}
        ></div>
      {/each}
      {#each handoverTicks as handover (handover.id)}
        <div
          class="handover-band"
          style={`left:${handover.left}%;width:${handover.width}%`}
          title={`換手 ${formatClock(handover.seconds)}`}
        ></div>
      {/each}
      <div class="playhead" style={`left:${ratio(time)}%`}></div>
    </div>

    <input
      class="seek"
      type="range"
      min={bounds.start}
      max={bounds.end}
      step="0.001"
      value={time}
      disabled={!session.result}
      aria-label="播放時間（秒）"
      oninput={(event) => playback.seek(Number(event.currentTarget.value))}
    />
  </div>

  <div class="row row-wrap small">
    <label class="check">
      <input
        type="checkbox"
        checked={playback.loopEnabled}
        onchange={(event) => playback.setLoop(event.currentTarget.checked)}
      />
      <span>循環片段</span>
    </label>
    <button class="btn btn--icon" onclick={() => playback.setLoopStart(time)}>設為起點</button>
    <button class="btn btn--icon" onclick={() => playback.setLoopEnd(time)}>設為終點</button>
    <span class="muted mono xsmall">
      {formatClock(playback.loopStart)} – {formatClock(playback.loopEnd)}
    </span>
    <button
      class="btn btn--icon"
      onclick={() => {
        playback.setLoopStart(bounds.start);
        playback.setLoopEnd(bounds.end);
      }}>整段</button
    >

    <span class="spacer"></span>

    {#if session.hasHands}
      <span class="hand-state">
        <span class="badge badge--left">L</span>
        <span class="mono">
          {leftState ? (MODE_LABEL[leftState.mode] ?? leftState.mode) : '—'}
          {leftState?.noteId ? ` ${leftState.noteId}` : ''}
        </span>
      </span>
      <span class="hand-state">
        <span class="badge badge--right">R</span>
        <span class="mono">
          {rightState ? (MODE_LABEL[rightState.mode] ?? rightState.mode) : '—'}
          {rightState?.noteId ? ` ${rightState.noteId}` : ''}
        </span>
      </span>
    {:else}
      <span class="muted xsmall">目前沒有可播放的雙手方案</span>
    {/if}
  </div>

  <div class="row row-wrap xsmall muted legend">
    <span><span class="swatch swatch--left"></span>左手 L（實線・圓形）</span>
    <span><span class="swatch swatch--right"></span>右手 R（虛線・方形）</span>
    <span><span class="swatch swatch--accent"></span>換手標記</span>
    <span>空心圓＝Tap，長條＝Hold，星形＋箭頭＝Slide</span>
    {#if view.approach && view.noteMode === 'window'}
      <span>音符飛入為視覺效果，判定時間以 Rust 資料為準</span>
    {/if}
  </div>
</div>

<style>
  .transport {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
  }

  .clock {
    font-size: var(--fs-lg);
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .timeline {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .ticks {
    position: relative;
    height: 22px;
    background: var(--c-control);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
    cursor: pointer;
  }

  .tick {
    position: absolute;
    top: 4px;
    width: 3px;
    height: 8px;
    margin-left: -1px;
    background: var(--c-text-dim);
  }

  .tick--left {
    background: var(--c-left);
  }

  .tick--right {
    background: var(--c-right);
  }

  .tick--both {
    background: var(--c-accent);
  }

  .tick--slide {
    height: 14px;
  }

  .tick--hold {
    height: 11px;
    width: 5px;
  }

  .handover-band {
    position: absolute;
    bottom: 3px;
    height: 5px;
    min-width: 3px;
    background: var(--c-accent);
  }

  .loop-band {
    position: absolute;
    top: 0;
    bottom: 0;
    background: #232a34;
  }

  .zero-mark {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--c-border-strong);
  }

  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--c-text);
  }

  .seek {
    margin: 0;
  }

  .hand-state {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .legend {
    gap: var(--space-3);
  }

  .swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 4px;
    vertical-align: middle;
    border-radius: 2px;
  }

  .swatch--left {
    background: var(--c-left);
  }

  .swatch--right {
    background: var(--c-right);
  }

  .swatch--accent {
    background: var(--c-accent);
  }
</style>
