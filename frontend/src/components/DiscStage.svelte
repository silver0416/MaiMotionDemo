<script lang="ts">
  import maimaiBackground from '../../../resource/maimai.png';
  import {
    BUTTONS,
    IMAGE_HEIGHT,
    IMAGE_WIDTH,
    buttonPoint,
    centerPx,
    chevrons,
    pointOnSamples,
    polylinePath,
    radiusPx,
    toStage,
    toStageLength,
  } from '../lib/disc';
  import { approachScale, noteVisual, pathLength, scalePoint, type NoteVisual } from '../lib/notes';
  import { sampleWithStarts, trackSlice, type HandState } from '../lib/motion';
  import { MODE_LABEL, KIND_LABEL, shapeLabel } from '../lib/contract';
  import { formatClock } from '../lib/format';
  import { session } from '../state/session.svelte';
  import { playback } from '../state/playback.svelte';
  import { view } from '../state/view.svelte';
  import type { Hand, Note, PathSample, Point } from '../lib/types';

  const time = $derived(playback.time);
  const calibration = $derived(view.calibration);
  const center = $derived(centerPx(calibration));
  const discRadius = $derived(radiusPx(calibration));

  const zoomTransform = $derived(
    `translate(${center.x} ${center.y}) scale(${view.zoom}) translate(${-center.x} ${-center.y})`,
  );

  const visuals = $derived.by<NoteVisual[]>(() =>
    session.notes.map((note) =>
      noteVisual(note, time, view.noteMode, view.approach, view.approachSeconds),
    ),
  );

  const leftState = $derived.by<HandState | null>(() => {
    const solution = session.solution;
    if (!solution) return null;
    return sampleWithStarts(solution.leftSegments, session.leftStarts, time);
  });

  const rightState = $derived.by<HandState | null>(() => {
    const solution = session.solution;
    if (!solution) return null;
    return sampleWithStarts(solution.rightSegments, session.rightStarts, time);
  });

  const leftFullPath = $derived.by(() =>
    session.leftTrack ? polylinePath(session.leftTrack.points, calibration) : '',
  );
  const rightFullPath = $derived.by(() =>
    session.rightTrack ? polylinePath(session.rightTrack.points, calibration) : '',
  );

  const leftTrail = $derived.by(() =>
    session.leftTrack
      ? polylinePath(trackSlice(session.leftTrack, time - view.trailSeconds, time), calibration)
      : '',
  );
  const rightTrail = $derived.by(() =>
    session.rightTrack
      ? polylinePath(trackSlice(session.rightTrack, time - view.trailSeconds, time), calibration)
      : '',
  );

  interface HandoverMarker {
    key: string;
    point: Point;
    label: string;
    active: boolean;
    startSeconds: number;
  }

  const handoverMarkers = $derived.by<HandoverMarker[]>(() => {
    const solution = session.solution;
    if (!solution) return [];
    // 碰頭互換兩條 Slide 各記一筆，位置與時間相同，盤面上只畫一個標記。
    const shown = solution.handovers.filter(
      (handover, index) =>
        !handover.swap ||
        solution.handovers.findIndex(
          (other) => other.swap && other.startSeconds === handover.startSeconds,
        ) === index,
    );
    return shown.map((handover, index) => {
      const source = handover.from === 'L' ? solution.leftSegments : solution.rightSegments;
      const starts = handover.from === 'L' ? session.leftStarts : session.rightStarts;
      const state = sampleWithStarts(source, starts, handover.startSeconds);
      return {
        key: `${handover.noteId}-${index}`,
        point: state?.point ?? { x: 0, y: 0 },
        label: handover.swap ? '互換' : `${handover.from}→${handover.to}`,
        active: time >= handover.startSeconds - 0.15 && time <= handover.endSeconds + 0.35,
        startSeconds: handover.startSeconds,
      };
    });
  });

  function tagFor(noteId: string): { text: string; hand: Hand; handover: boolean } | null {
    const list = session.assignmentsByNote.get(noteId) ?? [];
    if (list.length === 0) return null;
    const handovers = session.handoversByNote.get(noteId) ?? [];
    if (handovers.length > 0) {
      const first = handovers[0];
      const last = handovers[handovers.length - 1];
      return { text: `${first.from}→${last.to}`, hand: first.from, handover: true };
    }
    const head = list.find((item) => item.part === 'head' || item.part === 'contact');
    const slide = list.find((item) => item.part === 'slide');
    // 起點與軌道分屬不同手（且沒有交接段）時要講清楚，不能只寫其中一隻手。
    if (head && slide && head.hand !== slide.hand) {
      return { text: `${head.hand}起${slide.hand}滑`, hand: head.hand, handover: false };
    }
    const hands = [...new Set(list.map((item) => item.hand))];
    return { text: hands.join('/'), hand: hands[0], handover: false };
  }

  function px(point: Point): Point {
    return toStage(calibration, point);
  }

  function len(value: number): number {
    return toStageLength(calibration, value);
  }

  function diamondPath(atPoint: Point, radius: number): string {
    const p = px(atPoint);
    const r = len(radius);
    return `M${p.x} ${p.y - r}L${p.x + r} ${p.y}L${p.x} ${p.y + r}L${p.x - r} ${p.y}Z`;
  }

  /** 音符的簡短說明，例如「Tap 3」「Touch B5」「Slide 直線 1→5」。 */
  function noteCaption(note: Note): string {
    const kind = KIND_LABEL[note.kind] ?? note.kind;
    if (note.touchArea) {
      return `${kind} ${note.touchArea}${note.button > 0 ? note.button : ''}`;
    }
    if (note.kind === 'slide' && note.pathId) {
      const path = session.pathById.get(note.pathId);
      if (path) {
        return `${kind} ${shapeLabel(path.shape)} ${path.startButton}→${path.endButton}`;
      }
    }
    return `${kind} ${note.button}`;
  }

  function starPath(atPoint: Point, outer: number, inner: number): string {
    const p = px(atPoint);
    const ro = len(outer);
    const ri = len(inner);
    let d = '';
    for (let i = 0; i < 10; i += 1) {
      const r = i % 2 === 0 ? ro : ri;
      const angle = (-90 + i * 36) * (Math.PI / 180);
      const x = p.x + Math.cos(angle) * r;
      const y = p.y + Math.sin(angle) * r;
      d += `${i === 0 ? 'M' : 'L'}${x.toFixed(2)} ${y.toFixed(2)}`;
    }
    return `${d}Z`;
  }

  function partialPath(samples: PathSample[], upTo: number): string {
    if (samples.length === 0 || upTo <= 0) return '';
    const points: Point[] = [];
    for (const sample of samples) {
      if (sample.u > upTo) break;
      points.push({ x: sample.x, y: sample.y });
    }
    points.push(pointOnSamples(samples, upTo));
    return polylinePath(points, calibration);
  }

  function holdEnds(note: Note, visual: NoteVisual): { head: Point; tail: Point } {
    const factor = approachScale(visual);
    const head = scalePoint(note.position, factor);
    const duration = Math.max(note.endSeconds - note.timeSeconds, 0);
    const length = Math.min(0.62, Math.max(0.14, duration * 0.42));
    const tail = scalePoint(note.position, Math.max(factor - length, 0.08));
    return { head, tail };
  }

  function noteLabel(note: Note, visual: NoteVisual): string {
    const tag = tagFor(note.id);
    const hand = tag ? `，指派 ${tag.text}` : '，此候選沒有指派資料';
    return `${noteCaption(note)}，音符 ${note.id}，時間 ${formatClock(note.timeSeconds)}${hand}。目前狀態 ${
      visual.phase === 'upcoming' ? '未到' : visual.phase === 'active' ? '進行中' : '已過'
    }`;
  }

  function selectNote(noteId: string) {
    session.selectNote(session.selectedNoteId === noteId ? null : noteId);
  }

  function onNoteKey(event: KeyboardEvent, noteId: string) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      selectNote(noteId);
    }
  }

  const handMarkerRadius = $derived(len(0.075));
  // 右手方框畫得比左手圓稍大，交接時兩手位置重疊也看得出兩個標記。
  const rightMarkerRadius = $derived(len(0.1));

  // 盤面圓幾乎佔滿背景圖，額外留白避免標記與文字被裁掉。
  const MARGIN_X = 90;
  const MARGIN_TOP = 105;
  const MARGIN_BOTTOM = 105;
  const VIEW_BOX = `${-MARGIN_X} ${-MARGIN_TOP} ${IMAGE_WIDTH + MARGIN_X * 2} ${
    IMAGE_HEIGHT + MARGIN_TOP + MARGIN_BOTTOM
  }`;
</script>

<div class="stage-frame">
  <svg
    class="stage-svg"
    viewBox={VIEW_BOX}
    preserveAspectRatio="xMidYMid meet"
    aria-label="maimai 盤面：音符與左右手軌跡"
  >
    <g transform={zoomTransform}>
      <image
        href={maimaiBackground}
        x="0"
        y="0"
        width={IMAGE_WIDTH}
        height={IMAGE_HEIGHT}
        preserveAspectRatio="none"
      />

      {#if view.showCalibrationRing}
        <g class="calib">
          <circle cx={center.x} cy={center.y} r={discRadius} />
          <line x1={center.x - discRadius} y1={center.y} x2={center.x + discRadius} y2={center.y} />
          <line x1={center.x} y1={center.y - discRadius} x2={center.x} y2={center.y + discRadius} />
          <circle cx={center.x} cy={center.y} r={len(0.02)} />
        </g>
      {/if}

      {#if view.showButtons}
        <g class="buttons">
          {#each BUTTONS as button (button)}
            {@const outer = px(buttonPoint(button))}
            {@const inner = px(scalePoint(buttonPoint(button), 0.86))}
            <circle cx={outer.x} cy={outer.y} r={len(0.045)} />
            <text x={inner.x} y={inner.y} dominant-baseline="central" text-anchor="middle">
              {button}
            </text>
          {/each}
        </g>
      {/if}

      {#if view.showNotes}
        <!-- Slide 軌道：直接畫 Rust 輸出的路徑取樣點，前端不重算路徑 -->
        <g class="paths">
          {#each visuals as visual (visual.note.id)}
            {#if visual.visible && visual.note.pathId}
              {@const path = session.pathById.get(visual.note.pathId)}
              {#if path}
                {@const arrows = chevrons(
                  path.samples,
                  Math.min(26, Math.max(3, Math.round(pathLength(path.samples) / 0.16))),
                )}
                <g
                  class="slide-path"
                  class:is-selected={session.selectedNoteId === visual.note.id}
                  class:is-break={visual.note.modifiers.breakSlide}
                  opacity={visual.opacity}
                >
                  {#each path.branches as branch, index (index)}
                    <path class="slide-path-branch" d={polylinePath(branch, calibration)} />
                  {/each}
                  <path class="slide-path-base" d={polylinePath(path.samples, calibration)} />
                  {#if visual.slideU > 0}
                    <path class="slide-path-done" d={partialPath(path.samples, visual.slideU)} />
                  {/if}
                  {#each arrows as arrow, index (index)}
                    {@const point = px(arrow)}
                    <path
                      class="chevron"
                      class:is-passed={arrow.u <= visual.slideU}
                      d={`M${-len(0.035)} ${-len(0.032)} L0 0 L${-len(0.035)} ${len(0.032)}`}
                      transform={`translate(${point.x} ${point.y}) rotate(${arrow.angle})`}
                    />
                  {/each}
                </g>
              {/if}
            {/if}
          {/each}
        </g>

        <g class="notes">
          {#each visuals as visual (visual.note.id)}
            {@const note = visual.note}
            {#if visual.visible}
              {@const factor = approachScale(visual)}
              {@const center2 = px(scalePoint(note.position, factor))}
              {@const tag = view.showAssignmentTags ? tagFor(note.id) : null}
              <g
                class="note"
                class:is-selected={session.selectedNoteId === note.id}
                class:is-active={visual.phase === 'active'}
                class:is-break={note.modifiers.breakNote}
                class:is-ex={note.modifiers.ex}
                opacity={visual.opacity}
                role="button"
                tabindex="0"
                aria-label={noteLabel(note, visual)}
                onclick={() => selectNote(note.id)}
                onkeydown={(event) => onNoteKey(event, note.id)}
              >
                {#if note.kind === 'touch' || note.kind === 'touchHold'}
                  {@const at = scalePoint(note.position, factor)}
                  {@const mark = px(at)}
                  <path class="touch-ring" d={diamondPath(at, 0.105)} />
                  <path class="touch-core" d={diamondPath(at, 0.055)} />
                  {#if note.kind === 'touchHold' && visual.phase === 'active'}
                    <path class="touch-progress" d={diamondPath(at, 0.055 + 0.05 * (1 - visual.progress))} />
                  {/if}
                  <text
                    class="touch-label"
                    x={mark.x}
                    y={mark.y}
                    text-anchor="middle"
                    dominant-baseline="central"
                  >
                    {note.touchArea}{note.button > 0 ? note.button : ''}
                  </text>
                {:else if note.kind === 'hold'}
                  {@const ends = holdEnds(note, visual)}
                  {@const head = px(ends.head)}
                  {@const tail = px(ends.tail)}
                  <line
                    class="hold-bar"
                    x1={head.x}
                    y1={head.y}
                    x2={tail.x}
                    y2={tail.y}
                    stroke-width={len(0.105)}
                    stroke-linecap="round"
                  />
                  <line
                    class="hold-core"
                    x1={head.x}
                    y1={head.y}
                    x2={tail.x}
                    y2={tail.y}
                    stroke-width={len(0.05)}
                    stroke-linecap="round"
                  />
                  {#if visual.phase === 'active'}
                    <line
                      class="hold-progress"
                      x1={head.x}
                      y1={head.y}
                      x2={head.x + (tail.x - head.x) * (1 - visual.progress)}
                      y2={head.y + (tail.y - head.y) * (1 - visual.progress)}
                      stroke-width={len(0.05)}
                      stroke-linecap="round"
                    />
                  {/if}
                  <circle class="note-ring" cx={head.x} cy={head.y} r={len(0.085)} />
                {:else if note.kind === 'slide'}
                  {#if note.hasHead}
                    <path class="note-star" d={starPath(scalePoint(note.position, factor), 0.1, 0.045)} />
                  {/if}
                  {#if visual.slideU > 0 && visual.slideU < 1 && note.pathId}
                    {@const path = session.pathById.get(note.pathId)}
                    {#if path}
                      <path
                        class="note-star is-moving"
                        d={starPath(pointOnSamples(path.samples, visual.slideU), 0.085, 0.038)}
                      />
                    {/if}
                  {/if}
                {:else}
                  <circle class="note-ring" cx={center2.x} cy={center2.y} r={len(0.085)} />
                  <circle class="note-core" cx={center2.x} cy={center2.y} r={len(0.048)} />
                {/if}

                {#if tag}
                  {@const anchor = px(scalePoint(note.position, Math.max(factor - 0.17, 0.1)))}
                  <text
                    class="note-tag"
                    class:is-left={tag.hand === 'L'}
                    class:is-right={tag.hand === 'R'}
                    class:is-handover={tag.handover}
                    x={anchor.x}
                    y={anchor.y}
                    text-anchor="middle"
                    dominant-baseline="central"
                  >
                    {tag.text}
                  </text>
                {/if}
              </g>
            {/if}
          {/each}
        </g>
      {/if}

      {#if session.hasHands}
        <g class="tracks">
          {#if view.showFullTrack && view.showLeft}
            <path class="track track--left" d={leftFullPath} />
          {/if}
          {#if view.showFullTrack && view.showRight}
            <path class="track track--right" d={rightFullPath} />
          {/if}
          {#if view.showRecentTrail && view.showLeft}
            <path class="trail trail--left" d={leftTrail} />
          {/if}
          {#if view.showRecentTrail && view.showRight}
            <path class="trail trail--right" d={rightTrail} />
          {/if}
        </g>

        {#if view.showHandovers}
          <g class="handovers">
            {#each handoverMarkers as marker (marker.key)}
              {@const point = px(marker.point)}
              <g class="handover" class:is-active={marker.active}>
                <path
                  class="handover-diamond"
                  d={`M0 ${-len(0.06)} L${len(0.06)} 0 L0 ${len(0.06)} L${-len(0.06)} 0 Z`}
                  transform={`translate(${point.x} ${point.y})`}
                />
                <text
                  class="handover-label"
                  x={point.x}
                  y={point.y - len(0.11)}
                  text-anchor="middle"
                >
                  {marker.label === '互換' ? '兩手互換' : `換手 ${marker.label}`}
                </text>
              </g>
            {/each}
          </g>
        {/if}

        <g class="hands">
          {#if view.showRight && rightState}
            {@const point = px(rightState.point)}
            <g class="hand hand--right" class:is-contact={rightState.contacting}>
              <rect
                x={point.x - rightMarkerRadius}
                y={point.y - rightMarkerRadius}
                width={rightMarkerRadius * 2}
                height={rightMarkerRadius * 2}
                rx={len(0.022)}
              />
              {#if rightState.contacting}
                <circle class="contact-ring" cx={point.x} cy={point.y} r={len(0.145)} />
              {/if}
              <text x={point.x} y={point.y} text-anchor="middle" dominant-baseline="central">R</text>
              <text class="hand-mode" x={point.x} y={point.y + len(0.17)} text-anchor="middle">
                {MODE_LABEL[rightState.mode] ?? rightState.mode}
              </text>
            </g>
          {/if}
          {#if view.showLeft && leftState}
            {@const point = px(leftState.point)}
            <g class="hand hand--left" class:is-contact={leftState.contacting}>
              <circle cx={point.x} cy={point.y} r={handMarkerRadius} />
              {#if leftState.contacting}
                <circle class="contact-ring" cx={point.x} cy={point.y} r={len(0.115)} />
              {/if}
              <text x={point.x} y={point.y} text-anchor="middle" dominant-baseline="central">L</text>
              <text class="hand-mode" x={point.x} y={point.y + len(0.17)} text-anchor="middle">
                {MODE_LABEL[leftState.mode] ?? leftState.mode}
              </text>
            </g>
          {/if}
        </g>
      {/if}
    </g>
  </svg>
</div>

<style>
  /* 絕對定位並置中：盤面尺寸完全由外層可用空間決定，
     永遠不會用自己的寬度把播放控制擠出視窗。 */
  .stage-frame {
    position: absolute;
    inset: 0;
    margin: auto;
    height: 100%;
    width: auto;
    aspect-ratio: 1;
    max-width: 100%;
    background: #000;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  /* 絕對定位，避免 SVG 的內建 980px 高度把外層 grid 撐開。 */
  .stage-svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }

  .calib circle {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 2;
    stroke-dasharray: 10 8;
  }

  .calib line {
    stroke: var(--c-accent);
    stroke-width: 1.5;
    stroke-dasharray: 6 8;
  }

  .buttons circle {
    fill: none;
    stroke: #8d97a6;
    stroke-width: 2;
  }

  .buttons text {
    fill: #c8d1de;
    font-family: var(--font-mono);
    font-size: 30px;
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 5px;
  }

  .slide-path-base {
    fill: none;
    stroke: #6d7788;
    stroke-width: 5;
    stroke-linecap: round;
  }

  .slide-path-done {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 6;
    stroke-linecap: round;
  }

  .slide-path-branch {
    fill: none;
    stroke: #55606f;
    stroke-width: 4;
    stroke-linecap: round;
  }

  .slide-path.is-selected .slide-path-base {
    stroke: var(--c-accent);
    stroke-width: 7;
  }

  .slide-path.is-break .slide-path-base,
  .slide-path.is-break .slide-path-branch {
    stroke: #c9863a;
  }

  .chevron {
    fill: none;
    stroke: #9aa5b5;
    stroke-width: 5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .chevron.is-passed {
    stroke: #ffffff;
  }

  .note {
    cursor: pointer;
  }

  .note-ring {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 7;
  }

  .note-core {
    fill: none;
    stroke: #aab4c3;
    stroke-width: 4;
  }

  .note-star {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 6;
    stroke-linejoin: round;
  }

  .note-star.is-moving {
    fill: #eef2f8;
    stroke: #0b0d11;
    stroke-width: 3;
  }

  .hold-bar {
    stroke: #6f7a8a;
  }

  .hold-core {
    stroke: #171b21;
  }

  .hold-progress {
    stroke: #eef2f8;
  }

  .touch-ring {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 6;
    stroke-linejoin: round;
  }

  .touch-core {
    fill: none;
    stroke: #8e9aab;
    stroke-width: 4;
    stroke-linejoin: round;
  }

  .touch-progress {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 4;
    stroke-linejoin: round;
  }

  .touch-label {
    fill: #c8d1de;
    font-family: var(--font-mono);
    font-size: 26px;
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 5px;
  }

  /* Break 與 EX 只換筆觸顏色，形狀維持一致，不單靠顏色傳達種類。 */
  .note.is-break .note-ring,
  .note.is-break .note-star,
  .note.is-break .touch-ring,
  .note.is-break .hold-bar {
    stroke: #f0913a;
  }

  .note.is-ex .note-core,
  .note.is-ex .touch-core {
    stroke: var(--c-focus);
    stroke-width: 6;
  }

  .note.is-active .note-ring {
    stroke-width: 9;
  }

  .note.is-selected .note-ring,
  .note.is-selected .note-star {
    stroke: var(--c-accent);
    stroke-width: 10;
  }

  .note:focus-visible {
    outline: none;
  }

  .note:focus-visible .note-ring,
  .note:focus-visible .note-star,
  .note:focus-visible .hold-bar {
    stroke: var(--c-focus);
  }

  .note-tag {
    font-family: var(--font-mono);
    font-size: 30px;
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 6px;
  }

  .note-tag.is-left {
    fill: var(--c-left);
  }

  .note-tag.is-right {
    fill: var(--c-right);
  }

  .note-tag.is-handover {
    fill: var(--c-accent);
  }

  .track {
    fill: none;
    stroke-width: 3;
    opacity: 0.55;
  }

  .track--left {
    stroke: var(--c-left);
  }

  .track--right {
    stroke: var(--c-right);
    stroke-dasharray: 14 9;
  }

  .trail {
    fill: none;
    stroke-width: 8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .trail--left {
    stroke: var(--c-left);
  }

  .trail--right {
    stroke: var(--c-right);
    stroke-dasharray: 20 10;
  }

  .handover-diamond {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 4;
  }

  .handover.is-active .handover-diamond {
    fill: var(--c-accent);
  }

  .handover-label {
    fill: var(--c-accent);
    font-family: var(--font-mono);
    font-size: 26px;
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 6px;
  }

  .hand circle,
  .hand rect {
    fill: #11151a;
    stroke-width: 6;
  }

  .hand text {
    font-family: var(--font-mono);
    font-size: 34px;
    font-weight: 700;
    fill: #ffffff;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 6px;
  }

  .hand-mode {
    font-size: 24px !important;
    font-weight: 600 !important;
  }

  .hand--left circle {
    stroke: var(--c-left);
  }

  .hand--left text {
    fill: var(--c-left);
  }

  .hand--right circle,
  .hand--right rect {
    stroke: var(--c-right);
  }

  .hand--right text {
    fill: var(--c-right);
  }

  .hand--left.is-contact circle {
    fill: var(--c-left);
  }

  .hand--right.is-contact rect {
    fill: var(--c-right);
  }

  .hand--left.is-contact > text {
    fill: #0b0d11;
    stroke-width: 0;
  }

  .hand--right.is-contact > text {
    fill: #0b0d11;
    stroke-width: 0;
  }

  .hand--left.is-contact .hand-mode {
    fill: var(--c-left);
    stroke: #000;
    stroke-width: 6px;
  }

  .hand--right.is-contact .hand-mode {
    fill: var(--c-right);
    stroke: #000;
    stroke-width: 6px;
  }

  .contact-ring {
    fill: none !important;
    stroke-width: 3 !important;
    stroke-dasharray: 8 7;
  }
</style>
