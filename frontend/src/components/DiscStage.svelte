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
  import {
    approachScale,
    noteVisual,
    pathLength,
    scalePoint,
    touchGather,
    type NoteVisual,
  } from '../lib/notes';
  import { sampleWithStarts, trackSlice, type HandState } from '../lib/motion';
  import { MODE_LABEL, KIND_LABEL, shapeLabel } from '../lib/contract';
  import {
    activePalms,
    coveredTargets,
    palmCovers,
    palmLabelAnchor,
    palmLabelUpward,
  } from '../lib/palm';
  import {
    TOUCH_AREA_PLACE,
    TOUCH_ICON_ANGLE,
    fireworkBursts,
    fireworkShape,
    isTouchNote,
    radialAngle,
    sensorKey,
    sparkAngles,
    touchHoldFrame,
    touchHoldRingRadius,
    touchName,
    touchOuterRadius,
    touchPetals,
    touchPolygon,
    touchRadius,
    touchSparkRadius,
  } from '../lib/touch';
  import { formatClock } from '../lib/format';
  import { session } from '../state/session.svelte';
  import { playback } from '../state/playback.svelte';
  import { view } from '../state/view.svelte';
  import type { Hand, Note, PalmPlacement, PathSample, Point } from '../lib/types';

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

  // Touch 落點參考標記：座標一律取 chart.touchSensors，前端只決定外形大小。
  const showSensors = $derived(
    view.sensorMode === 'all' || (view.sensorMode === 'auto' && session.hasTouchNotes),
  );

  interface SensorMark {
    key: string;
    label: string;
    used: boolean;
    outline: string;
    at: Point;
  }

  /** 參考標記畫得比音符小一級，音符落下時仍是畫面上最亮的一層。 */
  function sensorRadius(area: string): number {
    if (area === 'C') return 0.085;
    if (area === 'B' || area === 'E') return 0.068;
    return 0.075;
  }

  const sensorMarks = $derived.by<SensorMark[]>(() => {
    if (!showSensors) return [];
    return session.touchSensors.map((sensor) => {
      const area = String(sensor.area);
      const key = sensorKey(area, sensor.index);
      return {
        key,
        label: touchName(area, sensor.index),
        used: session.usedSensorKeys.has(key),
        outline: shapePath(area, sensor.position, sensorRadius(area), radialAngle(sensor.position)),
        at: px(sensor.position),
      };
    });
  });

  /**
   * 已經有音符停在上面的落點不再重畫參考標記，避免同一個位置出現兩層字。
   * Touch 現在原地收攏，一出現就壓在落點上，所以未到的音符也要蓋掉標記。
   */
  const coveredSensorKeys = $derived.by(() => {
    const keys = new Set<string>();
    if (!showSensors) return keys;
    for (const visual of visuals) {
      if (!visual.visible) continue;
      if (!isTouchNote(visual.note)) continue;
      keys.add(sensorKey(visual.note.touchArea, visual.note.button));
    }
    return keys;
  });

  const SPARK_ANGLES = sparkAngles();

  /**
   * 目前要畫的煙火。
   * 進度只由 (播放時間 − 判定時間) 換算，沒有自己的計時器：
   * 拖曳 seek 到同一時間得到同一畫面，慢放與循環也自動正確。
   */
  const bursts = $derived.by(() =>
    view.showFireworks && view.showNotes ? fireworkBursts(session.fireworkNotes, time) : [],
  );

  /**
   * 目前要畫的手掌覆蓋區。
   * 掌心、半徑、覆蓋名單與起止時間全部取自 Rust 的 palmPlacements，
   * 前端只挑出播放時間落在區間內的那幾筆，不自行判斷哪些 Touch 能被一掌蓋住。
   */
  const palms = $derived.by(() =>
    view.showPalms ? activePalms(session.palmPlacements, time) : [],
  );

  const PALM_TITLE_SIZE = 27;
  const PALM_LIST_SIZE = 23;

  /**
   * 手掌標籤的兩行基線位置。
   * 方向由掌心決定（palm.ts），這裡再夾回 viewBox 之內，
   * 半徑設得很大、掌心貼著盤面邊緣時文字才不會被裁掉。標籤只是說明，不影響任何判定。
   */
  function palmLabel(placement: PalmPlacement): { x: number; title: number; list: number } {
    const anchor = px(palmLabelAnchor(placement));
    const gap = PALM_TITLE_SIZE + 4;
    let title = palmLabelUpward(placement) ? anchor.y - gap : anchor.y + gap * 0.7;
    let list = title + PALM_LIST_SIZE + 10;
    const top = -MARGIN_TOP + PALM_TITLE_SIZE + 6;
    const bottom = IMAGE_HEIGHT + MARGIN_BOTTOM - 6;
    const down = Math.max(0, top - title);
    title += down;
    list += down;
    const up = Math.max(0, list - bottom);
    title -= up;
    list -= up;
    // 文字置中，兩側各留一個標籤寬度的餘裕。
    const x = Math.min(Math.max(anchor.x, -MARGIN_X + 110), IMAGE_WIDTH + MARGIN_X - 110);
    return { x, title, list };
  }

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
    // Touch Group 連帶判定沒有實際接觸，標籤要和真的按下去的區分開。
    if (list.every((item) => item.part === 'group')) {
      return { text: `${list[0].hand}連帶`, hand: list[0].hand, handover: false };
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

  /** Touch 外形：A／D 四邊、B／E 三角、C 八邊，朝向以落點的半徑方向為基準。 */
  function shapePath(
    area: string | null,
    atPoint: Point,
    radius: number,
    angle: number,
  ): string {
    return `${polylinePath(touchPolygon(area, atPoint, radius, angle), calibration)}Z`;
  }

  /** 盤面座標的封閉多邊形 → 畫面路徑，Touch 的三角形與方框共用。 */
  function closedPath(points: Point[]): string {
    return `${polylinePath(points, calibration)}Z`;
  }

  /** 音符的簡短說明，例如「Tap 3」「Touch B5」「Slide 直線 1→5」。 */
  function noteCaption(note: Note): string {
    const kind = KIND_LABEL[note.kind] ?? note.kind;
    if (note.touchArea) {
      return `${kind} ${touchName(note.touchArea, note.button)}`;
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

  /**
   * Hold 長條的半寬。取 Tap 空心圓的半徑，筆觸與 Tap 完全對齊，
   * 長條與 Tap 一眼看得出是同一家族。
   */
  const HOLD_HALF_WIDTH = 0.085;

  /** 兩端矮三角形的高度。矮，所以只取半寬的一半。 */
  const HOLD_TIP = HOLD_HALF_WIDTH * 0.5;

  /**
   * Hold 長條的輪廓：兩條長邊，兩端各收成一個矮三角形。
   * 三角形的底不畫，所以整條是一圈連續的六邊形空心外框，中間完全挖空。
   * 頭在判定位置，尾往盤面中心延伸，長度代表時值；
   * 判定開始後尾端往頭收，剩下的長度就是剩餘時間。
   */
  function holdOutline(note: Note, visual: NoteVisual): Point[] {
    const factor = approachScale(visual);
    const duration = Math.max(note.endSeconds - note.timeSeconds, 0);
    const full = Math.min(0.62, Math.max(0.14, duration * 0.42));
    const length = visual.phase === 'upcoming' ? full : full * (1 - visual.progress);
    const head = scalePoint(note.position, factor);
    const tail = scalePoint(note.position, Math.max(factor - length, 0.06));
    // 由盤面中心指向鍵位的單位向量；長邊沿它、肩寬沿它的垂直方向。
    const base = Math.hypot(note.position.x, note.position.y) || 1;
    const ux = note.position.x / base;
    const uy = note.position.y / base;
    const w = HOLD_HALF_WIDTH;
    const tip = HOLD_TIP;
    return [
      { x: head.x + ux * tip, y: head.y + uy * tip },
      { x: head.x - uy * w, y: head.y + ux * w },
      { x: tail.x - uy * w, y: tail.y + ux * w },
      { x: tail.x - ux * tip, y: tail.y - uy * tip },
      { x: tail.x + uy * w, y: tail.y - ux * w },
      { x: head.x + uy * w, y: head.y - ux * w },
    ];
  }

  /**
   * 指派標籤的位置。
   * Tap／Hold／Slide 沿半徑往內退一點，跟著飛入一起移動。
   * Touch 原地收攏、外形大小固定，標籤改成貼在外形之外：
   * 內圈（B／E／C）往外放、外圈（A／D）往內放，避開鍵位標記與相鄰落點。
   */
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

  function noteLabel(note: Note, visual: NoteVisual): string {
    const tag = tagFor(note.id);
    const hand = tag ? `，指派 ${tag.text}` : '，此候選沒有指派資料';
    const place = note.touchArea ? `，${TOUCH_AREA_PLACE[note.touchArea] ?? ''}` : '';
    const fireworks = note.modifiers.fireworks ? '，判定時間有煙火' : '';
    return `${noteCaption(note)}${place}，音符 ${note.id}，時間 ${formatClock(
      note.timeSeconds,
    )}${fireworks}${hand}。目前狀態 ${
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

      {#if showSensors}
        <!-- Touch 落點參考：33 個 simai 可指名落點，座標取自 chart.touchSensors。
             這是 Demo 的落點標記，不是實機感應區的精確輪廓。 -->
        <g class="sensors" aria-hidden="true">
          {#each sensorMarks as mark (mark.key)}
            {#if !coveredSensorKeys.has(mark.key)}
              <g class="sensor" class:is-used={mark.used}>
                <path class="sensor-outline" d={mark.outline} />
                {#if view.showSensorLabels}
                  <text
                    class="sensor-label"
                    x={mark.at.x}
                    y={mark.at.y}
                    text-anchor="middle"
                    dominant-baseline="central"
                  >
                    {mark.label}
                  </text>
                {/if}
              </g>
            {/if}
          {/each}
        </g>
      {/if}

      {#if palms.length > 0}
        <!-- 手掌覆蓋區：掌心、半徑、覆蓋名單與起止時間全部取自 Rust 的 palmPlacements。
             畫在音符、軌跡與手標記下層，只有虛線圓與細框，不填色，不遮住音符。
             大圓＋左右手顏色，與灰色小多邊形的 33 個落點參考標記明顯不同。 -->
        <g class="palms" aria-hidden="true">
          {#each palms as palm (`${palm.hand}-${palm.startSeconds}-${palm.coveredNoteIds[0] ?? ''}`)}
            {@const at = px(palm.center)}
            {@const edge = len(palm.radius)}
            {@const tickIn = len(0.13)}
            {@const tickOut = len(0.21)}
            {@const label = palmLabel(palm)}
            {@const targets = coveredTargets(palm, session.noteById)}
            <g class="palm" class:is-left={palm.hand === 'L'} class:is-right={palm.hand === 'R'}>
              <circle class="palm-edge" cx={at.x} cy={at.y} r={edge} />
              <!-- 掌心十字：中間留空，手標記蓋上去之後四個端點仍看得見。 -->
              <path
                class="palm-center"
                d={`M${at.x + tickIn} ${at.y} H${at.x + tickOut} M${at.x - tickIn} ${at.y} H${at.x - tickOut} M${at.x} ${at.y + tickIn} V${at.y + tickOut} M${at.x} ${at.y - tickIn} V${at.y - tickOut}`}
              />
              <!-- 被這一掌蓋住的 Touch：在落點外圍加一圈同色虛線，音符本身仍畫在上層。 -->
              {#each palmCovers(palm, session.noteById) as cover (cover.noteId)}
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
                  <!-- Touch 一律停在 Rust 給的落點上，按壓提示是四片三角形由外往內收攏；
                       收到位的瞬間就是判定時間，不從盤面中心飛出去。 -->
                  {@const at = note.position}
                  {@const mark = px(at)}
                  {@const r = touchRadius(note.touchArea)}
                  {@const isHold = note.kind === 'touchHold'}
                  {#if note.modifiers.fireworks}
                    <!-- 判定前就預告會放煙火，不必等到效果出現 -->
                    <circle
                      class="touch-spark-mark"
                      cx={mark.x}
                      cy={mark.y}
                      r={len(touchSparkRadius(r, isHold))}
                    />
                  {/if}
                  {#if isHold}
                    {@const ring = len(touchHoldRingRadius(r))}
                    <!-- 剩餘時間：方框外面一整圈，從正上方順時針消耗，長度只由播放時間換算 -->
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
                    <!-- 目標方框固定在落點，斜向的三角形往它收 -->
                    <path
                      class="touch-hold-frame"
                      d={closedPath(touchHoldFrame(at, r, TOUCH_ICON_ANGLE))}
                    />
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
                  <!-- 空心長條：兩條長邊＋兩端矮三角形，中間挖空，不畫三角形的底。 -->
                  <path class="hold-bar" d={closedPath(holdOutline(note, visual))} />
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
                    {tag.text}
                  </text>
                {/if}
              </g>
            {/if}
          {/each}
        </g>

        {#if bursts.length > 0}
          <!-- 煙火：在 Rust 落點擴散，進度完全由絕對播放時間決定，
               只在起始判定觸發，Touch Hold 結束不重播。 -->
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
              {#if marker.active}
                {@const point = px(marker.point)}
                <g class="handover is-active">
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
              {/if}
            {/each}
          </g>
        {/if}

        <g class="hands">
          {#if view.showRight && rightState}
            {@const point = px(rightState.point)}
            <g class="hand hand--right" class:is-contact={rightState.contacting}>
              <circle cx={point.x} cy={point.y} r={handMarkerRadius} />
              {#if rightState.contacting}
                <circle class="contact-ring" cx={point.x} cy={point.y} r={len(0.115)} />
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

  /* 空心長條：中間完全挖空，線寬與 Tap 空心圓一致。 */
  .hold-bar {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 7;
    stroke-linejoin: round;
  }

  /* Touch 落點參考標記。沿用鍵位標記的中性色，不新增色相。 */
  .sensor-outline {
    fill: none;
    stroke: #4d5765;
    stroke-width: 2;
    stroke-linejoin: round;
  }

  .sensor-label {
    fill: #7b8595;
    font-family: var(--font-mono);
    font-size: 17px;
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 4px;
  }

  /* 這份譜面真的用到的落點加重一級，密集譜面裡先看到相關的區。 */
  .sensor.is-used .sensor-outline {
    stroke: #8d97a6;
    stroke-width: 3;
  }

  .sensor.is-used .sensor-label {
    fill: #c8d1de;
  }

  /* Touch 按壓提示：四片空心三角形，尖端朝落點。 */
  .touch-petal {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 5;
    stroke-linejoin: round;
  }

  /* Touch Hold 的目標方框，配斜向三角形。 */
  .touch-hold-frame {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 5;
    stroke-linejoin: round;
  }

  .touch-spark-mark {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 3;
    stroke-dasharray: 7 10;
    stroke-linejoin: round;
  }

  /* Touch Hold 剩餘時間環：底環固定一整圈，亮環順時針消耗。 */
  .touch-hold-track {
    fill: none;
    stroke: #3d4551;
    stroke-width: 4;
  }

  .touch-hold-remain {
    fill: none;
    stroke: #eef2f8;
    stroke-width: 8;
    stroke-linecap: butt;
  }

  /* 落點編號放在三角形圍出來的中心，黑色描邊保證壓在任何筆觸上都讀得到。 */
  .touch-label {
    fill: #eef2f8;
    font-family: var(--font-mono);
    font-size: 22px;
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 5px;
  }

  /* C 的外形較大，字也放大一級，中央不會看起來空掉。 */
  .touch-label.is-center {
    font-size: 26px;
  }

  /* 煙火筆觸。stroke-opacity 只是短暫的筆觸淡出，不是半透明底色。 */
  .firework-ring {
    fill: none;
    stroke: var(--c-accent);
    stroke-linejoin: round;
  }

  .firework-spark {
    stroke: var(--c-accent);
    stroke-width: 5;
    stroke-linecap: round;
  }

  .firework-core {
    fill: var(--c-text-strong);
    stroke: var(--c-accent);
    stroke-width: 3;
  }

  /* Break 與 EX 只換筆觸顏色，形狀維持一致，不單靠顏色傳達種類。 */
  .note.is-break .note-ring,
  .note.is-break .note-star,
  .note.is-break .touch-petal,
  .note.is-break .touch-hold-frame,
  .note.is-break .touch-hold-remain,
  .note.is-break .hold-bar {
    stroke: #f0913a;
  }

  .note.is-ex .note-core {
    stroke: var(--c-focus);
    stroke-width: 6;
  }

  .note.is-ex .touch-petal {
    stroke: var(--c-focus);
    stroke-width: 7;
  }

  .note.is-active .note-ring,
  .note.is-active .hold-bar {
    stroke-width: 9;
  }

  .note.is-active .touch-petal,
  .note.is-active .touch-hold-frame {
    stroke-width: 7;
  }

  .note.is-selected .note-ring,
  .note.is-selected .note-star,
  .note.is-selected .touch-petal,
  .note.is-selected .touch-hold-frame {
    stroke: var(--c-accent);
    stroke-width: 9;
  }

  .note:focus-visible {
    outline: none;
  }

  .note:focus-visible .note-ring,
  .note:focus-visible .note-star,
  .note:focus-visible .touch-petal,
  .note:focus-visible .touch-hold-frame,
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

  /* 手掌覆蓋區。只用左右手既有色相的虛線筆觸，不填色、不新增色相，
     和灰色小多邊形的落點參考標記在大小、形狀與顏色上都不同。 */
  .palm-edge {
    fill: none;
    stroke-width: 4;
    stroke-dasharray: 24 14;
  }

  .palm-center {
    fill: none;
    stroke-width: 4;
    stroke-linecap: round;
  }

  .palm-cover {
    fill: none;
    stroke-width: 3;
    stroke-dasharray: 10 8;
  }

  .palm-title,
  .palm-list {
    font-family: var(--font-mono);
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 6px;
  }

  .palm-title {
    font-size: 27px;
  }

  .palm-list {
    font-size: 23px;
  }

  .palm.is-left .palm-edge,
  .palm.is-left .palm-center,
  .palm.is-left .palm-cover {
    stroke: var(--c-left);
  }

  .palm.is-left .palm-title,
  .palm.is-left .palm-list {
    fill: var(--c-left);
  }

  .palm.is-right .palm-edge,
  .palm.is-right .palm-center,
  .palm.is-right .palm-cover {
    stroke: var(--c-right);
  }

  .palm.is-right .palm-title,
  .palm.is-right .palm-list {
    fill: var(--c-right);
  }

  .hand circle {
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

  .hand--right circle {
    stroke: var(--c-right);
  }

  .hand--right text {
    fill: var(--c-right);
  }

  .hand--left.is-contact circle {
    fill: var(--c-left);
  }

  .hand--right.is-contact circle {
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
