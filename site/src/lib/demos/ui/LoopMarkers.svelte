<script lang="ts">
  import Icon from '$app/components/Icon.svelte';
  import { formatClock } from '$app/lib/format';
  import { bounce } from '$app/lib/press';
  import Keycap from './Keycap.svelte';
  import { DemoPlayer, userAction } from './player.svelte';
  import Stage from './Stage.svelte';

  /**
   * 示意：播放列的循環片段與時間軸標籤（樣子照 App 的 Transport.svelte，縮成一段 8 秒的假譜面）。
   * 自動示範：播放 → 設為起點（I）→ 設為終點（O）→ 拖曳把手 → 暫停 → M 加標籤 → 雙擊加標籤 → ]／[ 跳標籤。
   * 之後可以自己按播放、拖時間軸與把手、雙擊時間軸上方，或在演示裡按 I O L M [ ]。
   * 時間軸上的刻度只是位置示意，不是分析結果。文字對照 play/transport.md。
   */

  interface Marker {
    id: number;
    time: number;
    label: string;
  }

  const DURATION = 8;
  const TICKS = Array.from({ length: 15 }, (_, index) => 0.5 + index * 0.5);

  let time = $state(0);
  let playing = $state(false);
  let loopOn = $state(false);
  let loopStart = $state(0);
  let loopEnd = $state(DURATION);
  let markers = $state<Marker[]>([]);
  let nextId = 1;
  let drag = $state<'start' | 'end' | null>(null);

  let playBtn = $state<HTMLElement | null>(null);
  let startBtn = $state<HTMLElement | null>(null);
  let endBtn = $state<HTMLElement | null>(null);
  let rangeLane = $state<HTMLElement | null>(null);
  let markerLane = $state<HTMLElement | null>(null);

  const pct = (t: number) => (t / DURATION) * 100;
  const rangeIsFull = $derived(loopStart <= 0 && loopEnd >= DURATION);
  const dim = $derived(loopOn && !rangeIsFull);
  const sortedMarkers = $derived([...markers].sort((a, b) => a.time - b.time));
  const round = (t: number) => Math.round(Math.min(DURATION, Math.max(0, t)) * 1000) / 1000;

  function reset() {
    time = 0;
    playing = false;
    loopOn = false;
    loopStart = 0;
    loopEnd = DURATION;
    markers = [];
    nextId = 1;
  }

  // ---- 播放：只在示範播放中或使用者接手後才跑，暫停示範時播放頭也停住 ----

  $effect(() => {
    if (!(playing && (player.playing || player.manual || player.done))) return;
    let last = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      const next = time + (now - last) / 1000;
      last = now;
      if (loopOn && next >= loopEnd) time = loopStart;
      else if (next >= DURATION) {
        time = DURATION;
        playing = false;
        return;
      } else time = next;
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  });

  function togglePlay() {
    if (!playing && time >= DURATION) time = 0;
    if (!playing && loopOn && (time < loopStart || time >= loopEnd)) time = loopStart;
    playing = !playing;
  }

  function setStart(at = time) {
    loopStart = round(Math.min(at, loopEnd - 0.1));
    loopOn = true;
  }

  function setEnd(at = time) {
    loopEnd = round(Math.max(at, loopStart + 0.1));
    loopOn = true;
  }

  function clearRange() {
    loopStart = 0;
    loopEnd = DURATION;
    loopOn = false;
  }

  function addMarker(at = time) {
    const used = new Set(markers.map((item) => item.label));
    let index = 1;
    while (used.has(`標籤 ${index}`)) index += 1;
    markers.push({ id: nextId++, time: round(at), label: `標籤 ${index}` });
  }

  function jump(direction: 1 | -1): Marker | null {
    const list = sortedMarkers;
    const target =
      direction > 0 ? list.find((item) => item.time > time + 0.001) : [...list].reverse().find((item) => item.time < time - 0.001);
    if (target) time = target.time;
    return target ?? null;
  }

  // ---- 使用者操作 ----

  // player 在下面才建立，所以在按下時才取用它。
  const act =
    <A extends unknown[]>(fn: (...args: A) => void) =>
    (...args: A) =>
      userAction(player, fn)(...args);

  const userPlay = act(() => {
    togglePlay();
    player.say = playing ? '播放中。' : `暫停在 ${formatClock(time)}。`;
  });
  const userReset = act(() => {
    time = loopOn ? loopStart : 0;
    player.say = '回到開頭。';
  });
  const userNudge = act((delta: number) => {
    time = round(time + delta);
    player.say = `${formatClock(time)}`;
  });
  const userStart = act(() => {
    player.flash('I');
    setStart();
    player.say = `片段起點設在 ${formatClock(loopStart)}，循環打開。`;
  });
  const userEnd = act(() => {
    player.flash('O');
    setEnd();
    player.say = `片段終點設在 ${formatClock(loopEnd)}，循環打開。`;
  });
  const userLoop = act((on: boolean) => {
    player.flash('L');
    loopOn = on;
    player.say = on ? '循環片段打開。' : '循環片段關閉。';
  });
  const userClear = act(() => {
    clearRange();
    player.say = '片段回到整段，循環關閉。';
  });
  const userMarker = act((at?: number) => {
    if (at === undefined) player.flash('M');
    addMarker(at);
    player.say = `在 ${formatClock(round(at ?? time))} 加了一個標籤。`;
  });
  const userJump = act((direction: 1 | -1) => {
    player.flash(direction > 0 ? ']' : '[');
    const target = jump(direction);
    player.say = target ? `跳到「${target.label}」（${formatClock(target.time)}）。` : direction > 0 ? '後面沒有標籤了。' : '前面沒有標籤了。';
  });
  const userSeek = act((value: number) => {
    time = round(value);
  });
  const userGoMarker = act((item: Marker) => {
    time = item.time;
    player.say = `跳到「${item.label}」。`;
  });

  function timeAt(lane: HTMLElement, clientX: number): number {
    const rect = lane.getBoundingClientRect();
    return round(((clientX - rect.left) / rect.width) * DURATION);
  }

  function onLaneDblClick(event: MouseEvent) {
    if (!markerLane || (event.target as HTMLElement).closest('.marker')) return;
    userMarker(timeAt(markerLane, event.clientX));
  }

  function onHandleDown(which: 'start' | 'end', event: PointerEvent) {
    player.takeOver();
    drag = which;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onHandleMove(event: PointerEvent) {
    if (!drag || !rangeLane) return;
    const at = timeAt(rangeLane, event.clientX);
    if (drag === 'start') loopStart = round(Math.min(at, loopEnd - 0.1));
    else loopEnd = round(Math.max(at, loopStart + 0.1));
  }

  function onHandleUp() {
    if (!drag) return;
    player.say = `片段 ${formatClock(loopStart)} – ${formatClock(loopEnd)}。`;
    drag = null;
  }

  function onHandleKey(which: 'start' | 'end', event: KeyboardEvent) {
    const step = event.shiftKey ? 1 : 0.1;
    let value = which === 'start' ? loopStart : loopEnd;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') value -= step;
    else if (event.key === 'ArrowRight' || event.key === 'ArrowUp') value += step;
    else if (event.key === 'Home') value = 0;
    else if (event.key === 'End') value = DURATION;
    else return;
    event.preventDefault();
    event.stopPropagation();
    player.takeOver();
    if (which === 'start') loopStart = round(Math.min(value, loopEnd - 0.1));
    else loopEnd = round(Math.max(value, loopStart + 0.1));
    player.say = `片段${which === 'start' ? '起點' : '終點'} ${formatClock(which === 'start' ? loopStart : loopEnd)}。`;
  }

  function onkey(key: string): boolean {
    switch (key.toLowerCase()) {
      case 'i':
        userStart();
        return true;
      case 'o':
        userEnd();
        return true;
      case 'l':
        userLoop(!loopOn);
        return true;
      case 'm':
        userMarker();
        return true;
      case '[':
        userJump(-1);
        return true;
      case ']':
        userJump(1);
        return true;
      default:
        return false;
    }
  }

  /** 示範時用目前的時間；減少動態時一口氣跑完，改用固定的示意時間。 */
  const now = (fixed: number): number => (player.skipping ? fixed : time);

  const player: DemoPlayer = new DemoPlayer({
    intro: '播放列：循環片段與時間軸標籤。',
    outro: '示範結束。換你試試：拖把手、雙擊時間軸上方，或在這裡按 I、O、L、M、[、]。',
    reset,
    steps: [
      { say: '按「播放」：播放頭開始往前跑。', target: () => playBtn, run: () => (playing = true), hold: 1500 },
      {
        say: '跑到 2 秒左右，按「設為起點」（I）：循環會自動打開。',
        target: () => startBtn,
        key: 'I',
        run: () => setStart(now(2)),
        hold: 1700,
      },
      {
        say: '再按「設為終點」（O）：播放頭就在這一段之間反覆。',
        target: () => endBtn,
        key: 'O',
        run: () => setEnd(now(4.2)),
        hold: 3200,
      },
      { say: '下方的兩個把手也能直接拖：把終點拉到 5 秒。', target: () => rangeLane?.querySelector('.range-handle--end'), hold: 300 },
      {
        say: '下方的兩個把手也能直接拖：把終點拉到 5 秒。',
        target: () => rangeLane?.querySelector('[data-anchor="5"]'),
        early: true,
        run: () => (loopEnd = 5),
        hold: 2400,
      },
      { say: '按「暫停」。', target: () => playBtn, run: () => (playing = false), hold: 1000 },
      { say: '按 M：在目前的時間加一個標籤。', key: 'M', run: () => addMarker(now(3.1)), hold: 1500 },
      {
        say: '在時間軸上方的空白處雙擊，也能加標籤。',
        target: () => markerLane?.querySelector('[data-anchor="6.5"]'),
        run: () => addMarker(6.5),
        hold: 1600,
      },
      { say: '按 ]：跳到下一個標籤。', key: ']', run: () => jump(1), hold: 1400 },
      { say: '按 [：跳回上一個標籤。', key: '[', run: () => jump(-1), hold: 1600 },
    ],
  });
</script>

<Stage label="循環片段與時間軸標籤" {player} {onkey}>
  <div class="transport">
    <div class="row row-wrap">
      <button
        class="btn btn--primary play-btn"
        bind:this={playBtn}
        onclick={userPlay}
        aria-label={playing ? '暫停' : '播放'}
      >
        <Icon name={playing ? 'pause' : 'play'} size={13} />{playing ? '暫停' : '播放'}
      </button>
      <button class="btn" use:bounce={-1} onclick={userReset}><Icon name="skip-back" />重置</button>
      <button class="btn btn--icon nudge" use:bounce={-1} onclick={() => userNudge(-0.1)} aria-label="後退 0.1 秒">
        <Icon name="chevron-left" />0.1s
      </button>
      <button class="btn btn--icon nudge" use:bounce={1} onclick={() => userNudge(0.1)} aria-label="前進 0.1 秒">
        0.1s<Icon name="chevron-right" />
      </button>
      <span class="clock mono">{formatClock(time)}</span>
      <span class="muted small mono total">／ {formatClock(DURATION)}</span>
      <button class="btn" onclick={() => userMarker()} title="在目前時間加標籤（M）"><Icon name="bookmark-plus" />標籤</button>
    </div>

    <div class="timeline" class:is-dimmed={dim}>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="marker-lane" bind:this={markerLane} ondblclick={onLaneDblClick} title="雙擊加標籤">
        {#if markers.length === 0}
          <span class="lane-hint">雙擊這裡或按 M 加標籤</span>
        {/if}
        <span class="anchor" data-anchor="6.5" style:left={`${pct(6.5)}%`}></span>
        {#each sortedMarkers as item (item.id)}
          <button
            class="marker"
            class:is-end={item.time > DURATION * 0.85}
            style:left={`${pct(item.time)}%`}
            aria-label={`標籤 ${item.label}，${formatClock(item.time)}，按下跳過去`}
            onclick={() => userGoMarker(item)}
          >
            <span class="marker-text">{item.label}</span>
          </button>
        {/each}
      </div>

      <div class="track">
        <div class="track-bg" aria-hidden="true"></div>
        <div class="marks" aria-hidden="true">
          <div class="outside outside--before" class:is-on={dim} style:transform={`scaleX(${loopStart / DURATION})`}></div>
          <div class="outside outside--after" class:is-on={dim} style:transform={`scaleX(${(DURATION - loopEnd) / DURATION})`}></div>
          {#each sortedMarkers as item (item.id)}
            <div class="marker-line" style:left={`${pct(item.time)}%`}></div>
          {/each}
          {#each TICKS as tick (tick)}
            <div class="tick" class:tick--muted={dim && (tick < loopStart || tick > loopEnd)} style:left={`${pct(tick)}%`}></div>
          {/each}
          <div class="slider-layer" style:transform={`translateX(${pct(time)}%)`}>
            <div class="playhead"></div>
          </div>
        </div>
        <input
          class="seek"
          type="range"
          min="0"
          max={DURATION}
          step="0.01"
          value={time}
          aria-label="播放時間（秒）"
          oninput={(event) => userSeek(Number(event.currentTarget.value))}
        />
      </div>

      <div class="range-lane" class:is-active={loopOn} bind:this={rangeLane}>
        <div
          class="range-bar"
          class:is-dragging={drag !== null}
          style:transform={`translateX(${pct(loopStart)}%) scaleX(${(loopEnd - loopStart) / DURATION})`}
        ></div>
        <span class="anchor" data-anchor="5" style:left={`${pct(5)}%`}></span>
        {#each [{ which: 'start', value: loopStart, label: '片段起點' }, { which: 'end', value: loopEnd, label: '片段終點' }] as handle (handle.which)}
          {@const which = handle.which as 'start' | 'end'}
          <div class="slider-layer" class:is-dragging={drag !== null} style:transform={`translateX(${pct(handle.value)}%)`}>
            <div
              class="range-handle"
              class:range-handle--end={which === 'end'}
              class:is-dragging={drag === which}
              role="slider"
              tabindex="0"
              aria-label={handle.label}
              aria-valuemin={0}
              aria-valuemax={DURATION}
              aria-valuenow={handle.value}
              aria-valuetext={formatClock(handle.value)}
              onpointerdown={(event) => onHandleDown(which, event)}
              onpointermove={onHandleMove}
              onpointerup={onHandleUp}
              onpointercancel={onHandleUp}
              onkeydown={(event) => onHandleKey(which, event)}
            ></div>
          </div>
        {/each}
      </div>
    </div>

    <div class="row row-wrap loop-row">
      <label class="check">
        <input type="checkbox" checked={loopOn} onchange={(event) => userLoop(event.currentTarget.checked)} />
        <span>循環片段</span>
      </label>
      <button class="btn btn--icon" bind:this={startBtn} onclick={userStart} title="目前時間設為片段起點（I）">
        <Icon name="to-start" />設為起點
      </button>
      <button class="btn btn--icon" bind:this={endBtn} onclick={userEnd} title="目前時間設為片段終點（O）">
        <Icon name="to-end" />設為終點
      </button>
      <span class="mono xsmall range-text" class:muted={!loopOn}>{formatClock(loopStart)} – {formatClock(loopEnd)}</span>
      <button class="btn btn--icon" onclick={userClear} disabled={rangeIsFull && !loopOn} title="片段回到整段並關閉循環">
        <Icon name="move-horizontal" />整段
      </button>
    </div>

    <div class="keys" role="group" aria-label="快捷鍵">
      <Keycap cap="I" label="設為起點" lit={player.lit === 'I'} onclick={userStart} />
      <Keycap cap="O" label="設為終點" lit={player.lit === 'O'} onclick={userEnd} />
      <Keycap cap="L" label="循環片段" lit={player.lit === 'L'} onclick={() => userLoop(!loopOn)} />
      <Keycap cap="M" label="加標籤" lit={player.lit === 'M'} onclick={() => userMarker()} />
      <Keycap cap="[" label="上一個標籤" lit={player.lit === '['} onclick={() => userJump(-1)} />
      <Keycap cap="]" label="下一個標籤" lit={player.lit === ']'} onclick={() => userJump(1)} />
    </div>
  </div>
</Stage>

<style>
  .transport {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .play-btn {
    min-width: 5.5rem;
  }

  .clock {
    min-width: 9ch;
    color: var(--c-text-strong);
    font-size: var(--fs-lg);
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .total {
    min-width: 9ch;
  }

  /* 時間軸：標籤列、時間軸、把手列共用一條座標軸（左右各內縮半個拖曳點）。 */
  .timeline {
    --thumb: 14px;
    --inset: calc(var(--thumb) / 2);
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: var(--space-1);
  }

  .marker-lane {
    position: relative;
    height: 22px;
    margin: 0 var(--inset);
    cursor: copy;
  }

  .lane-hint {
    position: absolute;
    top: 50%;
    left: 0;
    transform: translateY(-50%);
    color: var(--c-text-dim);
    font-size: var(--fs-xs);
    pointer-events: none;
  }

  .anchor {
    position: absolute;
    top: 50%;
    width: 0;
    height: 0;
    pointer-events: none;
  }

  /* 標籤：貼在時間點右側（靠右端改貼左側），左下角對準下方的標記線。 */
  .marker {
    position: absolute;
    bottom: 0;
    display: flex;
    align-items: center;
    max-width: 132px;
    height: 18px;
    padding: 0 6px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--c-text);
    background: var(--c-control-hover);
    border: 1px solid var(--c-border-strong);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    cursor: pointer;
  }

  .marker.is-end {
    transform: translateX(-100%);
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }

  .marker-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .marker:focus-visible {
    outline-offset: 1px;
  }

  @media (hover: hover) {
    .marker:hover {
      color: var(--c-on-invert);
      background: var(--c-invert-bg);
      border-color: var(--c-invert-bg);
    }
  }

  .track {
    position: relative;
    height: 26px;
  }

  .track-bg {
    position: absolute;
    inset: 0;
    background: var(--c-control);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
  }

  .marks {
    position: absolute;
    top: 1px;
    bottom: 1px;
    left: var(--inset);
    right: var(--inset);
    pointer-events: none;
  }

  /* 片段外：不用半透明遮罩，直接換成最深的底色；寬度用 scaleX 跟著把手走。 */
  .outside {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 100%;
    background: var(--c-bg);
    opacity: 0;
    transition:
      transform 600ms var(--ease-out),
      opacity 160ms ease;
  }

  .outside.is-on {
    opacity: 1;
  }

  .outside--before {
    transform-origin: left center;
  }

  .outside--after {
    transform-origin: right center;
  }

  .marker-line {
    position: absolute;
    top: -1px;
    bottom: -1px;
    width: 1px;
    background: var(--c-border-strong);
  }

  .tick {
    position: absolute;
    top: 7px;
    width: 3px;
    height: 10px;
    margin-left: -1px;
    background: var(--c-text-dim);
  }

  .tick--muted {
    background: var(--c-border-strong);
  }

  /* 和軌道一樣寬的一層，用 translateX 的百分比把播放頭、把手移到對的時間。 */
  .slider-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .range-lane .slider-layer {
    transition: transform 600ms var(--ease-out);
  }

  .range-lane .slider-layer.is-dragging {
    transition: none;
  }

  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -1px;
    width: 2px;
    background: var(--c-text);
  }

  .seek {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    appearance: none;
    background: transparent;
  }

  .seek::-webkit-slider-runnable-track {
    height: 100%;
    background: transparent;
  }

  .seek::-webkit-slider-thumb {
    appearance: none;
    width: var(--thumb);
    height: var(--thumb);
    margin-top: 6px;
    border: 2px solid var(--c-bg);
    border-radius: 50%;
    background: var(--c-text-strong);
    box-shadow: none;
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
    background: var(--c-text-strong);
  }

  .range-lane {
    position: relative;
    height: 18px;
    margin: 0 var(--inset);
  }

  .range-bar {
    position: absolute;
    top: 2px;
    left: 0;
    width: 100%;
    height: 3px;
    background: var(--c-border-strong);
    transform-origin: left center;
    transition: transform 600ms var(--ease-out);
  }

  .range-bar.is-dragging {
    transition: none;
  }

  .range-lane.is-active .range-bar {
    background: var(--c-text-dim);
  }

  /* 把手：朝時間軸的直角括號，起點開口向右、終點開口向左，不靠顏色也分得出方向。 */
  .range-handle {
    position: absolute;
    top: 0;
    left: -1px;
    width: 12px;
    height: 16px;
    border: 3px solid var(--c-border-strong);
    border-right: none;
    border-bottom: none;
    border-radius: 2px 0 0 0;
    cursor: ew-resize;
    touch-action: none;
    pointer-events: auto;
  }

  .range-handle--end {
    left: -11px;
    border-left: none;
    border-right: 3px solid var(--c-border-strong);
    border-radius: 0 2px 0 0;
  }

  .range-lane.is-active .range-handle {
    border-color: var(--c-text);
  }

  .range-handle:focus-visible,
  .range-handle.is-dragging {
    border-color: var(--c-focus);
  }

  @media (hover: hover) {
    .range-handle:hover {
      border-color: var(--c-focus);
    }
  }

  .range-handle:focus-visible {
    outline-offset: 1px;
  }

  /* 觸控：把手的感應區放大到 44px，外觀不變。 */
  @media (pointer: coarse) {
    .range-handle::after {
      content: '';
      position: absolute;
      inset: -14px -16px;
    }
  }

  .loop-row {
    gap: var(--space-2);
  }

  .range-text {
    min-width: 19ch;
  }

  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    padding-top: var(--space-2);
    border-top: 1px solid var(--c-border);
  }

  @media (prefers-reduced-motion: reduce) {
    .outside,
    .range-bar,
    .range-lane .slider-layer {
      transition: none;
    }
  }
</style>
