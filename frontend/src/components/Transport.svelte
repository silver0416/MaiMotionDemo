<script lang="ts">
  import { MODE_LABEL } from '../lib/contract';
  import { formatClock } from '../lib/format';
  import { sampleWithStarts } from '../lib/motion';
  import { playback, RATES } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';

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
      };
    }),
  );

  const handoverTicks = $derived.by(() =>
    (session.solution?.handovers ?? []).map((handover, index) => ({
      id: `${handover.noteId}-${index}`,
      left: ratio(handover.startSeconds),
      width: Math.max(0.6, ratio(handover.endSeconds) - ratio(handover.startSeconds)),
    })),
  );

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
    <span class="muted small mono total">／ {formatClock(bounds.end)}</span>

    <span class="spacer"></span>

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

  <!-- 標記層與時間滑桿共用同一條座標軸：標記層左右各內縮半個滑鈕寬度，
       正好等於滑鈕中心的移動範圍，因此大頭針與上方標記線永遠對齊。 -->
  <div class="timeline">
    <div class="ticks" aria-hidden="true"></div>
    <div class="marks" aria-hidden="true">
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
            class:tick--hold={tick.kind === 'hold' || tick.kind === 'touchHold'}
            class:tick--touch={tick.kind === 'touch' || tick.kind === 'touchHold'}
            style={`left:${tick.left}%`}
          ></div>
        {/each}
        {#each handoverTicks as handover (handover.id)}
          <div
            class="handover-band"
            style={`left:${handover.left}%;width:${handover.width}%`}
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
        <span class="mono state-text">
          {leftState ? (MODE_LABEL[leftState.mode] ?? leftState.mode) : '—'}
          {leftState?.noteId ? ` ${leftState.noteId}` : ''}
        </span>
      </span>
      <span class="hand-state">
        <span class="badge badge--right">R</span>
        <span class="mono state-text">
          {rightState ? (MODE_LABEL[rightState.mode] ?? rightState.mode) : '—'}
          {rightState?.noteId ? ` ${rightState.noteId}` : ''}
        </span>
      </span>
    {:else}
      <span class="muted xsmall hands-empty">沒有可播放的方案</span>
    {/if}
  </div>

  <div class="row row-wrap xsmall muted legend">
    <span><span class="swatch swatch--left"></span>左手 L・實線圓形</span>
    <span><span class="swatch swatch--right"></span>右手 R・虛線方形</span>
    <span><span class="swatch swatch--accent"></span>換手</span>
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

  /* 時間與手部狀態都保留固定寬度：內容變動時播放列的換行數不變，
     盤面高度因此穩定，不會在切換範例或播放時被重新縮放。 */
  .clock {
    min-width: 9ch;
    font-size: var(--fs-lg);
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .total {
    min-width: 9ch;
  }

  .timeline {
    --thumb: 14px;
    position: relative;
    height: 26px;
  }

  .ticks {
    position: absolute;
    inset: 0;
    background: var(--c-control);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    overflow: hidden;
    pointer-events: none;
  }

  .marks {
    position: absolute;
    top: 1px;
    bottom: 1px;
    pointer-events: none;
    left: calc(var(--thumb) / 2);
    right: calc(var(--thumb) / 2);
  }

  .tick {
    position: absolute;
    top: 5px;
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
    height: 12px;
    width: 5px;
  }

  .tick--touch {
    top: 9px;
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

  /* 滑桿疊在標記層上，軌道透明，只留下大頭針。 */
  .seek {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    appearance: none;
    -webkit-appearance: none;
    background: transparent;
  }

  .seek::-webkit-slider-runnable-track {
    height: 100%;
    background: transparent;
  }

  .seek::-webkit-slider-thumb {
    appearance: none;
    -webkit-appearance: none;
    width: var(--thumb);
    height: var(--thumb);
    border: 2px solid var(--c-bg);
    border-radius: 50%;
    background: var(--c-text);
  }

  .seek:disabled::-webkit-slider-thumb {
    background: var(--c-border-strong);
  }

  .seek::-moz-range-track {
    height: 100%;
    background: transparent;
  }

  .seek::-moz-range-thumb {
    width: var(--thumb);
    height: var(--thumb);
    border: 2px solid var(--c-bg);
    border-radius: 50%;
    background: var(--c-text);
  }

  .hand-state {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .state-text {
    display: inline-block;
    min-width: 8ch;
  }

  .hands-empty {
    display: inline-block;
    min-width: 21ch;
    text-align: right;
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
