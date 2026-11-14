<script lang="ts">
  /**
   * 盤面演示（徽章「核心實際結果」）：播放 scripts/gen-demos.mjs 用 Rust 核心算出的 ../data/<name>.json。
   * 畫法照 App 的 DiscStage.svelte，但資料全部來自載入的 JSON，不依賴 App 的 session／playback／view。
   * 前端不重算左右手、不重算路徑，只依單一播放時鐘查詢核心給的動作段。
   */
  import './disc.css';
  import DiscBoard from '$app/components/DiscBoard.svelte';
  import Icon from '$app/components/Icon.svelte';
  import { BUTTONS, buttonPoint, chevrons, pointOnSamples } from '$app/lib/disc';
  import {
    approachScale,
    noteVisual,
    pathLength,
    scalePoint,
    simultaneousSlideHands,
    touchGather,
  } from '$app/lib/notes';
  import { buildTrack, sampleWithStarts, trackSlice, type HandState, type Track } from '$app/lib/motion';
  import { MODE_LABEL } from '$app/lib/contract';
  import { activePalms, coveredTargets, palmCovers } from '$app/lib/palm';
  import {
    TOUCH_ICON_ANGLE,
    fireworkBursts,
    fireworkShape,
    isFireworkTouch,
    isTouchNote,
    radialAngle,
    sensorKey,
    sparkAngles,
    touchHoldFrame,
    touchHoldRingRadius,
    touchName,
    touchOuterRadius,
    touchPetals,
    touchRadius,
    touchSparkRadius,
  } from '$app/lib/touch';
  import { formatClock } from '$app/lib/format';
  import { reducedMotion } from '$app/lib/press';
  import { segmented } from '$app/lib/segmented';
  import type {
    Assignment,
    Handover,
    Hand,
    MotionSegment,
    Note,
    PalmPlacement,
    Point,
    SlidePath,
    TouchSensor,
  } from '$app/lib/types';
  import {
    CENTER,
    DISC_RADIUS,
    VIEW_BOX,
    chevronPath,
    closedPath,
    diamondPath,
    holdLength,
    holdOutline,
    len,
    linePath,
    palmCrossPath,
    palmLabel,
    partialPath,
    px,
    sensorRadius,
    shapePath,
    starPath,
  } from './draw';

  interface Props {
    /** 演示 id，也是資料檔名（../data/<name>.json）。 */
    name: string;
  }

  let { name }: Props = $props();

  // ---- 資料 ----

  /** gen-demos.mjs 寫出的精簡格式：修飾只留 true、動作段取樣存成 [秒, x, y]。 */
  interface RawSegment extends Omit<MotionSegment, 'samples'> {
    samples: [number, number, number][];
  }
  interface RawNote extends Omit<Note, 'modifiers' | 'sourceSpan'> {
    modifiers: Partial<Note['modifiers']>;
  }
  interface DemoFile {
    id: string;
    source: string;
    status: string;
    chart: { durationSeconds: number; notes: RawNote[]; paths: SlidePath[]; touchSensors: TouchSensor[] } | null;
    solution: {
      assignments: Assignment[];
      handovers: Handover[];
      palmPlacements: PalmPlacement[];
      leftSegments: RawSegment[];
      rightSegments: RawSegment[];
      warnings: string[];
    } | null;
  }

  interface Demo {
    source: string;
    notes: Note[];
    noteById: Map<string, Note>;
    pathById: Map<string, SlidePath>;
    sensors: TouchSensor[];
    usedSensorKeys: Set<string>;
    fireworkNotes: Note[];
    assignmentsByNote: Map<string, Assignment[]>;
    handoversByNote: Map<string, Handover[]>;
    handovers: Handover[];
    palms: PalmPlacement[];
    left: MotionSegment[];
    right: MotionSegment[];
    leftStarts: number[];
    rightStarts: number[];
    leftTrack: Track;
    rightTrack: Track;
    start: number;
    end: number;
  }

  const files = import.meta.glob<DemoFile>('../data/*.json', { import: 'default' });

  const NO_MODIFIERS: Note['modifiers'] = {
    breakNote: false,
    ex: false,
    star: false,
    spinStar: false,
    fireworks: false,
    breakSlide: false,
    exSlide: false,
  };

  function expandSegments(list: RawSegment[]): MotionSegment[] {
    return list.map((segment) => ({
      ...segment,
      samples: segment.samples.map(([timeSeconds, x, y]) => ({ timeSeconds, x, y })),
    }));
  }

  function groupBy<T extends { noteId: string }>(list: T[]): Map<string, T[]> {
    const map = new Map<string, T[]>();
    for (const item of list) {
      const bucket = map.get(item.noteId);
      if (bucket) bucket.push(item);
      else map.set(item.noteId, [item]);
    }
    return map;
  }

  /** 播完後多留一點，讓最後一顆音符淡出（DiscStage 的淡出是 0.4 秒）。 */
  const TAIL_SECONDS = 0.5;

  function prepare(file: DemoFile): Demo {
    const chart = file.chart!;
    const solution = file.solution!;
    const notes: Note[] = chart.notes.map((note) => ({
      ...note,
      modifiers: { ...NO_MODIFIERS, ...note.modifiers },
      sourceSpan: { start: 0, end: 0, line: 0, column: 0 },
    }));
    const left = expandSegments(solution.leftSegments);
    const right = expandSegments(solution.rightSegments);
    // 範圍同 App 的 session.computeBounds：含第一顆前的預備動作（負時間）與最後一個動作。
    let start = 0;
    let end = chart.durationSeconds;
    for (const note of notes) {
      start = Math.min(start, note.timeSeconds);
      end = Math.max(end, note.endSeconds);
    }
    for (const segments of [left, right]) {
      if (segments.length === 0) continue;
      start = Math.min(start, segments[0].startSeconds);
      end = Math.max(end, segments[segments.length - 1].endSeconds);
    }
    const touchNotes = notes.filter(isTouchNote);
    return {
      source: file.source,
      notes,
      noteById: new Map(notes.map((note) => [note.id, note])),
      pathById: new Map(chart.paths.map((path) => [path.id, path])),
      sensors: touchNotes.length > 0 ? chart.touchSensors : [],
      usedSensorKeys: new Set(touchNotes.map((note) => sensorKey(note.touchArea, note.button))),
      fireworkNotes: notes.filter(isFireworkTouch),
      assignmentsByNote: groupBy(solution.assignments),
      handoversByNote: groupBy(solution.handovers),
      handovers: solution.handovers,
      palms: solution.palmPlacements,
      left,
      right,
      leftStarts: left.map((segment) => segment.startSeconds),
      rightStarts: right.map((segment) => segment.startSeconds),
      leftTrack: buildTrack(left),
      rightTrack: buildTrack(right),
      start,
      end: end + TAIL_SECONDS,
    };
  }

  let demo = $state<Demo | null>(null);
  let source = $state('');
  let error = $state<string | null>(null);

  $effect(() => {
    const loader = files[`../data/${name}.json`];
    demo = null;
    error = null;
    if (!loader) {
      error = `找不到這段演示的資料（${name}），請重新產生演示資料。`;
      return;
    }
    let cancelled = false;
    loader().then(
      (file) => {
        if (cancelled) return;
        source = file.source;
        if (file.status !== 'ok' || !file.chart || !file.solution) {
          error = `核心沒有算出可播放的方案（${file.status}），這段演示暫時無法播放。`;
          return;
        }
        demo = prepare(file);
        time = initialTime(demo);
      },
      () => {
        if (!cancelled) error = '演示資料載入失敗，請重新整理頁面。';
      },
    );
    return () => {
      cancelled = true;
    };
  });

  // ---- 播放時鐘 ----

  /**
   * 減少動態時停在這一格（秒）；沒列的演示取整段中間。
   * 選的是最能看出該頁主題的瞬間，例如兩手在中心碰頭、手掌覆蓋的期間。
   */
  const STILL: Record<string, number> = {
    'tap-alternate': 1.52,
    'hold-other-hand': 1.01,
    'slide-basic': 1.8,
    swap: 0.85,
    cross: 0.82,
    'slide-long': 3.3,
    jack: 0.09,
    abab: 0.1,
    'abab-far': 0.13,
    stair: 0.31,
    palm: 0.5,
    'palm-hold': 3.6,
    wifi: 1.0,
    'wifi-one-hand': 0.76,
    'slide-palm': 0.63,
    'touch-kinds': 2.62,
    shapes: 5.7,
  };

  const RATES = [0.25, 0.5, 1] as const;
  /** 播完停在最後一格多久再從頭循環。 */
  const LOOP_PAUSE_MS = 800;

  let time = $state(0);
  let playing = $state(false);
  let rate = $state<number>(0.5);
  /** 使用者自己按了暫停：之後捲回可視範圍也不自動播放。 */
  let userPaused = $state(false);
  let inView = $state(false);
  let pageVisible = $state(true);

  function initialTime(data: Demo): number {
    if (reducedMotion()) {
      const still = STILL[name] ?? (data.start + data.end) / 2;
      return Math.min(Math.max(still, data.start), data.end);
    }
    return data.start;
  }

  function clampTime(value: number): number {
    if (!demo) return value;
    return Math.min(Math.max(value, demo.start), demo.end);
  }

  // 自動播放：資料就緒、在可視範圍、分頁在前景、沒有被使用者暫停，而且沒有要求減少動態。
  $effect(() => {
    if (!demo) return;
    const auto = inView && pageVisible && !userPaused && !reducedMotion();
    if (auto) playing = true;
    else if (!inView || !pageVisible) playing = false;
  });

  let frame = 0;
  let last = 0;
  let holdUntil = 0;

  function step(now: number) {
    frame = 0;
    if (!playing || !demo) return;
    const dt = last ? Math.min((now - last) / 1000, 0.1) : 0;
    last = now;
    if (holdUntil) {
      if (now >= holdUntil) {
        holdUntil = 0;
        time = demo.start;
      }
    } else {
      const next = time + dt * rate;
      if (next >= demo.end) {
        time = demo.end;
        holdUntil = now + LOOP_PAUSE_MS;
      } else {
        time = next;
      }
    }
    frame = requestAnimationFrame(step);
  }

  $effect(() => {
    if (!playing || !demo) return;
    last = 0;
    holdUntil = 0;
    frame = requestAnimationFrame(step);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
    };
  });

  /** 播放／暫停切換時，盤面中央閃現目前狀態的圖示（同 DiscStage）；只有使用者操作才閃。 */
  let flash = $state<{ id: number; playing: boolean } | null>(null);

  function setPlaying(next: boolean) {
    if (!demo) return;
    if (next && time >= demo.end) time = demo.start;
    playing = next;
    userPaused = !next;
    if (!reducedMotion()) flash = { id: (flash?.id ?? 0) + 1, playing: next };
  }

  function toggle() {
    setPlaying(!playing);
  }

  function replay() {
    if (!demo) return;
    time = demo.start;
    holdUntil = 0;
    setPlaying(true);
  }

  function seek(value: number) {
    time = clampTime(value);
    holdUntil = 0;
  }

  function onSeekKey(event: KeyboardEvent) {
    if (!demo) return;
    const delta = event.shiftKey ? 1 : 0.1;
    let next: number | null = null;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') next = time - delta;
    else if (event.key === 'ArrowRight' || event.key === 'ArrowUp') next = time + delta;
    else if (event.key === 'Home') next = demo.start;
    else if (event.key === 'End') next = demo.end;
    if (next === null) return;
    event.preventDefault();
    seek(next);
  }

  let root = $state<HTMLElement | null>(null);

  $effect(() => {
    if (!root || typeof IntersectionObserver === 'undefined') {
      inView = true;
      return;
    }
    const observer = new IntersectionObserver(
      (records) => {
        for (const record of records) inView = record.isIntersecting;
      },
      { threshold: 0.35 },
    );
    observer.observe(root);
    return () => observer.disconnect();
  });

  $effect(() => {
    const update = () => (pageVisible = document.visibilityState !== 'hidden');
    update();
    document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  });

  // ---- 盤面（照 DiscStage，資料換成 demo） ----

  /** App 的顯示預設值（view.svelte.ts）：飛入 0.85 秒、最近軌跡 1.2 秒。 */
  const APPROACH_SECONDS = 0.85;
  const TRAIL_SECONDS = 1.2;
  const SPARK_ANGLES = sparkAngles();

  const visuals = $derived(
    demo ? demo.notes.map((note) => noteVisual(note, time, 'window', true, APPROACH_SECONDS)) : [],
  );

  interface SensorMark {
    key: string;
    label: string;
    used: boolean;
    outline: string;
    at: Point;
  }

  // Touch 落點參考：有 Touch 的演示自動顯示（App「顯示」的「自動」），座標取 chart.touchSensors。
  const sensorMarks = $derived.by<SensorMark[]>(() => {
    if (!demo) return [];
    const used = demo.usedSensorKeys;
    return demo.sensors.map((sensor) => {
      const area = String(sensor.area);
      const key = sensorKey(area, sensor.index);
      return {
        key,
        label: touchName(area, sensor.index),
        used: used.has(key),
        outline: shapePath(area, sensor.position, sensorRadius(area), radialAngle(sensor.position)),
        at: px(sensor.position),
      };
    });
  });

  /** 已有 Touch 音符停在上面的落點不再重畫參考標記。 */
  const coveredSensorKeys = $derived.by(() => {
    const keys = new Set<string>();
    for (const visual of visuals) {
      if (visual.visible && isTouchNote(visual.note)) keys.add(sensorKey(visual.note.touchArea, visual.note.button));
    }
    return keys;
  });

  const bursts = $derived(demo ? fireworkBursts(demo.fireworkNotes, time) : []);
  const palms = $derived(demo ? activePalms(demo.palms, time) : []);

  const leftState = $derived<HandState | null>(
    demo ? sampleWithStarts(demo.left, demo.leftStarts, time) : null,
  );
  const rightState = $derived<HandState | null>(
    demo ? sampleWithStarts(demo.right, demo.rightStarts, time) : null,
  );

  const leftTrail = $derived(demo ? linePath(trackSlice(demo.leftTrack, time - TRAIL_SECONDS, time)) : '');
  const rightTrail = $derived(demo ? linePath(trackSlice(demo.rightTrack, time - TRAIL_SECONDS, time)) : '');

  interface HandoverMarker {
    key: string;
    point: Point;
    label: string;
    active: boolean;
  }

  const handoverMarkers = $derived.by<HandoverMarker[]>(() => {
    if (!demo) return [];
    const data = demo;
    // 碰頭互換兩條 Slide 各記一筆，位置與時間相同，盤面上只畫一個標記。
    const shown = data.handovers.filter(
      (handover, index) =>
        !handover.swap ||
        data.handovers.findIndex((other) => other.swap && other.startSeconds === handover.startSeconds) === index,
    );
    return shown.map((handover, index) => {
      const source = handover.from === 'L' ? data.left : data.right;
      const starts = handover.from === 'L' ? data.leftStarts : data.rightStarts;
      const state = sampleWithStarts(source, starts, handover.startSeconds);
      return {
        key: `${handover.noteId}-${index}`,
        point: state?.point ?? { x: 0, y: 0 },
        label: handover.swap ? '兩手互換' : `換手 ${handover.from}→${handover.to}`,
        active: time >= handover.startSeconds - 0.15 && time <= handover.endSeconds + 0.35,
      };
    });
  });

  /** 音符旁的指派標籤，規則同 DiscStage.tagFor。 */
  function tagFor(noteId: string): { text: string; hand: Hand; handover: boolean; dual: boolean } | null {
    if (!demo) return null;
    const list = demo.assignmentsByNote.get(noteId) ?? [];
    if (list.length === 0) return null;
    const handovers = demo.handoversByNote.get(noteId) ?? [];
    if (handovers.length > 0) {
      const first = handovers[0];
      const last = handovers[handovers.length - 1];
      return { text: `${first.from}→${last.to}`, hand: first.from, handover: true, dual: false };
    }
    if (simultaneousSlideHands(list).length > 0) {
      return { text: 'L+R滑', hand: 'L', handover: false, dual: true };
    }
    if (list.every((item) => item.part === 'group')) {
      return { text: `${list[0].hand}連帶`, hand: list[0].hand, handover: false, dual: false };
    }
    const head = list.find((item) => item.part === 'head' || item.part === 'contact');
    const slide = list.find((item) => item.part === 'slide');
    if (head && slide && head.hand !== slide.hand) {
      return { text: `${head.hand}起${slide.hand}滑`, hand: head.hand, handover: false, dual: false };
    }
    const hands = [...new Set(list.map((item) => item.hand))];
    return { text: hands.join('/'), hand: hands[0], handover: false, dual: false };
  }

  /** 指派標籤的位置（同 DiscStage.tagAnchor）。 */
  function tagAnchor(note: Note, factor: number): Point {
    if (note.kind !== 'touch' && note.kind !== 'touchHold') {
      return scalePoint(note.position, Math.max(factor - 0.17, 0.1));
    }
    const gap = touchOuterRadius(touchRadius(note.touchArea), note.kind === 'touchHold') + 0.075;
    const angle = radialAngle(note.position);
    const away = Math.hypot(note.position.x, note.position.y) < 0.6 ? 1 : -1;
    return {
      x: note.position.x + Math.cos(angle) * gap * away,
      y: note.position.y + Math.sin(angle) * gap * away,
    };
  }

  const handMarkerRadius = len(0.075);
  const DIAMOND = diamondPath();
  const CHEVRON = chevronPath();

  // ---- 時間軸標記（同 App 播放列：音符刻度、手掌與換手區間） ----

  const span = $derived(demo ? Math.max(demo.end - demo.start, 1e-6) : 1);

  function ratio(value: number): number {
    return demo ? ((value - demo.start) / span) * 100 : 0;
  }

  const ticks = $derived.by(() => {
    if (!demo) return [];
    const data = demo;
    return data.notes.map((note) => {
      const hands = [...new Set((data.assignmentsByNote.get(note.id) ?? []).map((item) => item.hand))];
      return {
        id: note.id,
        left: ratio(note.timeSeconds),
        hand: hands.length === 1 ? hands[0] : hands.length > 1 ? 'B' : 'N',
        kind: note.kind,
      };
    });
  });

  const palmBands = $derived(
    demo
      ? demo.palms.map((palm, index) => ({
          id: `${palm.hand}-${index}`,
          hand: palm.hand,
          left: ratio(palm.startSeconds),
          width: Math.max(0.6, ratio(palm.endSeconds) - ratio(palm.startSeconds)),
        }))
      : [],
  );

  const handoverBands = $derived(
    demo
      ? demo.handovers.map((handover, index) => ({
          id: `${handover.noteId}-${index}`,
          left: ratio(handover.startSeconds),
          width: Math.max(0.6, ratio(handover.endSeconds) - ratio(handover.startSeconds)),
        }))
      : [],
  );

  function handText(state: HandState | null): string {
    return state ? (MODE_LABEL[state.mode] ?? state.mode) : '';
  }

  const ariaTime = $derived(`${formatClock(time)}，左手${handText(leftState)}，右手${handText(rightState)}`);
</script>

<div class="disc-demo" bind:this={root}>
  <div class="code-block demo-source">
    <code class="source-text">{source}</code>
    <button type="button" class="btn btn--round code-copy" aria-label="複製" title="複製" disabled={!source}>
      <span class="icon-copy"><Icon name="copy" /></span>
      <span class="icon-check"><Icon name="check" /></span>
    </button>
  </div>

  <div class="stage-box">
    {#if error}
      <div class="mm-frame stage-error">
        <p class="alert alert--error" role="alert">{error}</p>
      </div>
    {:else}
      <!-- 點盤面切換播放／暫停；鍵盤用下方的播放鈕。 -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="mm-frame stage-hit" class:is-ready={demo !== null} onclick={toggle}>
        <svg
          class="mm-disc"
          viewBox={VIEW_BOX}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={`maimai 盤面：音符與左右手軌跡。${demo ? ariaTime : '載入中'}`}
        >
          {#if !demo}
            <!-- 載入中：黑底骨架，固定長寬比，不跳動。 -->
            <circle class="skeleton-ring" cx={CENTER.x} cy={CENTER.y} r={DISC_RADIUS} />
            <circle class="skeleton-ring skeleton-ring--inner" cx={CENTER.x} cy={CENTER.y} r={DISC_RADIUS * 0.52} />
            {#each { length: 8 } as _, index (index)}
              {@const angle = ((index * 45 - 67.5) * Math.PI) / 180}
              <circle
                class="skeleton-dot"
                cx={CENTER.x + DISC_RADIUS * Math.cos(angle)}
                cy={CENTER.y + DISC_RADIUS * Math.sin(angle)}
                r={DISC_RADIUS * 0.045}
              />
            {/each}
          {:else}
            {@const data = demo}
            <DiscBoard center={CENTER} radius={DISC_RADIUS} />

            {#if sensorMarks.length > 0}
              <g class="sensors" aria-hidden="true">
                {#each sensorMarks as mark (mark.key)}
                  {#if !coveredSensorKeys.has(mark.key)}
                    <g class="sensor" class:is-used={mark.used}>
                      <path class="sensor-outline" d={mark.outline} />
                      <text class="sensor-label" x={mark.at.x} y={mark.at.y} text-anchor="middle" dominant-baseline="central">
                        {mark.label}
                      </text>
                    </g>
                  {/if}
                {/each}
              </g>
            {/if}

            {#if palms.length > 0}
              <!-- 手掌覆蓋區：掌心、半徑、覆蓋名單與起止時間全部取自核心的 palmPlacements。 -->
              <g class="palms" aria-hidden="true">
                {#each palms as palm (`${palm.hand}-${palm.startSeconds}-${palm.coveredNoteIds[0] ?? ''}`)}
                  {@const at = px(palm.center)}
                  {@const label = palmLabel(palm)}
                  {@const targets = coveredTargets(palm, data.noteById)}
                  <g class="palm" class:is-left={palm.hand === 'L'} class:is-right={palm.hand === 'R'}>
                    <circle class="palm-edge" cx={at.x} cy={at.y} r={len(palm.radius)} />
                    <path class="palm-center" d={palmCrossPath(at)} />
                    {#each palmCovers(palm, data.noteById) as cover (cover.noteId)}
                      {@const spot = px(cover.at)}
                      <circle class="palm-cover" cx={spot.x} cy={spot.y} r={len(0.165)} />
                    {/each}
                    <text class="palm-title" x={label.x} y={label.title} text-anchor="middle">
                      {palm.hand} 手掌・一次覆蓋 {targets.length} 個 Touch
                    </text>
                    <text class="palm-list" x={label.x} y={label.list} text-anchor="middle">
                      {targets.join('・')}
                    </text>
                  </g>
                {/each}
              </g>
            {/if}

            <g class="buttons" aria-hidden="true">
              {#each BUTTONS as button (button)}
                {@const outer = px(buttonPoint(button))}
                {@const inner = px(scalePoint(buttonPoint(button), 0.86))}
                <circle cx={outer.x} cy={outer.y} r={len(0.045)} />
                <text x={inner.x} y={inner.y} dominant-baseline="central" text-anchor="middle">{button}</text>
              {/each}
            </g>

            <!-- Slide 軌道：直接畫核心輸出的路徑取樣點 -->
            <g class="paths" aria-hidden="true">
              {#each visuals as visual (visual.note.id)}
                {#if visual.visible && visual.note.pathId}
                  {@const path = data.pathById.get(visual.note.pathId)}
                  {#if path}
                    {@const arrows = chevrons(
                      path.samples,
                      Math.min(26, Math.max(3, Math.round(pathLength(path.samples) / 0.16))),
                    )}
                    <g class="slide-path" class:is-break={visual.note.modifiers.breakSlide} opacity={visual.opacity}>
                      {#each path.branches as branch, index (index)}
                        <path class="slide-path-branch" d={linePath(branch)} />
                      {/each}
                      <path class="slide-path-base" d={linePath(path.samples)} />
                      {#if visual.slideU > 0}
                        {#each path.branches as branch, index (index)}
                          <path class="slide-path-done" d={partialPath(branch, visual.slideU)} />
                        {/each}
                        <path class="slide-path-done" d={partialPath(path.samples, visual.slideU)} />
                      {/if}
                      {#each arrows as arrow, index (index)}
                        {@const point = px(arrow)}
                        <path
                          class="chevron"
                          class:is-passed={arrow.u <= visual.slideU}
                          d={CHEVRON}
                          transform={`translate(${point.x} ${point.y}) rotate(${arrow.angle})`}
                        />
                      {/each}
                    </g>
                  {/if}
                {/if}
              {/each}
            </g>

            <g class="notes" aria-hidden="true">
              {#each visuals as visual (visual.note.id)}
                {@const note = visual.note}
                {#if visual.visible}
                  {@const factor = approachScale(visual)}
                  {@const center2 = px(scalePoint(note.position, factor))}
                  {@const tag = tagFor(note.id)}
                  <g
                    class="note"
                    class:is-active={visual.phase === 'active'}
                    class:is-break={note.modifiers.breakNote}
                    class:is-ex={note.modifiers.ex}
                    opacity={visual.opacity}
                  >
                    {#if note.kind === 'touch' || note.kind === 'touchHold'}
                      {@const at = note.position}
                      {@const mark = px(at)}
                      {@const r = touchRadius(note.touchArea)}
                      {@const isHold = note.kind === 'touchHold'}
                      {#if note.modifiers.fireworks}
                        <circle class="touch-spark-mark" cx={mark.x} cy={mark.y} r={len(touchSparkRadius(r, isHold))} />
                      {/if}
                      {#if isHold}
                        {@const ring = len(touchHoldRingRadius(r))}
                        <circle class="touch-hold-track" cx={mark.x} cy={mark.y} r={ring} />
                        <circle
                          class="touch-hold-remain"
                          cx={mark.x}
                          cy={mark.y}
                          r={ring}
                          stroke-dasharray={2 * Math.PI * ring}
                          stroke-dashoffset={2 * Math.PI * ring * visual.progress}
                          transform={`rotate(-90 ${mark.x} ${mark.y})`}
                        />
                        <path class="touch-hold-frame" d={closedPath(touchHoldFrame(at, r, TOUCH_ICON_ANGLE))} />
                      {/if}
                      {#each touchPetals(at, r, TOUCH_ICON_ANGLE, touchGather(visual), isHold) as petal, index (index)}
                        <path class="touch-petal" d={closedPath(petal)} />
                      {/each}
                      <text
                        class="touch-label"
                        class:is-center={note.touchArea === 'C'}
                        x={mark.x}
                        y={mark.y}
                        text-anchor="middle"
                        dominant-baseline="central"
                      >
                        {touchName(note.touchArea, note.button)}
                      </text>
                    {:else if note.kind === 'hold'}
                      {@const duration = Math.max(note.endSeconds - note.timeSeconds, 0)}
                      {@const full = holdLength(duration)}
                      <path
                        class="hold-bar"
                        d={closedPath(
                          holdOutline(note.position, factor, visual.phase === 'upcoming' ? full : full * (1 - visual.progress)),
                        )}
                      />
                    {:else if note.kind === 'slide'}
                      {#if note.hasHead}
                        <path class="note-star" d={starPath(scalePoint(note.position, factor), 0.1, 0.045)} />
                      {/if}
                      {#if visual.slideU > 0 && visual.slideU < 1 && note.pathId}
                        {@const path = data.pathById.get(note.pathId)}
                        {#if path}
                          {#each path.branches as branch, index (index)}
                            <path class="note-star is-moving" d={starPath(pointOnSamples(branch, visual.slideU), 0.085, 0.038)} />
                          {/each}
                          <path class="note-star is-moving" d={starPath(pointOnSamples(path.samples, visual.slideU), 0.085, 0.038)} />
                        {/if}
                      {/if}
                    {:else}
                      <circle class="note-ring" cx={center2.x} cy={center2.y} r={len(0.085)} />
                      <circle class="note-core" cx={center2.x} cy={center2.y} r={len(0.048)} />
                    {/if}

                    {#if tag}
                      {@const anchor = px(tagAnchor(note, factor))}
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
                        {#if tag.dual}
                          <tspan class="tag-left">L</tspan><tspan class="tag-plain">+</tspan><tspan class="tag-right"
                            >R</tspan
                          ><tspan class="tag-plain">滑</tspan>
                        {:else}
                          {tag.text}
                        {/if}
                      </text>
                    {/if}
                  </g>
                {/if}
              {/each}
            </g>

            {#if bursts.length > 0}
              <g class="fireworks" aria-hidden="true">
                {#each bursts as burst (burst.note.id)}
                  {@const shape = fireworkShape(burst.progress)}
                  {@const at = px(burst.note.position)}
                  <g class="firework">
                    <circle
                      class="firework-ring"
                      cx={at.x}
                      cy={at.y}
                      r={len(shape.ringRadius)}
                      stroke-width={shape.ringWidth}
                      stroke-opacity={shape.fade}
                    />
                    {#each SPARK_ANGLES as angle, index (index)}
                      <line
                        class="firework-spark"
                        x1={at.x + Math.cos(angle) * len(shape.sparkInner)}
                        y1={at.y + Math.sin(angle) * len(shape.sparkInner)}
                        x2={at.x + Math.cos(angle) * len(shape.sparkOuter)}
                        y2={at.y + Math.sin(angle) * len(shape.sparkOuter)}
                        stroke-opacity={shape.fade}
                      />
                    {/each}
                    {#if shape.coreRadius > 0}
                      <circle class="firework-core" cx={at.x} cy={at.y} r={len(shape.coreRadius)} />
                    {/if}
                  </g>
                {/each}
              </g>
            {/if}

            <g class="tracks" aria-hidden="true">
              <path class="trail trail--left" d={leftTrail} />
              <path class="trail trail--right" d={rightTrail} />
            </g>

            <g class="handovers" aria-hidden="true">
              {#each handoverMarkers as marker (marker.key)}
                {#if marker.active}
                  {@const point = px(marker.point)}
                  <g class="handover is-active">
                    <path class="handover-diamond" d={DIAMOND} transform={`translate(${point.x} ${point.y})`} />
                    <text class="handover-label" x={point.x} y={point.y - len(0.11)} text-anchor="middle">
                      {marker.label}
                    </text>
                  </g>
                {/if}
              {/each}
            </g>

            <g class="hands" aria-hidden="true">
              {#each [{ hand: 'R', state: rightState }, { hand: 'L', state: leftState }] as item (item.hand)}
                {#if item.state}
                  {@const point = px(item.state.point)}
                  <g
                    class="hand"
                    class:hand--left={item.hand === 'L'}
                    class:hand--right={item.hand === 'R'}
                    class:is-contact={item.state.contacting}
                  >
                    <circle cx={point.x} cy={point.y} r={handMarkerRadius} />
                    {#if item.state.contacting}
                      <circle class="contact-ring" cx={point.x} cy={point.y} r={len(0.115)} />
                    {/if}
                    <text x={point.x} y={point.y} text-anchor="middle" dominant-baseline="central">{item.hand}</text>
                    <text class="hand-mode" x={point.x} y={point.y + len(0.17)} text-anchor="middle">
                      {handText(item.state)}
                    </text>
                  </g>
                {/if}
              {/each}
            </g>
          {/if}
        </svg>
        {#if flash}
          {#key flash.id}
            <div
              class="play-flash"
              class:is-playing={flash.playing}
              aria-hidden="true"
              onanimationend={(event) => {
                if (event.animationName.includes('flash-fade')) flash = null;
              }}
            >
              <span class="flash-icon"><span class="flash-a"></span><span class="flash-b"></span></span>
            </div>
          {/key}
        {/if}
      </div>
    {/if}
  </div>

  <div class="controls" class:is-playing={playing}>
    <div class="track">
      <div class="track-bg" aria-hidden="true"></div>
      <div class="marks" aria-hidden="true">
        {#if demo && demo.start < 0}
          <div class="zero-mark" style={`left:${ratio(0)}%`}></div>
        {/if}
        {#each ticks as item (item.id)}
          <div
            class="tick"
            class:tick--left={item.hand === 'L'}
            class:tick--right={item.hand === 'R'}
            class:tick--both={item.hand === 'B'}
            class:tick--slide={item.kind === 'slide'}
            class:tick--hold={item.kind === 'hold' || item.kind === 'touchHold'}
            class:tick--touch={item.kind === 'touch' || item.kind === 'touchHold'}
            style={`left:${item.left}%`}
          ></div>
        {/each}
        {#each palmBands as band (band.id)}
          <div class="palm-band" class:palm-band--left={band.hand === 'L'} style={`left:${band.left}%;width:${band.width}%`}></div>
        {/each}
        {#each handoverBands as band (band.id)}
          <div class="handover-band" style={`left:${band.left}%;width:${band.width}%`}></div>
        {/each}
        {#if demo}
          <div class="playhead" style={`left:${ratio(time)}%`}></div>
        {/if}
      </div>
      <input
        class="seek"
        type="range"
        min={demo?.start ?? 0}
        max={demo?.end ?? 1}
        step="0.001"
        value={time}
        disabled={!demo}
        aria-label="播放時間（秒）"
        aria-valuetext={formatClock(time)}
        oninput={(event) => seek(Number(event.currentTarget.value))}
        onkeydown={onSeekKey}
      />
    </div>

    <div class="buttons-row">
      <button
        type="button"
        class="btn btn--round play-btn"
        class:is-playing={playing}
        onclick={toggle}
        disabled={!demo}
        aria-label={playing ? '暫停' : '播放'}
        title={playing ? '暫停' : '播放'}
      >
        <span class="morph" aria-hidden="true"><span class="morph-a"></span><span class="morph-b"></span></span>
      </button>
      <button type="button" class="btn btn--round" onclick={replay} disabled={!demo} aria-label="重播" title="重播">
        <Icon name="rotate-ccw" />
      </button>
      <span class="clock mono">{formatClock(time)}</span>
      <span class="total mono muted">／ {formatClock(demo?.end ?? 0)}</span>
      <span class="spacer"></span>
      <div class="seg seg--bare seg--mono seg--fit rates" role="group" aria-label="播放倍率" use:segmented>
        {#each RATES as value (value)}
          <button type="button" class="seg-btn" aria-pressed={rate === value} onclick={() => (rate = value)}>{value}×</button>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .disc-demo {
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 100%;
    max-width: 480px;
    margin: 0 auto;
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
  }

  /* simai 原文：一列等寬字，可橫向捲動；複製鈕用內容頁共用的 .code-copy（版面統一處理點擊）。 */
  .demo-source {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-4);
  }

  .source-text {
    flex: 1 1 auto;
    min-width: 0;
    overflow-x: auto;
    padding: var(--space-1) 0;
    border-radius: 0;
    background: none;
    white-space: pre;
    overflow-wrap: normal;
    font-family: var(--font-mono);
    font-size: var(--fs-md);
    line-height: 1.7;
    color: var(--c-text-strong);
    scrollbar-width: thin;
    scrollbar-color: var(--c-border-strong) transparent;
  }

  /* 盤面：固定正方形，資料載入前後高度不變。 */
  .stage-box {
    position: relative;
    width: 100%;
    aspect-ratio: 1;
  }

  .stage-hit.is-ready {
    cursor: pointer;
  }

  .stage-error {
    display: grid;
    place-items: center;
    padding: var(--space-4);
  }

  .stage-error .alert {
    margin: 0;
  }

  .play-flash {
    position: absolute;
    left: 50%;
    top: 50%;
    z-index: 4;
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    margin: -32px 0 0 -32px;
    border-radius: var(--radius-pill);
    background: #14181e;
    color: #ffffff;
    pointer-events: none;
    animation:
      flash-fade 420ms ease-out 80ms both,
      flash-grow 500ms var(--ease-out) both;
  }

  .flash-icon {
    position: relative;
    width: 22px;
    height: 22px;
  }

  .flash-a,
  .flash-b {
    position: absolute;
    inset: 0;
    background: currentColor;
  }

  .flash-a {
    clip-path: polygon(8% 0%, 54% 25%, 54% 75%, 8% 100%);
  }

  .flash-b {
    clip-path: polygon(54% 25%, 100% 50%, 100% 50%, 54% 75%);
  }

  .play-flash.is-playing .flash-a {
    clip-path: polygon(8% 0%, 40% 0%, 40% 100%, 8% 100%);
  }

  .play-flash.is-playing .flash-b {
    clip-path: polygon(64% 0%, 96% 0%, 96% 100%, 64% 100%);
  }

  @keyframes flash-fade {
    from {
      opacity: 1;
    }
    to {
      opacity: 0;
    }
  }

  @keyframes flash-grow {
    from {
      transform: scale(0.9);
    }
    to {
      transform: scale(1.25);
    }
  }

  /* ---- 播放列（照 App 的 Transport） ---- */

  .controls {
    --thumb: 14px;
    --inset: calc(var(--thumb) / 2);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .buttons-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .spacer {
    flex: 1 1 auto;
  }

  .clock {
    min-width: 9ch;
    font-size: var(--fs-lg);
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--c-text-strong);
  }

  .total {
    font-size: var(--fs-sm);
    white-space: nowrap;
  }

  /* 窄的時候只留目前時間，倍率不換行。 */
  @container (width < 400px) {
    .total {
      display: none;
    }
  }

  /* 320px 寬的手機：觸控按鈕 44px 時一列放不下目前時間，收起來（時間軸還在），避免整頁橫向捲動。 */
  @container (width < 340px) {
    .clock {
      display: none;
    }
  }

  .rates {
    flex: none;
  }

  .rates .seg-btn {
    min-width: 44px;
    height: 30px;
  }

  .morph {
    position: relative;
    flex: none;
    width: 12px;
    height: 12px;
  }

  .morph-a,
  .morph-b {
    position: absolute;
    inset: 0;
    background: currentColor;
    transition: clip-path 220ms var(--ease-out);
  }

  .morph-a {
    clip-path: polygon(8% 0%, 54% 25%, 54% 75%, 8% 100%);
  }

  .morph-b {
    clip-path: polygon(54% 25%, 100% 50%, 100% 50%, 54% 75%);
  }

  .play-btn.is-playing .morph-a {
    clip-path: polygon(8% 0%, 40% 0%, 40% 100%, 8% 100%);
  }

  .play-btn.is-playing .morph-b {
    clip-path: polygon(64% 0%, 96% 0%, 96% 100%, 64% 100%);
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
    pointer-events: none;
  }

  .marks {
    position: absolute;
    top: 1px;
    bottom: 1px;
    left: var(--inset);
    right: var(--inset);
    pointer-events: none;
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

  .palm-band {
    position: absolute;
    top: 1px;
    height: 3px;
    min-width: 3px;
    background: var(--c-right);
  }

  .palm-band--left {
    background: var(--c-left);
  }

  .handover-band {
    position: absolute;
    bottom: 3px;
    height: 5px;
    min-width: 3px;
    background: var(--c-accent);
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

  /* 滑桿疊在標記層上，軌道透明，只留下大頭針（同 App 播放列）。 */
  .seek {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    appearance: none;
    -webkit-appearance: none;
    background: transparent;
    cursor: pointer;
    touch-action: none;
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
    margin-top: 0;
    border: 2px solid var(--c-bg);
    border-radius: 50%;
    background: var(--c-text-strong);
    box-shadow: none;
    transform: scale(0.75);
    transition: transform 160ms var(--ease-out);
  }

  .controls.is-playing .seek::-webkit-slider-thumb,
  .seek:active::-webkit-slider-thumb {
    transform: scale(1);
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

  .seek:focus-visible {
    outline: 2px solid var(--c-focus);
    outline-offset: 2px;
    border-radius: var(--radius-sm);
  }

  /* 觸控：時間軸與倍率都放大到 44px。 */
  @media (pointer: coarse) {
    .controls {
      --thumb: 22px;
    }

    .track {
      height: 44px;
    }

    .tick {
      top: 12px;
      height: 12px;
    }

    .tick--slide {
      height: 18px;
    }

    .tick--hold {
      height: 16px;
    }

    .tick--touch {
      top: 18px;
    }

    .rates .seg-btn {
      height: 38px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .morph-a,
    .morph-b,
    .seek::-webkit-slider-thumb {
      transition: none;
    }
  }
</style>
