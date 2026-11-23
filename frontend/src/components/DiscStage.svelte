<script lang="ts">
  import DiscBoard from './DiscBoard.svelte';
  import {
    BUTTONS,
    IMAGE_HEIGHT,
    IMAGE_WIDTH,
    buttonPoint,
    centerPx,
    chevronFan,
    chevronStrip,
    directionOnSamples,
    eachArcPath,
    pointOnSamples,
    polylinePath,
    radiusPx,
    remainingChevronShadows,
    remainingChevrons,
    toStage,
    toStageLength,
    type ChevronStrip,
  } from '../lib/disc';
  import {
    approachScale,
    eachInfo,
    noteVisual,
    scalePoint,
    simultaneousSlideHands,
    starSpin,
    tailScale,
    touchGather,
    type NoteVisual,
  } from '../lib/notes';
  import {
    EX_GLOW_BLUR_PX,
    EX_GLOW_SPREAD_PX,
    NOTE_DOT,
    NOTE_RADIUS,
    SHADE_FLAT,
    STAR_DOT,
    bandWidth,
    edgeWidth,
    exGlowOffset,
    exGlowWidth,
    holdOutline,
    outlineEdges,
    rimInnerOffset,
    rimOffset,
    ringQuarters,
    starBandPath,
    starFillHole,
    starOutlinePath,
    starFillOuter,
  } from '../lib/noteShape';
  import { hitEffects, hitGlow, hitOpacity, hitStrokes, holdLoopRings } from '../lib/hitEffect';
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
    TOUCH_DOT,
    TOUCH_DOT_RING,
    TOUCH_FAN_HOLE,
    TOUCH_HOLD_BAND,
    fanLoop,
    fireworkBursts,
    fireworkShape,
    isTouchNote,
    radialAngle,
    sensorKey,
    sensorMarkRadius,
    sparkAngles,
    touchFans,
    touchHoldBorder,
    touchHoldBorderFill,
    touchJustBorder,
    touchName,
    touchOuterRadius,
    touchPolygon,
    touchRadius,
    touchSparkRadius,
    type TouchFan,
  } from '../lib/touch';
  import { formatClock } from '../lib/format';
  import { session } from '../state/session.svelte';
  import { playback } from '../state/playback.svelte';
  import { reducedMotion } from '../lib/press';
  import { view } from '../state/view.svelte';
  import type { Hand, Note, PalmPlacement, PathSample, Point, SlidePath } from '../lib/types';

  const time = $derived(playback.time);

  /** 播放／暫停切換時，盤面中央閃現目前狀態的圖示，放大並淡出（Claude Design「互動動畫」2c）。 */
  let flash = $state<{ id: number; playing: boolean } | null>(null);
  let lastPlaying: boolean | null = null;
  $effect(() => {
    const playing = playback.playing;
    if (lastPlaying !== null && playing !== lastPlaying && !reducedMotion()) {
      flash = { id: (flash?.id ?? 0) + 1, playing };
    }
    lastPlaying = playing;
  });
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

  /**
   * Slide 箭頭帶的快取：每條路徑（含 WiFi 側線）的箭頭幾何只算一次，
   * 換譜面或改校準時整個丟掉重算。播放時只依 slideU 取出還沒走過的那段字串。
   */
  const stripCache = $derived.by(() => {
    void calibration;
    void session.pathById;
    return new Map<PathSample[], ChevronStrip>();
  });

  function stripFor(samples: PathSample[]): ChevronStrip {
    let strip = stripCache.get(samples);
    if (!strip) {
      strip = chevronStrip(samples, calibration);
      stripCache.set(samples, strip);
    }
    return strip;
  }

  /** WiFi（剛好兩條側線）畫成一整面扇形，其餘形狀是一條箭頭帶。快取的鍵都是中央那條線。 */
  function isFan(path: SlidePath): boolean {
    return path.branches.length === 2;
  }

  function trackFor(path: SlidePath): ChevronStrip {
    if (!isFan(path)) return stripFor(path.samples);
    let strip = stripCache.get(path.samples);
    if (!strip) {
      strip = chevronFan(path.samples, path.branches[0], path.branches[1], calibration);
      stripCache.set(path.samples, strip);
    }
    return strip;
  }

  /** 遊戲的 each（同時押）：照遊戲上色時換成黃色，另外畫兩顆之間的連線。只看時間，不影響指派。 */
  const each = $derived.by(() => eachInfo(session.notes));

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

  const sensorMarks = $derived.by<SensorMark[]>(() => {
    if (!showSensors) return [];
    return session.touchSensors.map((sensor) => {
      const area = String(sensor.area);
      const key = sensorKey(area, sensor.index);
      return {
        key,
        label: touchName(area, sensor.index),
        used: session.usedSensorKeys.has(key),
        outline: shapePath(area, sensor.position, sensorMarkRadius(area), radialAngle(sensor.position)),
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

  /**
   * 目前要畫的打擊效果（「顯示」的開關，預設打開）。和煙火一樣只由絕對播放時間換算；
   * 系統設成減少動態效果時不畫。
   */
  const hitsOn = $derived(view.hitEffects && view.showNotes && !reducedMotion());
  const hits = $derived.by(() => (hitsOn ? hitEffects(session.notes, time) : []));
  /** 按住 Hold／Touch Hold 的期間，從音符頭一圈一圈往外擴的光圈（同一個「打擊效果」開關）。 */
  const holdRings = $derived.by(() => (hitsOn ? holdLoopRings(calibration, session.notes, time) : []));

  /**
   * 音符本體的不透明度。開著打擊效果時，判定完的 Tap／Hold／Touch／Touch Hold 像遊戲一樣立刻消失，
   * 讓光暈接手；不然慢慢淡出的音符會和光暈疊在一起。沒開打擊效果時照原本的淡出。
   * Slide 不在此列：它的起點打完之後星星還要沿著軌道走。
   */
  const HIT_VANISH_SECONDS = 0.05;
  function noteOpacity(visual: NoteVisual): number {
    if (!hitsOn || visual.phase !== 'done' || visual.note.kind === 'slide') return visual.opacity;
    const end = Math.max(visual.note.endSeconds, visual.note.timeSeconds);
    return visual.opacity * Math.max(0, 1 - (time - end) / HIT_VANISH_SECONDS);
  }

  /**
   * 打擊效果的光暈：整組過一次模糊，疊成「外面一圈寬而淡的光＋裡面一條柔邊的亮線」。
   * 模糊半徑跟著圖形的大小走（`hitGlow`），所以每個效果各有一個濾鏡；
   * 遊戲的圖是整張一起放大，線和光暈跟著變粗，這樣才不會小的時候糊成一團、大的時候又太銳利。
   * 濾鏡 id 每個盤面各自一組，同一頁有兩個盤面時不會互相指到。
   */
  const hitGlowId = `hit-glow-${Math.random().toString(36).slice(2, 8)}`;

  /**
   * EX 音符的光暈：每個盤面一個模糊濾鏡，所有 EX 音符共用（模糊半徑是固定的像素值）。
   * 照遊戲上色時環帶的漸層則是每顆音符各自的（顏色跟著那顆音符的 --nc／--nd），這是它們 id 的前綴。
   */
  const exGlowId = `ex-glow-${Math.random().toString(36).slice(2, 8)}`;
  const shadeId = `shade-${Math.random().toString(36).slice(2, 8)}`;

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

  function tagFor(
    noteId: string,
  ): { text: string; hand: Hand; handover: boolean; dual: boolean } | null {
    const list = session.assignmentsByNote.get(noteId) ?? [];
    if (list.length === 0) return null;
    const handovers = session.handoversByNote.get(noteId) ?? [];
    if (handovers.length > 0) {
      const first = handovers[0];
      const last = handovers[handovers.length - 1];
      return { text: `${first.from}→${last.to}`, hand: first.from, handover: true, dual: false };
    }
    // 兩手同時各滑一段（WiFi 2+1）：標成 L+R 滑，兩個字母各用自己的手色。
    if (simultaneousSlideHands(list).length > 0) {
      return { text: 'L+R滑', hand: 'L', handover: false, dual: true };
    }
    // Touch Group 連帶判定沒有實際接觸，標籤要和真的按下去的區分開。
    if (list.every((item) => item.part === 'group')) {
      return { text: `${list[0].hand}連帶`, hand: list[0].hand, handover: false, dual: false };
    }
    const head = list.find((item) => item.part === 'head' || item.part === 'contact');
    const slide = list.find((item) => item.part === 'slide');
    // 起點與軌道分屬不同手（且沒有交接段）時要講清楚，不能只寫其中一隻手。
    if (head && slide && head.hand !== slide.hand) {
      return { text: `${head.hand}起${slide.hand}滑`, hand: head.hand, handover: false, dual: false };
    }
    const hands = [...new Set(list.map((item) => item.hand))];
    return { text: hands.join('/'), hand: hands[0], handover: false, dual: false };
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

  /** 星形環帶（中間挖掉一顆小星）；angle 是其中一個外角指的方向。 */
  function starBand(atPoint: Point, angle: number, outer?: number, hole?: number): string {
    return starBandPath(calibration, atPoint, angle, outer, hole);
  }

  /** 照遊戲上色時，白邊裡面那一層顏色：外角縮一圈、星形孔大一圈，外緣和孔緣各露出一圈白邊。 */
  function starFill(atPoint: Point, angle: number): string {
    return starBand(atPoint, angle, starFillOuter(discRadius), starFillHole(discRadius));
  }

  /** 起點星星的轉角（notes.starSpin）；只在有飛入動畫時轉。 */
  function headSpin(note: Note): number {
    return view.noteMode === 'window' && view.approach ? starSpin(note, time) : 0;
  }

  /** 音符在畫面上相對於盤面中心的方向（弧度，y 向下）；環帶明暗的分界落在這個方向上。 */
  function screenAngle(position: Point): number {
    const at = px(position);
    return Math.atan2(at.y - center.y, at.x - center.x);
  }

  /** 幾個封閉輪廓接成一條路徑（配 fill-rule: evenodd 就是挖洞）。 */
  function loopsPath(loops: Point[][]): string {
    return loops.map((loop) => closedPath(loop)).join('');
  }

  /** 一條邊（盤面座標的兩個端點）→ 畫面路徑。 */
  function edgePath(from: Point, to: Point): string {
    return polylinePath([from, to], calibration);
  }

  /** Tap 環帶的四段弧：所有 Tap 共用同一組路徑，各自轉到自己的分界方向。 */
  const quarters = $derived(ringQuarters(len(NOTE_RADIUS)));

  /** 三角形的縮放倍率：三條邊各往外（正）或往內（負）平移這麼多像素。 */
  function fanScale(fan: TouchFan, offsetPx: number): number {
    return 1 + offsetPx / len(fan.inradius);
  }

  /** Touch 的三角形中間挖空；Touch Hold 照遊戲上色時是實心的。 */
  function fanHollow(isHold: boolean): boolean {
    return !(isHold && view.noteColors);
  }

  /** 三角形本體：外輪廓，加上要挖掉的小三角形。 */
  function fanBody(at: Point, fan: TouchFan, isHold: boolean): string {
    const loops = [fanLoop(at, fan)];
    if (fanHollow(isHold)) loops.push(fanLoop(at, fan, TOUCH_FAN_HOLE));
    return loopsPath(loops);
  }

  /** 照遊戲上色時白邊裡面的那一層顏色：外緣縮一圈，挖空的小三角形大一圈。 */
  function fanFill(at: Point, fan: TouchFan, isHold: boolean): string {
    const loops = [fanLoop(at, fan, fanScale(fan, -FAN_RIM_PX))];
    if (fanHollow(isHold)) loops.push(fanLoop(at, fan, TOUCH_FAN_HOLE + FAN_HOLE_RIM_PX / len(fan.inradius)));
    return loopsPath(loops);
  }

  /** 照遊戲上色時三角形外緣白邊的寬度（像素）；挖空那一塊的邊緣細一點。 */
  const FAN_RIM_PX = 2;
  const FAN_HOLE_RIM_PX = 2;
  /** Touch Hold 外框底下那層深色描邊，兩側各露出多寬（像素）。 */
  const HOLD_BAND_EDGE_PX = 2;

  function holdPoints(note: Note, visual: NoteVisual, offset = 0): Point[] {
    return holdOutline(
      note.position,
      approachScale(visual),
      tailScale(visual),
      NOTE_RADIUS + offset / discRadius,
    );
  }

  function holdPath(note: Note, visual: NoteVisual, offset = 0): string {
    return closedPath(holdPoints(note, visual, offset));
  }

  /** 軌道上的星星：位置取 Rust 的路徑取樣點，一個角朝前進方向。 */
  function starOnPath(samples: PathSample[], u: number, fill: boolean): string {
    const at = pointOnSamples(samples, u);
    const angle = directionOnSamples(samples, u);
    return fill ? starFill(at, angle) : starBand(at, angle);
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

  // 盤面圓幾乎佔滿畫布，額外留白避免標記與文字被裁掉。
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
    class:is-colored={view.noteColors}
    viewBox={VIEW_BOX}
    preserveAspectRatio="xMidYMid meet"
    aria-label="maimai 盤面：音符與左右手軌跡"
  >
    <g transform={zoomTransform}>
      <!-- 盤面底圖：純向量繪製（DiscBoard），以校準的圓心與半徑為準，不再使用 maimai.png。 -->
      <DiscBoard {center} radius={discRadius} grid={view.showBoardGrid} />

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
                <!-- 一條軌道的箭頭合成一個 path；星星走過的箭頭不畫，只留前方那段。
                     WiFi 是一整面扇形（尖端在中央、兩翼跟著側線），三顆星共用同一個 slideU。 -->
                <g
                  class="slide-path"
                  class:is-selected={session.selectedNoteId === visual.note.id}
                  class:is-break={visual.note.modifiers.breakSlide}
                  class:is-each={each.tracks.has(visual.note.id)}
                  opacity={visual.trackOpacity}
                >
                  <!-- 每條箭頭帶兩層：底下一層黑色（箭頭的外框＋往後錯開的陰影），上面才是箭頭本身。
                       疊在別條軌道上時，每個箭頭都看得出是一個一個的，不會糊成一片。 -->
                  {#if !isFan(path)}
                    {#each path.branches as branch, index (index)}
                      <path
                        class="slide-under"
                        d={remainingChevronShadows(stripFor(branch), visual.slideU)}
                      />
                      <path
                        class="slide-chevrons"
                        d={remainingChevrons(stripFor(branch), visual.slideU)}
                      />
                    {/each}
                  {/if}
                  <path class="slide-under" d={remainingChevronShadows(trackFor(path), visual.slideU)} />
                  <path class="slide-chevrons" d={remainingChevrons(trackFor(path), visual.slideU)} />
                </g>
              {/if}
            {/if}
          {/each}
        </g>

        {#if view.noteMode === 'window' && each.links.length > 0}
          <!-- each 連線：同時的兩顆外圈音符之間一段細圓弧，跟著音符一起往外飛，判定後消失。 -->
          <g class="each-lines" aria-hidden="true">
            {#each each.links as link (link.key)}
              {@const visual = visuals[link.a]}
              {@const other = visuals[link.b]}
              {#if visual && other && visual.visible && visual.phase === 'upcoming'}
                <path
                  class="each-line"
                  d={eachArcPath(calibration, visual.note.position, other.note.position, approachScale(visual))}
                  opacity={visual.grow * visual.opacity}
                />
              {/if}
            {/each}
          </g>
        {/if}

        <!-- EX 音符共用的模糊濾鏡：把墊在音符底下的那一筆暈開成光暈。 -->
        <filter id={exGlowId} x="-60%" y="-60%" width="220%" height="220%" color-interpolation-filters="sRGB">
          <feGaussianBlur stdDeviation={EX_GLOW_BLUR_PX} />
        </filter>

        <g class="notes">
          {#each visuals as visual, slot (visual.note.id)}
            {@const note = visual.note}
            {#if visual.visible}
              {@const factor = approachScale(visual)}
              {@const center2 = px(scalePoint(note.position, factor))}
              {@const tag = view.showAssignmentTags ? tagFor(note.id) : null}
              {@const selected = session.selectedNoteId === note.id}
              {@const band = bandWidth(selected || visual.phase === 'active')}
              <!-- 起飛前在內圈原地放大：以音符中心縮放整組，筆寬跟著縮；指派標籤在放大期不畫。
                   倍率只由播放時間換算，暫停與拖曳都停在同一刻。Touch 的 grow 永遠是 1。 -->
              <g
                class="note"
                class:is-tap={note.kind === 'tap'}
                class:is-hold={note.kind === 'hold'}
                class:is-slide={note.kind === 'slide'}
                class:is-touch={note.kind === 'touch' || note.kind === 'touchHold'}
                class:is-touch-hold={note.kind === 'touchHold'}
                class:is-each={each.heads.has(note.id)}
                class:is-each-track={each.tracks.has(note.id)}
                class:is-selected={selected}
                class:is-active={visual.phase === 'active'}
                class:is-break={note.modifiers.breakNote}
                class:is-break-track={note.modifiers.breakSlide}
                class:is-ex={note.modifiers.ex}
                opacity={noteOpacity(visual)}
                transform={visual.grow < 1
                  ? `translate(${center2.x} ${center2.y}) scale(${visual.grow}) translate(${-center2.x} ${-center2.y})`
                  : undefined}
                role="button"
                tabindex="0"
                aria-label={noteLabel(note, visual)}
                onclick={() => selectNote(note.id)}
                onkeydown={(event) => onNoteKey(event, note.id)}
              >
                {#if note.kind === 'touch' || note.kind === 'touchHold'}
                  <!-- Touch 一律停在 Rust 給的落點上，照遊戲畫：四片三角形尖端朝落點、由外往內收攏，
                       四個尖端在落點相碰的瞬間就是判定時間，不從盤面中心飛出去。 -->
                  {@const at = note.position}
                  {@const mark = px(at)}
                  {@const r = touchRadius(note.touchArea)}
                  {@const isHold = note.kind === 'touchHold'}
                  {@const fans = touchFans(r, touchGather(visual), isHold)}
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
                    {#if visual.phase !== 'upcoming'}
                      <!-- Touch Hold：按住之後，外面一圈粗的圓角菱形從正上方順時針亮起來，
                           亮滿一圈就是結束時間。按下去之前沒有外框（照遊戲）。 -->
                      {@const bandWidth = len(r * TOUCH_HOLD_BAND)}
                      <g class="touch-hold-border">
                        {#each touchHoldBorder(at, r) as quarter, index (index)}
                          {@const fill = touchHoldBorderFill(visual.progress, index)}
                          <g class="quarter">
                            {#if fill > 0}
                              <path
                                class="touch-hold-edge"
                                d={polylinePath(quarter, calibration)}
                                pathLength="1"
                                stroke-dasharray={`${fill} 1`}
                                stroke-width={bandWidth + HOLD_BAND_EDGE_PX * 2}
                              />
                              <path
                                class="touch-hold-fill"
                                d={polylinePath(quarter, calibration)}
                                pathLength="1"
                                stroke-dasharray={`${fill} 1`}
                                stroke-width={bandWidth}
                              />
                            {/if}
                          </g>
                        {/each}
                      </g>
                    {/if}
                  {:else if visual.phase !== 'upcoming'}
                    <!-- Touch：收到位那一刻亮起一個圓角方框，跟著音符一起淡出 -->
                    <path class="touch-just" d={closedPath(touchJustBorder(at, r))} />
                  {/if}
                  <!-- 四片圓角的直角三角形；收到位時剛好拼成一整塊（Touch 正方形、Touch Hold 菱形）。 -->
                  {#if view.noteColors}
                    <!-- 照遊戲上色時，四片的深色外框先整圈畫在最底下：
                         收到位時片與片之間只剩白邊拼成的接縫，中間不會夾一條黑線。 -->
                    <path class="touch-fan-edge" d={loopsPath(fans.map((fan) => fanLoop(at, fan)))} />
                  {/if}
                  <g class="touch-fans" class:is-hold={isHold}>
                    {#each fans as fan, index (index)}
                      <g class="fan">
                        <!-- EX 在 Touch／Touch Hold 上不畫：遊戲建立這兩種音符時直接把 EX 關掉，沒有 EX 的樣子。 -->
                        <path class="touch-fan" d={fanBody(at, fan, isHold)} />
                        {#if view.noteColors}
                          <path class="touch-fan-fill" d={fanFill(at, fan, isHold)} />
                        {/if}
                      </g>
                    {/each}
                  </g>
                  {#if showSensors && view.showSensorLabels}
                    <!-- 落點標記開著時，音符會蓋住底下的編號，所以改寫在音符中心 -->
                    <text
                      class="touch-label"
                      x={mark.x}
                      y={mark.y}
                      text-anchor="middle"
                      dominant-baseline="central"
                    >
                      {touchName(note.touchArea, note.button)}
                    </text>
                  {:else}
                    <!-- 落點正中心的圓點：外面一圈白邊，壓在四個尖端上面 -->
                    <circle
                      class="touch-dot-rim"
                      cx={mark.x}
                      cy={mark.y}
                      r={len(TOUCH_DOT + TOUCH_DOT_RING)}
                    />
                    <circle class="touch-dot" cx={mark.x} cy={mark.y} r={len(TOUCH_DOT)} />
                  {/if}
                {:else if note.kind === 'hold'}
                  <!-- 空心長條：一圈粗環帶，兩端收成六邊形的尖角；墊一層深色描邊，疊在別的東西上也分得開。 -->
                  {@const bar = holdPath(note, visual)}
                  <!-- EX：同形狀的一圈柔和光暈，貼著外緣往外淡出。
                       照遊戲上色時直接貼著白邊（蓋住深色描邊）；白色音符時墊在深色描邊底下，環帶的邊緣才分得出來。 -->
                  {#snippet holdGlow()}
                    <path
                      class="note-ex"
                      d={holdPath(note, visual, exGlowOffset(band))}
                      stroke-width={exGlowWidth(band)}
                      filter={`url(#${exGlowId})`}
                    />
                  {/snippet}
                  {#if note.modifiers.ex && !view.noteColors}{@render holdGlow()}{/if}
                  <path class="note-edge" d={bar} stroke-width={edgeWidth(band)} />
                  {#if note.modifiers.ex && view.noteColors}{@render holdGlow()}{/if}
                  <path class="hold-bar" d={bar} stroke-width={band} />
                  {#if view.noteColors}
                    <!-- 照遊戲上色：六條邊各自由深漸到淺（每條邊一個線性漸層），外緣與內緣各一圈白邊 -->
                    {#each outlineEdges(holdPoints(note, visual)) as edge, index (index)}
                      {@const from = px(edge.from)}
                      {@const to = px(edge.to)}
                      <linearGradient
                        id={`${shadeId}-${slot}-${index}`}
                        gradientUnits="userSpaceOnUse"
                        x1={from.x}
                        y1={from.y}
                        x2={to.x}
                        y2={to.y}
                      >
                        <stop class="shade-deep" offset="0" />
                        <stop class="shade-deep" offset={SHADE_FLAT} />
                        <stop class="shade-light" offset="1" />
                      </linearGradient>
                      <path
                        class="note-shade"
                        d={edgePath(edge.from, edge.to)}
                        stroke={`url(#${shadeId}-${slot}-${index})`}
                        stroke-width={band}
                      />
                    {/each}
                    <path class="note-rim" d={holdPath(note, visual, rimOffset(band))} />
                    <path class="note-rim is-inner" d={holdPath(note, visual, rimInnerOffset(band))} />
                  {/if}
                  {#if visual.tailTravel > 0}
                    <!-- 尾端離開內圈之後，尾巴中心有一顆小點（遊戲裡標示 Hold 結束的位置） -->
                    {@const tail = px(scalePoint(note.position, tailScale(visual)))}
                    <circle class="note-dot" cx={tail.x} cy={tail.y} r={len(NOTE_DOT)} />
                  {/if}
                {:else if note.kind === 'slide'}
                  {@const path = note.pathId ? session.pathById.get(note.pathId) : undefined}
                  {#if visual.phase === 'upcoming' || !path}
                    {#if note.hasHead}
                      <!-- 起點星星：飛到鍵位之前，一個角朝盤面外側。 -->
                      {@const at = scalePoint(note.position, factor)}
                      {@const angle = radialAngle(note.position) + headSpin(note)}
                      {#if note.modifiers.ex}
                        <!-- EX：星形的柔和光暈，墊在星星底下 -->
                        <path
                          class="note-ex"
                          d={starOutlinePath(calibration, at, angle)}
                          stroke-width={EX_GLOW_SPREAD_PX * 2}
                          filter={`url(#${exGlowId})`}
                        />
                      {/if}
                      <path class="note-star" d={starBand(at, angle)} />
                      {#if view.noteColors}
                        <path class="note-star-fill" d={starFill(at, angle)} />
                      {/if}
                      <circle class="note-dot" cx={center2.x} cy={center2.y} r={len(STAR_DOT)} />
                    {/if}
                  {:else}
                    <!-- 判定之後換成軌道上的星星：先停在起點等，再沿路徑滑到終點，一個角朝前進方向。
                         WiFi 側線的星星與中央同一個 slideU，三顆同時抵達。 -->
                    {#each [...path.branches, path.samples] as line, index (index)}
                      {@const dot = px(pointOnSamples(line, visual.slideU))}
                      <path class="note-star is-moving" d={starOnPath(line, visual.slideU, false)} />
                      {#if view.noteColors}
                        <path class="note-star-fill is-moving" d={starOnPath(line, visual.slideU, true)} />
                      {/if}
                      <circle class="note-dot is-moving" cx={dot.x} cy={dot.y} r={len(STAR_DOT)} />
                    {/each}
                  {/if}
                {:else}
                  <!-- Tap：一圈粗環帶＋正中心一個小點，中間挖空，看得到下面的軌跡與手。 -->
                  {@const ring = len(NOTE_RADIUS)}
                  <!-- EX：一圈柔和的光暈，貼著外緣往外淡出（照遊戲，不是另外一圈細線）。
                       照遊戲上色時直接貼著白邊（蓋住深色描邊）；白色音符時墊在深色描邊底下，環帶的邊緣才分得出來。 -->
                  {#snippet tapGlow()}
                    <circle
                      class="note-ex"
                      cx={center2.x}
                      cy={center2.y}
                      r={ring + exGlowOffset(band)}
                      stroke-width={exGlowWidth(band)}
                      filter={`url(#${exGlowId})`}
                    />
                  {/snippet}
                  {#if note.modifiers.ex && !view.noteColors}{@render tapGlow()}{/if}
                  <circle class="note-edge" cx={center2.x} cy={center2.y} r={ring} stroke-width={edgeWidth(band)} />
                  {#if note.modifiers.ex && view.noteColors}{@render tapGlow()}{/if}
                  <circle class="note-ring" cx={center2.x} cy={center2.y} r={ring} stroke-width={band} />
                  {#if view.noteColors}
                    <!-- 照遊戲上色：一圈切成四段，每段順時針由深漸到淺（每段一條弧＋一個線性漸層），
                         整組轉到「盤面中心 → 音符」的方向；外緣與內緣各一圈白邊 -->
                    <g
                      transform={`translate(${center2.x} ${center2.y}) rotate(${(screenAngle(note.position) * 180) / Math.PI})`}
                    >
                      {#each quarters as quarter, index (index)}
                        <linearGradient
                          id={`${shadeId}-${slot}-${index}`}
                          gradientUnits="userSpaceOnUse"
                          x1={quarter.from.x}
                          y1={quarter.from.y}
                          x2={quarter.to.x}
                          y2={quarter.to.y}
                        >
                          <stop class="shade-deep" offset="0" />
                          <stop class="shade-deep" offset={SHADE_FLAT} />
                          <stop class="shade-light" offset="1" />
                        </linearGradient>
                        <path
                          class="note-shade"
                          d={quarter.d}
                          stroke={`url(#${shadeId}-${slot}-${index})`}
                          stroke-width={band}
                        />
                      {/each}
                    </g>
                    <circle class="note-rim" cx={center2.x} cy={center2.y} r={ring + rimOffset(band)} />
                    <circle
                      class="note-rim is-inner"
                      cx={center2.x}
                      cy={center2.y}
                      r={ring + rimInnerOffset(band)}
                    />
                  {/if}
                  <circle class="note-dot" cx={center2.x} cy={center2.y} r={len(NOTE_DOT)} />
                {/if}

                {#if tag && visual.tagOpacity > 0}
                  {@const anchor = px(tagAnchor(note, factor))}
                  <!-- 起飛後才淡入，避免內圈放大中的多顆音符標籤擠在盤面中心 -->
                  <text
                    class="note-tag"
                    opacity={visual.tagOpacity}
                    class:is-left={tag.hand === 'L'}
                    class:is-right={tag.hand === 'R'}
                    class:is-handover={tag.handover}
                    x={anchor.x}
                    y={anchor.y}
                    text-anchor="middle"
                    dominant-baseline="central"
                  >
                    {#if tag.dual}
                      <tspan class="tag-left">L</tspan><tspan class="tag-plain">+</tspan><tspan
                        class="tag-right">R</tspan
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

        {#if hits.length > 0 || holdRings.length > 0}
          <!-- 打擊效果（可選）：判定那一刻在原地放出的光暈。一般音符是六角形、Break 是星星、
               Touch 是實心圓。線條本身只畫一次，光暈交給濾鏡：寬的模糊當外圈的光、
               窄的模糊疊兩次當柔邊的亮線（使用者 2026-11-23 要求要有模糊的光暈，不要純線條）。
               進度只由絕對播放時間換算，暫停與拖曳都停在同一刻。
               按住 Hold／Touch Hold 的期間另有圓形的光圈一圈一圈往外擴：每一圈各自的濾鏡，
               線寬與模糊都跟著那一圈的半徑放大。 -->
          <g class="hits" aria-hidden="true">
            {#each holdRings as ring, slot (ring.key)}
              <filter
                id={`${hitGlowId}-loop-${slot}`}
                x="-45%"
                y="-45%"
                width="190%"
                height="190%"
                color-interpolation-filters="sRGB"
              >
                <feGaussianBlur in="SourceGraphic" stdDeviation={ring.wide} result="wide" />
                <feGaussianBlur in="SourceGraphic" stdDeviation={ring.soft} result="soft" />
                <feMerge>
                  <feMergeNode in="wide" />
                  <feMergeNode in="soft" />
                  <feMergeNode in="soft" />
                </feMerge>
              </filter>
              <path
                class="hit-line hit-ring"
                d={ring.d}
                stroke-width={ring.width}
                opacity={ring.opacity}
                filter={`url(#${hitGlowId}-loop-${slot})`}
              />
            {/each}
            {#each hits as hit, slot (hit.key)}
              {@const glow = hitGlow(calibration, hit)}
              <filter
                id={`${hitGlowId}-${slot}`}
                x="-45%"
                y="-45%"
                width="190%"
                height="190%"
                color-interpolation-filters="sRGB"
              >
                <feGaussianBlur in="SourceGraphic" stdDeviation={glow.wide} result="wide" />
                <feGaussianBlur in="SourceGraphic" stdDeviation={glow.soft} result="soft" />
                <feMerge>
                  <feMergeNode in="wide" />
                  <feMergeNode in="soft" />
                  <feMergeNode in="soft" />
                </feMerge>
              </filter>
              <g class="hit" opacity={hitOpacity(hit)} filter={`url(#${hitGlowId}-${slot})`}>
                {#each hitStrokes(calibration, hit) as stroke, index (index)}
                  {#if stroke.filled}
                    <path class="hit-fill" d={stroke.d} />
                  {:else}
                    <path class="hit-line" d={stroke.d} stroke-width={stroke.width} />
                  {/if}
                {/each}
              </g>
            {/each}
          </g>
        {/if}

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
  {#if session.loading}
    <!-- 分析或載入中：舊譜面的盤面已經不相關，蓋上骨架（延遲淡入，很快載完就不會閃）。 -->
    <div class="skeleton-veil disc-veil" role="status" aria-label={session.phase === 'analyzing' ? '分析中' : '載入中'}>
      <svg class="stage-svg" viewBox={VIEW_BOX} preserveAspectRatio="xMidYMid meet" aria-hidden="true">
        <g transform={zoomTransform}>
          <circle class="skeleton-ring" cx={center.x} cy={center.y} r={discRadius} />
          <circle class="skeleton-ring skeleton-ring--inner" cx={center.x} cy={center.y} r={discRadius * 0.52} />
          {#each { length: 8 } as _, index (index)}
            {@const angle = ((index * 45 - 67.5) * Math.PI) / 180}
            <circle
              class="skeleton-dot"
              cx={center.x + discRadius * Math.cos(angle)}
              cy={center.y + discRadius * Math.sin(angle)}
              r={discRadius * 0.045}
            />
          {/each}
        </g>
      </svg>
      <span class="disc-veil-label small">{session.phase === 'analyzing' ? '分析中…' : '載入中…'}</span>
    </div>
  {/if}
</div>

<style>
  .disc-veil {
    --veil-bg: #000;
    display: grid;
    place-items: center;
  }

  .skeleton-ring,
  .skeleton-dot {
    animation: skeleton-pulse 1.3s ease-in-out infinite;
  }

  /* 盤面永遠是黑底，骨架用固定的深灰，不隨淺色配色變亮。 */
  .skeleton-ring {
    fill: none;
    stroke: #1e242d;
    stroke-width: 26;
  }

  .skeleton-ring--inner {
    stroke-width: 14;
    animation-delay: 0.2s;
  }

  .skeleton-dot {
    fill: #272f3a;
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

  .disc-veil-label {
    position: relative;
    color: var(--c-text-dim);
  }

  @media (prefers-reduced-motion: reduce) {
    .skeleton-ring,
    .skeleton-dot {
      animation: none;
    }
  }

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

  /*
   * 音符與軌道的顏色走變數：--nc（音符本體）、--sc（滑行中的星星）、--tc（Slide 軌道）。
   * 平常是中性的白與灰，Break 橘；「照遊戲上色」打開（.is-colored）才換成遊戲的配色，形狀完全不變。
   */
  .slide-path {
    --tc: #8d97a6;
  }

  .slide-path.is-break {
    --tc: #c9863a;
  }

  .note {
    --nc: #eef2f8;
    --sc: #eef2f8;
    /* 明暗的深色那一端；不上色時用不到。 */
    --nd: #eef2f8;
    /* EX 光暈的顏色；不上色時是白光。 */
    --xc: #eef2f8;
    cursor: pointer;
  }

  .note.is-break {
    --nc: #f0913a;
  }

  .note.is-break-track {
    --sc: #f0913a;
  }

  /* 遊戲配色：Tap／Hold 粉紅、Slide 與 Touch 藍、同時押黃、Break 橘。
     每個顏色有淺（--nc）深（--nd）兩端：環帶不是一個顏色塗滿，是由深漸到淺（量自遊戲畫面）。
     EX 光暈的顏色（--xc）照遊戲：Tap／Hold 粉、星星白，同時押與 Break 都是黃。 */
  .stage-svg.is-colored .note.is-tap,
  .stage-svg.is-colored .note.is-hold {
    /* 深淺兩端與 EX 光暈都量自使用者 2026-11-23 的截圖。 */
    --nc: #ff7fd0;
    --nd: #ff1f7f;
    --xc: #ffc0f2;
  }

  .stage-svg.is-colored .note.is-slide,
  .stage-svg.is-colored .note.is-touch {
    --nc: #22b8f0;
    --sc: #22b8f0;
    --xc: #ffffff;
  }

  .stage-svg.is-colored .note.is-touch {
    --nc: #1aa6f7;
    --nd: #1aa6f7;
  }

  .stage-svg.is-colored .slide-path {
    --tc: #22b8f0;
  }

  .stage-svg.is-colored .note.is-each {
    --nc: #fbfd1e;
    --nd: #f4cc2d;
    --xc: #fffe77;
  }

  .stage-svg.is-colored .note.is-slide.is-each {
    --nc: #faf52c;
  }

  .stage-svg.is-colored .note.is-each-track {
    --sc: #faf52c;
  }

  .stage-svg.is-colored .slide-path.is-each {
    --tc: #faf52c;
  }

  .stage-svg.is-colored .note.is-break {
    --nc: #ffb24a;
    --nd: #ec7a10;
    --xc: #fffe77;
  }

  .stage-svg.is-colored .note.is-slide.is-break {
    --nc: #f0913a;
  }

  /* Touch Hold 的四片三角形與外框四段，照遊戲各有自己的顏色：右上紅、右下黃、左下綠、左上藍
     （三角形量自遊戲錄影；外框那一段比三角形淡一階）。 */
  .stage-svg.is-colored .note.is-touch-hold .fan:nth-child(1) {
    --nc: #f64303;
  }

  .stage-svg.is-colored .note.is-touch-hold .fan:nth-child(2) {
    --nc: #f3ee00;
  }

  .stage-svg.is-colored .note.is-touch-hold .fan:nth-child(3) {
    --nc: #10a667;
  }

  .stage-svg.is-colored .note.is-touch-hold .fan:nth-child(4) {
    --nc: #0093f6;
  }

  .stage-svg.is-colored .note.is-touch-hold .quarter:nth-child(1) {
    --nc: #f5713a;
  }

  .stage-svg.is-colored .note.is-touch-hold .quarter:nth-child(2) {
    --nc: #f2d62a;
  }

  .stage-svg.is-colored .note.is-touch-hold .quarter:nth-child(3) {
    --nc: #2fbf7a;
  }

  .stage-svg.is-colored .note.is-touch-hold .quarter:nth-child(4) {
    --nc: #3aa6ee;
  }

  .stage-svg.is-colored .note.is-break-track {
    --sc: #f0913a;
  }

  .stage-svg.is-colored .slide-path.is-break {
    --tc: #f0913a;
  }

  .note.is-selected,
  .stage-svg.is-colored .note.is-selected,
  .stage-svg.is-colored .note.is-selected .fan,
  .stage-svg.is-colored .note.is-selected .quarter {
    --nc: var(--c-accent);
    --nd: var(--c-accent);
    --sc: var(--c-accent);
    --xc: var(--c-accent);
  }

  .slide-path.is-selected,
  .stage-svg.is-colored .slide-path.is-selected {
    --tc: var(--c-accent);
  }

  .note:focus-visible,
  .stage-svg.is-colored .note:focus-visible,
  .stage-svg.is-colored .note:focus-visible .fan,
  .stage-svg.is-colored .note:focus-visible .quarter {
    --nc: var(--c-focus);
    --nd: var(--c-focus);
    --xc: var(--c-focus);
    outline: none;
  }

  /* Slide 軌道：沒有中心線，一整排實心的寬「＞」。平常比音符暗一階，音符與手永遠壓在上面。
     每個箭頭底下墊一層黑色：一圈黑外框（描邊露出 3.5）加上往後錯開的陰影。
     黑底上看不出來；兩條軌道交疊時，上面那條的箭頭一個一個分得開，不會糊在一起。 */
  .slide-under {
    fill: #000;
    stroke: #000;
    stroke-width: 7;
    stroke-linejoin: round;
  }

  .slide-chevrons {
    fill: var(--tc);
  }

  /* each 連線：細圓弧，比音符暗一階。 */
  .each-line {
    fill: none;
    stroke: #8d97a6;
    stroke-width: 4;
  }

  .stage-svg.is-colored .each-line {
    stroke: #faf52c;
  }

  /* 墊在環帶下面的深色描邊（Tap 是圓、Hold 是長條）；筆寬由環帶寬度換算，寫在元素上。 */
  .note-edge {
    fill: none;
    stroke: #0b0d11;
    stroke-linejoin: miter;
  }

  /* Tap：粗環帶，中間挖空；筆寬與 Hold 相同（noteShape 的 BAND_PX），外緣對齊。 */
  .note-ring {
    fill: none;
    stroke: var(--nc);
  }

  /* 照遊戲上色時疊在環帶上的明暗：每一段（Tap 的四分之一圈、Hold 的一條邊）一筆，
     筆觸是從深（--nd）漸到淺（--nc）的線性漸層，漸層寫在元素旁邊、筆觸指過去。
     連續的漸層是使用者 2026-11-23 要求的（原本是幾階純色）；只用在音符本體，不是面板底色。 */
  .note-shade {
    fill: none;
    stroke-linecap: butt;
  }

  .shade-deep {
    stop-color: var(--nd);
  }

  .shade-light {
    stop-color: var(--nc);
  }

  /* 照遊戲上色時，環帶外緣的一圈白邊（noteShape 的 RIM_PX）；內緣那一圈細一點。 */
  .note-rim {
    fill: none;
    stroke: #f6f8fc;
    stroke-width: 3.5;
    stroke-linejoin: miter;
  }

  .note-rim.is-inner {
    stroke-width: 2.5;
  }

  /* Tap 與星星正中心的小點，Hold 尾端也有一顆。 */
  .note-dot {
    fill: var(--nc);
  }

  .note-dot.is-moving {
    fill: var(--sc);
  }

  /* EX：墊在音符底下的一圈柔和光暈（Tap 是圓、Hold 是同形狀外框、星星是星形的外輪廓），
     筆寬寫在元素上，整圈過一次模糊濾鏡；只往外淡出，洞裡面不亮。
     照遊戲的樣子，使用者 2026-11-23 要求（原本是多一圈細線）。 */
  .note-ex {
    fill: none;
    stroke: var(--xc);
    stroke-opacity: 0.88;
    stroke-linejoin: round;
  }

  /* 星星：星形環帶，中間挖掉一顆小星（evenodd）。描邊一半壓在底下，露出外緣與孔緣各一圈深色。 */
  .note-star {
    fill: var(--nc);
    fill-rule: evenodd;
    stroke: #0b0d11;
    stroke-width: 3;
    stroke-linejoin: miter;
    paint-order: stroke;
  }

  .note-star.is-moving {
    fill: var(--sc);
  }

  /* 照遊戲上色：外面那顆變成白邊，顏色畫在裡面縮一圈的這一層。 */
  .stage-svg.is-colored .note-star {
    fill: #f6f8fc;
  }

  .note-star-fill {
    fill: var(--nc);
    fill-rule: evenodd;
  }

  .note-star-fill.is-moving {
    fill: var(--sc);
  }

  /* 空心長條：中間完全挖空，筆寬與 Tap 環帶一致。 */
  .hold-bar {
    fill: none;
    stroke: var(--nc);
    stroke-linejoin: miter;
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

  /* Touch：四片圓角的直角三角形，直角那個尖端朝落點；中間挖掉一塊小三角形（evenodd），
     和 Tap 的環帶同一個做法。描邊一半壓在底下，露出外緣與孔緣各一圈深色。 */
  .touch-fan {
    fill: var(--nc);
    fill-rule: evenodd;
    stroke: #0b0d11;
    stroke-width: 3;
    stroke-linejoin: round;
    paint-order: stroke;
  }

  /* 照遊戲上色：外面那一層變成白邊，顏色畫在裡面縮一圈的這一層（平塗，遊戲的三角形幾乎沒有明暗）。 */
  .stage-svg.is-colored .touch-fan {
    fill: #f6f8fc;
    stroke: none;
  }

  /* 上色時四片共用的深色外框，墊在四片底下（見模板的說明）。 */
  .touch-fan-edge {
    fill: none;
    stroke: #0b0d11;
    stroke-width: 3;
    stroke-linejoin: round;
  }

  .touch-fan-fill {
    fill: var(--nc);
    fill-rule: evenodd;
  }

  /* 落點正中心的圓點：白邊＋深色描邊，裡面那顆跟著音符的顏色。 */
  .touch-dot-rim {
    fill: #f6f8fc;
    stroke: #0b0d11;
    stroke-width: 3;
    paint-order: stroke;
  }

  .touch-dot {
    fill: var(--nc);
  }

  /* Touch 收到位那一刻亮起的圓角方框。 */
  .touch-just {
    fill: none;
    stroke: #f6f8fc;
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

  /* Touch Hold 外框：粗的圓角菱形，按住之後從正上方順時針一段一段亮起來；筆寬寫在元素上。
     底下墊一層深色描邊，疊在別的音符上也分得開。 */
  .touch-hold-edge {
    fill: none;
    stroke: #0b0d11;
    stroke-linecap: butt;
    stroke-linejoin: round;
  }

  .touch-hold-fill {
    fill: none;
    stroke: var(--nc);
    stroke-linecap: butt;
    stroke-linejoin: round;
  }

  /* 落點編號寫在音符中心（落點標記開著時才有），黑色描邊保證壓在任何筆觸上都讀得到。 */
  .touch-label {
    fill: #eef2f8;
    font-family: var(--font-mono);
    font-size: 19px;
    font-weight: 700;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 5px;
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

  /* 打擊效果：發光的線條。線只畫一次（筆寬寫在元素上），光暈由整組的模糊濾鏡疊出來；
     線條略帶透明，兩條線交疊的地方會更亮，像遊戲的加亮混色。
     模糊與半透明只用在這個一閃而過的筆觸上，不是面板底色。
     不上色時是白光，照遊戲上色時是遊戲的淡黃光。 */
  .hits {
    pointer-events: none;
  }

  .hit-line {
    fill: none;
    stroke: #f4f7fb;
    stroke-opacity: 0.8;
    stroke-linejoin: round;
  }

  /* 按住 Hold／Touch Hold 時往外擴的光圈：一圈一圈各自獨立、不會互相交疊加亮，
     所以線本身畫到全亮，靠整圈的不透明度淡出（錄影裡剛冒出來的那一圈幾乎是全亮）。 */
  .hit-ring {
    stroke-opacity: 1;
  }

  /* Touch 的實心圓：整片淡淡的光，邊緣被濾鏡暈開。 */
  .hit-fill {
    fill: #eef2f8;
    fill-opacity: 0.6;
    stroke: none;
  }

  .stage-svg.is-colored .hit-line {
    stroke: #f2e75c;
  }

  .stage-svg.is-colored .hit-fill {
    fill: #f8ee7c;
  }

  /* 光圈的黃比六角形那組亮一階（錄影裡量到的顏色）。 */
  .stage-svg.is-colored .hit-ring {
    stroke: #fff76a;
  }

  /* 判定中、選中：Tap／Hold 的環帶加粗（筆寬寫在元素上），Touch 的深色描邊加粗。 */
  .note.is-active .touch-fan,
  .note.is-active .touch-fan-edge {
    stroke-width: 5;
  }

  .note.is-selected .touch-fan,
  .note.is-selected .touch-fan-edge {
    stroke-width: 7;
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

  /* 兩手同時滑行：字母各用自己的手色，連接字用中性白 */
  .note-tag .tag-left {
    fill: var(--c-left);
  }

  .note-tag .tag-right {
    fill: var(--c-right);
  }

  .note-tag .tag-plain {
    fill: #eef2f8;
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
