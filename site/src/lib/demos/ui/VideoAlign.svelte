<script lang="ts">
  import Icon from '$app/components/Icon.svelte';
  import { formatClock, formatDelta } from '$app/lib/format';
  import { bounce } from '$app/lib/press';
  import { segmented } from '$app/lib/segmented';
  import MiniDisc, { buttonPoint, type DiscMark } from './MiniDisc.svelte';
  import { DemoPlayer, userAction } from './player.svelte';
  import Stage from './Stage.svelte';

  /**
   * 示意：影片同步。上面一條是「影片」（不是真影片：一個小盤面加一顆代表手的白點，拍到的那一格鍵位亮起），
   * 下面一條是譜面的音符時間；一開始兩條錯開，按「設為第一顆」後譜面整條移過去對齊。
   * 自動示範：拖到第一顆附近 → 逐格 → 設為第一顆 → ±1 格 → 最後一顆 → 選音符時從判定前 0.4 秒以 0.5× 播到判定後 0.3 秒。
   * 之後可以自己逐格（←／→）、設為第一顆、微調、檢查與選音符。按鈕名稱照 video/VideoApp.svelte。
   * 文字對照 annotate/video.md。
   */

  const FPS = 30;
  const FRAME = 1 / FPS;
  /** 影片裡手真正拍到第一顆的時間（示意）。 */
  const TRUE_OFFSET = 1.2;
  const LENGTH = 5;
  const PRE = 0.4;
  const POST = 0.3;
  const CUE_RATE = 0.5;

  const NOTES = [
    { t: 0, button: 1 },
    { t: 0.5, button: 8 },
    { t: 1, button: 2 },
    { t: 1.5, button: 7 },
    { t: 2, button: 3 },
    { t: 2.5, button: 6 },
    { t: 3, button: 4 },
  ];
  const LAST = NOTES[NOTES.length - 1].t;

  let video = $state(0);
  let playing = $state(false);
  let rate = $state(1);
  let offset = $state<number | null>(null);
  let selected = $state<number | null>(null);
  let cue = $state<{ from: number; to: number; land: number } | null>(null);

  let videoTrack = $state<HTMLElement | null>(null);
  let alignBtn = $state<HTMLElement | null>(null);
  let nextBtn = $state<HTMLElement | null>(null);
  let plusBtn = $state<HTMLElement | null>(null);
  let minusBtn = $state<HTMLElement | null>(null);
  let lastBtn = $state<HTMLElement | null>(null);
  let notesEl = $state<HTMLElement | null>(null);

  const synced = $derived(offset !== null);
  const pct = (t: number) => (t / LENGTH) * 100;
  const snap = (t: number) => Math.round(Math.min(LENGTH, Math.max(0, t)) * FPS) / FPS;
  const chartTime = $derived(offset === null ? null : video - offset);

  // ---- 影片畫面：手照真正的偏移移動，拍到的那一格鍵位亮起 ----
  const hand = $derived.by(() => {
    const c = video - TRUE_OFFSET;
    const center = { x: 0, y: 0 };
    const at = (index: number) => buttonPoint(NOTES[index].button, 0.92);
    if (c <= -0.6) return center;
    if (c <= 0) {
      const k = (c + 0.6) / 0.6;
      const p = at(0);
      return { x: p.x * k, y: p.y * k };
    }
    for (let index = 0; index < NOTES.length - 1; index += 1) {
      const a = NOTES[index];
      const b = NOTES[index + 1];
      if (c <= b.t) {
        // 拍完先停一下再移動，看起來像手。
        const k = Math.max(0, Math.min(1, (c - a.t - 0.1) / (b.t - a.t - 0.15)));
        const p = at(index);
        const q = at(index + 1);
        return { x: p.x + (q.x - p.x) * k, y: p.y + (q.y - p.y) * k };
      }
    }
    return at(NOTES.length - 1);
  });
  const hitIndex = $derived(NOTES.findIndex((note) => Math.abs(video - TRUE_OFFSET - note.t) < FRAME / 2));
  const discMarks = $derived<DiscMark[]>(hitIndex >= 0 ? [{ button: NOTES[hitIndex].button, tone: 'hit' }] : []);

  // ---- 播放（只在示範播放中或使用者接手後才跑） ----

  $effect(() => {
    if (!(playing && (player.playing || player.manual || player.done))) return;
    let last = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      const next = video + ((now - last) / 1000) * rate;
      last = now;
      if (cue && next >= cue.to) {
        video = snap(cue.land);
        playing = false;
        rate = 1;
        return;
      }
      if (next >= LENGTH) {
        video = LENGTH;
        playing = false;
        return;
      }
      video = next;
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  });

  function reset() {
    video = 0;
    playing = false;
    rate = 1;
    offset = null;
    selected = null;
    cue = null;
  }

  function stepFrame(delta: number) {
    playing = false;
    cue = null;
    video = snap(video + delta * FRAME);
  }

  function alignFirst() {
    offset = Math.round(snap(video) * 1000) / 1000;
  }

  function nudge(delta: number) {
    if (offset === null) return;
    offset = Math.round((offset + delta) * 1000) / 1000;
  }

  function showAt(chart: number) {
    if (offset === null) return;
    playing = false;
    cue = null;
    video = snap(chart + offset);
  }

  function selectNote(index: number) {
    selected = index;
    if (offset === null) return;
    const judge = NOTES[index].t + offset;
    cue = { from: judge - PRE, to: judge + POST, land: judge };
    if (player.skipping) {
      video = snap(judge);
      playing = false;
      return;
    }
    video = snap(judge - PRE);
    rate = CUE_RATE;
    playing = true;
  }

  // ---- 使用者操作（player 在下面才建立，按下時才取用） ----

  const act =
    <A extends unknown[]>(fn: (...args: A) => void) =>
    (...args: A) =>
      userAction(player, fn)(...args);

  const describe = () =>
    hitIndex >= 0 ? `這一格手拍到 ${NOTES[hitIndex].button} 號鍵。` : '這一格手沒有拍到鍵。';

  const userPlay = act(() => {
    cue = null;
    if (!playing && video >= LENGTH) video = 0;
    playing = !playing;
    player.say = playing ? '播放影片。' : `暫停在影片 ${formatClock(video)}。${describe()}`;
  });
  const userStep = act((delta: number) => {
    stepFrame(delta);
    player.say = `影片 ${formatClock(video)}：${describe()}`;
  });
  const userJump = act((seconds: number) => {
    playing = false;
    cue = null;
    video = snap(video + seconds);
    player.say = `影片 ${formatClock(video)}。`;
  });
  const userRate = act((value: number) => {
    rate = value;
    player.say = `播放速度 ${value}×。`;
  });
  const userAlign = act(() => {
    alignFirst();
    player.say = `對齊了：偏移 ${formatDelta(offset ?? 0)} 秒。${hitIndex === 0 ? '' : '這一格不是第一顆被拍到的那一格，可以逐格找到再按一次。'}`;
  });
  const userNudge = act((delta: number) => {
    nudge(delta);
    player.say = `偏移 ${formatDelta(offset ?? 0)} 秒。`;
  });
  const userShow = act((chart: number) => {
    showAt(chart);
    player.say = `影片跳到 ${formatClock(video)}：${describe()}`;
  });
  const userNote = act((index: number) => {
    selectNote(index);
    player.say = synced
      ? `選了第 ${index + 1} 顆：從判定前 ${PRE} 秒以 ${CUE_RATE}× 播到判定後 ${POST} 秒，再停回判定那一格。`
      : '還沒對齊：先把影片停在第一顆被拍到的那一格，按「設為第一顆」。';
  });
  const userReplay = act(() => {
    if (selected === null || !synced) {
      player.say = '先對齊並選一顆音符，才能重播這顆。';
      return;
    }
    selectNote(selected);
    player.say = `重播第 ${selected + 1} 顆。`;
  });

  const player: DemoPlayer = new DemoPlayer({
    intro: '影片同步：把影片和譜面對齊一次。',
    outro: '示範結束。換你試試：逐格（←／→）、設為第一顆、微調，或選一顆音符。',
    reset,
    steps: [
      { say: '一開始兩條是錯開的：譜面第一顆在 0 秒，影片裡手要到 1.2 秒左右才拍到。', hold: 2400 },
      {
        say: '先把影片拖到第一顆附近。',
        target: () => videoTrack?.querySelector('[data-anchor="near"]'),
        run: () => (video = snap(1.1)),
        hold: 1300,
      },
      { say: '逐格（→）：手還沒拍到。', target: () => nextBtn, run: () => stepFrame(1), hold: 1000 },
      { say: '再一格：快到了。', target: () => nextBtn, run: () => stepFrame(1), hold: 1000 },
      { say: '這一格手剛好拍到 1 號鍵。', target: () => nextBtn, run: () => stepFrame(1), hold: 1500 },
      {
        say: '按「設為第一顆」：譜面整條移過來對齊，偏移 +1.200 秒。',
        target: () => alignBtn,
        run: alignFirst,
        hold: 2400,
      },
      { say: '微調：判定那一格看起來還沒打到，就按「+1 格」。', target: () => plusBtn, run: () => nudge(FRAME), hold: 1600 },
      { say: '這裡其實已經打到了，按「−1 格」改回來。', target: () => minusBtn, run: () => nudge(-FRAME), hold: 1600 },
      {
        say: '檢查：按「最後一顆」，影片跳到最後一顆也對得上，整首就沒有跑掉。',
        target: () => lastBtn,
        run: () => showAt(LAST),
        hold: 2200,
      },
      {
        say: `在主視窗選一顆音符：影片跳到判定前 ${PRE} 秒，以 ${CUE_RATE}× 播到判定後 ${POST} 秒，再停回判定那一格。`,
        target: () => notesEl?.querySelector('[data-note="2"]'),
        run: () => selectNote(2),
        hold: 2800,
      },
    ],
  });

  function onkey(key: string, event: KeyboardEvent): boolean {
    if (key === 'ArrowRight' || key === 'ArrowLeft') {
      const dir = key === 'ArrowRight' ? 1 : -1;
      if (event.shiftKey) userJump(dir);
      else userStep(dir);
      return true;
    }
    if (key === 'r' || key === 'R') {
      userReplay();
      return true;
    }
    return false;
  }
</script>

<Stage label="影片同步：對齊第一顆" {player} {onkey}>
  <div class="video">
    <div class="top">
      <div class="viewer">
        <MiniDisc label={`影片畫面（示意）：${describe()}`} hand={hand} marks={discMarks} />
        <span class="viewer-tag xsmall muted">影片畫面（示意）</span>
      </div>

      <div class="side">
        <div class="clock mono small">
          <span>影片 {formatClock(video)}</span>
          {#if chartTime === null}
            <span class="unsynced">譜面未對齊，盤面不會跟著動</span>
          {:else}
            <span>譜面 {formatClock(chartTime)}</span>
          {/if}
        </div>
        <div class="control-row">
          <button class="btn btn--primary play" onclick={userPlay} aria-label={playing ? '暫停' : '播放'}>
            <Icon name={playing ? 'pause' : 'play'} size={14} />
          </button>
          <div class="btn-group" role="group" aria-label="逐格">
            <button class="btn-group-item" use:bounce={-1} onclick={() => userStep(-1)} aria-label="上一格" title="上一格（←）">
              <Icon name="chevron-left" size={14} />
            </button>
            <button class="btn-group-item" use:bounce={1} bind:this={nextBtn} onclick={() => userStep(1)} aria-label="下一格" title="下一格（→）">
              <Icon name="chevron-right" size={14} />
            </button>
            <button class="btn-group-item" onclick={userReplay} disabled={!synced || selected === null} aria-label="重播這顆" title="重播這顆（R）">
              <Icon name="rotate-ccw" size={14} />
            </button>
          </div>
          <div class="seg seg--mono seg--fit rates" role="group" aria-label="播放速度" use:segmented>
            {#each [0.5, 1] as value (value)}
              <button class="seg-btn" aria-pressed={rate === value} onclick={() => userRate(value)}>{value}×</button>
            {/each}
          </div>
        </div>

        <div class="align">
          <div class="align-head">
            <span class="title">對齊</span>
            <span class="xsmall mono" class:muted={!synced}>{synced ? `偏移 ${formatDelta(offset ?? 0)} 秒` : '尚未對齊'}</span>
          </div>
          <button class="btn" class:btn--primary={!synced} bind:this={alignBtn} onclick={userAlign}>設為第一顆</button>
          <div class="field">
            <span class="field-label">微調</span>
            <div class="btn-group btn-group--fill mono">
              <button class="btn-group-item" bind:this={minusBtn} onclick={() => userNudge(-FRAME)} disabled={!synced} title="影片往前一格對齊">−1 格</button>
              <button class="btn-group-item" bind:this={plusBtn} onclick={() => userNudge(FRAME)} disabled={!synced} title="影片往後一格對齊">+1 格</button>
              <button class="btn-group-item" onclick={() => userNudge(-0.01)} disabled={!synced}>−10ms</button>
              <button class="btn-group-item" onclick={() => userNudge(0.01)} disabled={!synced}>+10ms</button>
            </div>
          </div>
          <div class="field">
            <span class="field-label">檢查</span>
            <div class="btn-group btn-group--fill">
              <button class="btn-group-item" onclick={() => userShow(0)} disabled={!synced}>第一顆</button>
              <button class="btn-group-item" onclick={() => selected !== null && userShow(NOTES[selected].t)} disabled={!synced || selected === null}>選取的音符</button>
              <button class="btn-group-item" bind:this={lastBtn} onclick={() => userShow(LAST)} disabled={!synced}>最後一顆</button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 兩條軌道共用同一把尺（影片時間 0–5 秒）。 -->
    <div class="tracks">
      <span class="lane-name">影片</span>
      <div class="track" bind:this={videoTrack}>
        {#if cue}
          <span class="cue" style:left={`${pct(cue.from)}%`} style:width={`${pct(cue.to - cue.from)}%`}>
            <span class="cue-label mono">{CUE_RATE}×</span>
          </span>
        {/if}
        {#each NOTES as note, index (index)}
          <span class="hit" class:is-now={hitIndex === index} style:left={`${pct(note.t + TRUE_OFFSET)}%`}></span>
        {/each}
        <span class="anchor" data-anchor="near" style:left={`${pct(1.1)}%`}></span>
        <div class="layer" style:transform={`translateX(${pct(video)}%)`}>
          <span class="playhead"></span>
        </div>
        <input
          class="seek"
          type="range"
          min="0"
          max={LENGTH}
          step={FRAME}
          value={video}
          aria-label="影片位置"
          oninput={act((event: Event) => {
            playing = false;
            cue = null;
            video = snap(Number((event.currentTarget as HTMLInputElement).value));
            player.say = `影片 ${formatClock(video)}：${describe()}`;
          })}
        />
      </div>

      <span class="lane-name">譜面</span>
      <div class="track track--chart">
        <div class="layer layer--chart" style:transform={`translateX(${pct(offset ?? 0)}%)`}>
          {#each NOTES as note, index (index)}
            <span class="note mono" class:is-selected={selected === index} style:left={`${pct(note.t)}%`}>{note.button}</span>
          {/each}
        </div>
      </div>

      <span></span>
      <div class="axis xsmall mono muted" aria-hidden="true">
        {#each [0, 1, 2, 3, 4, 5] as tick (tick)}
          <span style:left={`${pct(tick)}%`}>{tick}s</span>
        {/each}
      </div>
    </div>

    <div class="notes">
      <span class="title">在主視窗選音符</span>
      <div class="note-buttons" role="group" aria-label="選一顆音符" bind:this={notesEl}>
        {#each NOTES as note, index (index)}
          <button
            class="btn note-btn mono"
            data-note={index}
            aria-pressed={selected === index}
            aria-label={`第 ${index + 1} 顆：Tap ${note.button}`}
            onclick={() => userNote(index)}
          >
            {index + 1}
          </button>
        {/each}
      </div>
    </div>
  </div>
</Stage>

<style>
  .video {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .top {
    display: grid;
    grid-template-columns: 10rem minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
  }

  @container demo (width < 520px) {
    .top {
      grid-template-columns: minmax(0, 1fr);
    }

    .viewer {
      width: min(100%, 11rem);
      margin: 0 auto;
    }
  }


  .viewer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .clock {
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--space-3);
    color: var(--c-text-strong);
  }

  .unsynced {
    color: var(--c-text-dim);
    font-family: var(--font-sans);
  }

  .control-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .play {
    width: 32px;
    min-width: 32px;
    padding: 0;
  }

  .align {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--c-border);
  }

  .align-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .title {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .field {
    align-self: stretch;
  }

  /* ---- 兩條軌道 ---- */

  .tracks {
    display: grid;
    grid-template-columns: 2.5rem minmax(0, 1fr);
    align-items: center;
    row-gap: var(--space-1);
  }

  .lane-name {
    color: var(--c-text-dim);
    font-size: var(--fs-xs);
    font-weight: 600;
  }

  .track {
    position: relative;
    height: 28px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-control);
  }

  .track--chart {
    overflow: hidden;
  }

  .layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .layer--chart {
    transition: transform 700ms var(--ease-out);
  }

  .playhead {
    position: absolute;
    top: -3px;
    bottom: -3px;
    left: -1px;
    width: 2px;
    background: var(--c-text-strong);
  }

  /* 影片裡手拍到盤面的那幾格（示意）。 */
  .hit {
    position: absolute;
    top: 50%;
    width: 8px;
    height: 8px;
    margin: -4px 0 0 -4px;
    border: 2px solid var(--c-text-dim);
    border-radius: 50%;
    background: var(--c-panel);
  }

  .hit.is-now {
    border-color: var(--c-accent);
  }

  .note {
    position: absolute;
    top: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    margin: -9px 0 0 -9px;
    border: 2px solid var(--c-text-strong);
    border-radius: 50%;
    background: var(--c-panel);
    color: var(--c-text-strong);
    font-size: var(--fs-xs);
    font-weight: 700;
  }

  .note.is-selected {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  .cue {
    position: absolute;
    top: 2px;
    bottom: 2px;
    border-radius: var(--radius-sm);
    background: var(--c-control-hover);
    border: 1px dashed var(--c-accent);
  }

  .cue-label {
    position: absolute;
    top: -16px;
    left: 0;
    color: var(--c-accent);
    font-size: var(--fs-xs);
    font-weight: 600;
  }

  .anchor {
    position: absolute;
    top: 50%;
    width: 0;
    height: 0;
  }

  .seek {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    appearance: none;
    background: transparent;
    cursor: pointer;
  }

  .seek::-webkit-slider-runnable-track {
    height: 100%;
    background: transparent;
  }

  .seek::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 100%;
    background: transparent;
    border: none;
    box-shadow: none;
  }

  .seek::-moz-range-track {
    background: transparent;
  }

  .seek::-moz-range-thumb {
    width: 14px;
    height: 100%;
    background: transparent;
    border: none;
  }

  .axis {
    position: relative;
    height: 16px;
  }

  .axis span {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
  }

  .axis span:first-child {
    transform: none;
  }

  .axis span:last-child {
    transform: translateX(-100%);
  }

  .notes {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .note-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .note-btn {
    min-width: 32px;
    padding: 0 10px;
  }

  @media (pointer: coarse) {
    .btn-group-item {
      height: 38px;
    }

    .play {
      width: 44px;
      min-width: 44px;
    }

    .note-btn {
      min-width: 44px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .layer--chart {
      transition: none;
    }
  }
</style>
