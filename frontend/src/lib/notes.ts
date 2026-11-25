import { KIND_LABEL } from './contract';
import type { Assignment, Hand, Note, Point, SlidePath } from './types';

/**
 * 同一顆 Slide 由兩手「同時」各滑一段（例如 WiFi 2+1）時回傳兩手，否則回傳空陣列。
 * 只看 Rust 的指派區間是否重疊，不從軌道形狀推測哪隻手蓋到哪條線。
 * 呼叫端須先排除交接：交接的重疊期間也是兩手同在軌道上，但意義不同。
 */
export function simultaneousSlideHands(list: Assignment[]): Hand[] {
  const slides = list.filter((item) => item.part === 'slide');
  for (const a of slides) {
    for (const b of slides) {
      if (a.hand === 'L' && b.hand === 'R' && a.startSeconds < b.endSeconds && b.startSeconds < a.endSeconds) {
        return ['L', 'R'];
      }
    }
  }
  return [];
}

/** 音符落點的簡短寫法：外圈鍵為 `3`，Touch 為 `B5` 或 `C`。 */
export function noteTarget(note: Note): string {
  if (!note.touchArea) return String(note.button);
  return `${note.touchArea}${note.button > 0 ? note.button : ''}`;
}

/** 音符的簡短說明，例如「Tap 3」「Slide 1-5」。 */
export function noteLabel(note: Note, pathById: Map<string, SlidePath>): string {
  const kind = KIND_LABEL[note.kind] ?? note.kind;
  if (note.kind === 'slide' && note.pathId) {
    const path = pathById.get(note.pathId);
    if (path) return `${kind} ${path.startButton}${path.shape}${path.endButton}`;
  }
  return `${kind} ${noteTarget(note)}`;
}

/** 標註步驟的說明：有起點的 Slide 分成「起點」與「滑行」兩步。 */
export function stepLabel(
  step: { note: Note; part: 'hand' | 'track' },
  pathById: Map<string, SlidePath>,
): string {
  const label = noteLabel(step.note, pathById);
  if (step.note.kind !== 'slide' || !step.note.hasHead) return label;
  return `${label} ${step.part === 'hand' ? '起點' : '滑行'}`;
}

/** 修飾語標籤，例如 Break、EX、煙火。 */
export function noteBadges(note: Note): string[] {
  const m = note.modifiers;
  const out: string[] = [];
  if (m.breakNote) out.push('Break');
  if (m.ex) out.push('EX');
  if (m.breakSlide) out.push('Break Slide');
  if (m.exSlide) out.push('EX Slide');
  if (m.spinStar) out.push('旋轉星');
  else if (m.star) out.push('星形');
  if (m.fireworks) out.push('煙火');
  if (note.kind === 'slide' && !note.hasHead) out.push('無起點');
  return out;
}

/**
 * 遊戲的 each（同時押）：只用來上色與畫連線，不影響任何判定或指派。
 * 規則照 MajdataPlay 的 NoteLoader：
 * - Tap／Hold／Touch 與 Slide 的起點，同一時間有兩顆以上就都是 each；Slide 本體不算顆數。
 * - Slide 的軌道，同一時間有兩條以上 Slide 才是 each。
 * - 連線只接外圈的音符（Touch 沒有），照譜面順序兩兩相接。
 */
export interface EachInfo {
  /** 算 each 的音符（Slide 指它的起點星星）。 */
  heads: Set<string>;
  /** 軌道算 each 的 Slide。 */
  tracks: Set<string>;
  /** 連線兩端的音符，值是 notes 陣列的索引。 */
  links: { key: string; a: number; b: number }[];
}

export function eachInfo(notes: Note[]): EachInfo {
  const groups = new Map<number, number[]>();
  for (let i = 0; i < notes.length; i += 1) {
    // 同時的音符出自同一個時間點，秒數完全相同；取到微秒只是避免浮點誤差。
    const key = Math.round(notes[i].timeSeconds * 1e6);
    const group = groups.get(key);
    if (group) group.push(i);
    else groups.set(key, [i]);
  }
  const info: EachInfo = { heads: new Set(), tracks: new Set(), links: [] };
  for (const group of groups.values()) {
    if (group.length < 2) continue;
    const counted = group.filter((i) => notes[i].kind !== 'slide' || notes[i].hasHead);
    if (counted.length > 1) {
      for (const i of counted) info.heads.add(notes[i].id);
      const outer = counted.filter((i) => !isTouchKind(notes[i]));
      for (let k = 0; k + 1 < outer.length; k += 1) {
        info.links.push({
          key: `${notes[outer[k]].id}-${notes[outer[k + 1]].id}`,
          a: outer[k],
          b: outer[k + 1],
        });
      }
    }
    const slides = group.filter((i) => notes[i].kind === 'slide');
    if (slides.length > 1) for (const i of slides) info.tracks.add(notes[i].id);
  }
  return info;
}

export type NotePhase = 'upcoming' | 'active' | 'done';

/*
 * 照遊戲的出現方式（純視覺，不影響判定時間）：
 * Tap／Hold／Slide 的起點先停在內圈原地放大，放大完成後才以固定速度飛向鍵位。
 * 數字出自 MajdataPlay 的 TapDrop／HoldDrop／SlideDrop：
 *   distance = (t − 判定時間) × Speed + 4.8（判定線 4.8，內圈 1.225）
 *   scale    = distance × 0.265 + (1 − 0.265 × 1.225)
 * 所以放大期長 (1 / 0.265) / Speed、飛行期長 (4.8 − 1.225) / Speed。
 * 這裡把「飛入時間」當成飛行期的長度，其餘照比例換算。
 */

/** 起飛的內圈半徑（鍵位半徑為 1）：1.225 / 4.8。 */
export const APPROACH_INNER = 1.225 / 4.8;

/** 放大期是飛行期的幾倍：(1 / 0.265) / (4.8 − 1.225)。 */
export const GROW_RATIO = 1 / 0.265 / (4.8 - 1.225);

/** Slide 軌道在判定前幾倍飛行時間開始淡入：3.926913 / (4.8 − 1.225)。 */
export const SLIDE_FADE_RATIO = 3.926913 / (4.8 - 1.225);

/** Slide 軌道淡入到一半透明度最多花多久（秒）。 */
const SLIDE_FADE_IN = 0.2;

/** Slide 軌道在判定前多久變成完全不透明（秒）。 */
const SLIDE_SOLID_BEFORE = 0.05;

/**
 * Slide 軌道從半透明亮到完全不透明花多久（秒），在 SLIDE_SOLID_BEFORE 那一刻剛好亮滿。
 * 遊戲本身是一格之內直接跳到全亮；但遊戲裡半透明只停大約 0.1 秒，看起來是一路亮上去。
 * 這裡半透明會停比較久，直接跳會很突兀（使用者 2026-11-23：沒有過渡變化很奇怪），所以改成漸變。
 */
const SLIDE_SOLID_RAMP = 0.18;

/** Touch 進場的前幾成時間只在原地淡入，之後才開始收攏（MajdataPlay TouchDrop：0.2）。 */
const TOUCH_FADE_PORTION = 0.2;

/**
 * Touch 四片三角形的收攏曲線（MajdataPlay TouchDrop）：
 * 距離 = (−exp(−8 × 0.43 × 剩餘時間比例 − 0.85) + 0.42) × 0.75，夾在 0–0.3 之間。
 * remain 是剩餘的收攏時間比例（1 = 剛開始收，0 = 判定時間），回傳 0–1 的收攏進度：
 * 一開始幾乎不動，快到判定時才一下子合起來。
 */
function touchClose(remain: number): number {
  const distance = Math.min(0.3, Math.max(0, (-Math.exp(-3.44 * remain - 0.85) + 0.42) * 0.75));
  return 1 - distance / 0.3;
}

/**
 * 指派標籤在起飛後、飛行的前 15% 淡入。
 * 放大期不顯示，避免內圈同時放大的多顆音符，標籤全擠在盤面中心看不出誰是誰。
 */
const TAG_FADE_PORTION = 0.15;

export interface NoteVisual {
  note: Note;
  visible: boolean;
  phase: NotePhase;
  /**
   * 飛行進度 0–1（內圈 → 鍵位）；原地放大期間是 0。
   * Touch 家族是四片三角形的收攏進度（0 = 剛出現，1 = 合起來）。純視覺，不影響判定時間。
   */
  travel: number;
  /** 原地放大的進度 0–1，也就是大小倍率；起飛後固定 1。Touch、關掉飛入、全部同時顯示都是 1。 */
  grow: number;
  /** Hold 尾端（結束時間那一點）的飛行進度 0–1，和頭用同樣的速度。 */
  tailTravel: number;
  opacity: number;
  /** Slide 軌道的透明度：判定前先淡入到一半，快到判定時才完全不透明。 */
  trackOpacity: number;
  /** 指派標籤的透明度：放大期不顯示，起飛後才淡入；會再乘上整顆音符的 opacity。 */
  tagOpacity: number;
  /** Hold／Slide 的完成比例 0–1 */
  progress: number;
  /** Slide 軌道上的移動比例 0–1（依 motionStart／motionEnd 線性映射） */
  slideU: number;
}

const FADE_OUT = 0.4;

/**
 * 起點星星飛過來的時候會轉（MajdataPlay：每秒 180° ÷ 滑行時間，滑得越快轉得越快）。
 * 回傳相對於「一個角朝盤面外側」的轉角（弧度）；判定時間剛好轉回 0，之後換成軌道上的星星。
 * 角度只由播放時間換算；沒有飛入動畫時呼叫端不要用。
 */
export function starSpin(note: Note, time: number): number {
  if (note.motionStart === null || note.motionEnd === null) return 0;
  const slide = Math.max(note.motionEnd - note.motionStart, 0.15);
  return (Math.PI * (time - note.timeSeconds)) / slide;
}

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/**
 * approachSeconds 是 Tap／Hold／Slide 的飛入時間（飛行期）；
 * touchSeconds 是 Touch 從出現到判定的整段時間（淡入＋收攏），遊戲裡和 Tap 是分開的速度設定（見 noteSpeed.ts）。
 */
export function noteVisual(
  note: Note,
  time: number,
  mode: 'window' | 'all',
  approach: boolean,
  approachSeconds: number,
  touchSeconds: number,
): NoteVisual {
  const start = note.timeSeconds;
  const end = Math.max(note.endSeconds, note.timeSeconds);
  const flight = Math.max(approachSeconds, 1e-6);
  const phase: NotePhase = time < start ? 'upcoming' : time <= end ? 'active' : 'done';
  const span = end - start;
  const progress = span > 0 ? clamp01((time - start) / span) : phase === 'upcoming' ? 0 : 1;

  const motionStart = note.motionStart ?? start;
  const motionEnd = note.motionEnd ?? end;
  const motionSpan = motionEnd - motionStart;
  const slideU = motionSpan > 0 ? clamp01((time - motionStart) / motionSpan) : time >= motionEnd ? 1 : 0;

  // 兩段式（原地放大 → 飛行）只用在 Tap、Hold、Slide 的起點；Touch 維持原地收攏。
  // 沒有起點的 Slide 沒有星星可以放大或飛，改成跟著軌道一起淡入。
  const headless = note.kind === 'slide' && !note.hasHead;
  const animated = mode === 'window' && approach;
  const touchStage = animated && isTouchKind(note);
  const twoStage = animated && !isTouchKind(note) && !headless;
  const followTrack = animated && headless;

  // Hold 尾端：結束時間那一點以同樣速度飛行，還沒起飛就停在內圈。
  // 不是兩段式時，判定前顯示整段時值的長度（從判定時間起算）。
  const tailTime = twoStage ? time : Math.max(time, start);
  const tailTravel = clamp01((tailTime - (end - flight)) / flight);

  if (mode === 'all') {
    const opacity = phase === 'active' ? 1 : phase === 'upcoming' ? 0.45 : 0.28;
    return {
      note,
      visible: true,
      phase,
      travel: 1,
      grow: 1,
      tailTravel,
      opacity,
      trackOpacity: opacity,
      tagOpacity: 1,
      progress,
      slideU,
    };
  }

  let appear: number;
  let travel: number;
  let grow = 1;
  let opacity = 1;

  if (twoStage) {
    const takeoff = start - flight;
    const growSeconds = flight * GROW_RATIO;
    appear = takeoff - growSeconds;
    grow = clamp01((time - appear) / growSeconds);
    travel = clamp01((time - takeoff) / flight);
    // 放大本身就是進場，判定前不再另外淡入。
    if (phase === 'done') opacity = Math.max(0, 1 - (time - end) / FADE_OUT);
  } else if (followTrack) {
    // 和軌道同時出現；標籤直接在定位，透明度交給 trackOpacity／tagOpacity。
    appear = start - flight * SLIDE_FADE_RATIO;
    travel = 1;
    if (phase === 'done') opacity = Math.max(0, 1 - (time - end) / FADE_OUT);
  } else if (touchStage) {
    // Touch 有自己的進場時間：照遊戲，短短一段淡入之後加速收攏。
    const whole = Math.max(touchSeconds, 1e-6);
    const fadeSeconds = whole * TOUCH_FADE_PORTION;
    appear = start - whole;
    travel = touchClose(clamp01((start - time) / (whole - fadeSeconds)));
    if (phase === 'upcoming') opacity = clamp01((time - appear) / fadeSeconds);
    else if (phase === 'done') opacity = Math.max(0, 1 - (time - end) / FADE_OUT);
  } else {
    appear = start - approachSeconds;
    travel = approach ? clamp01((time - appear) / flight) : time >= appear ? 1 : 0;
    if (phase === 'upcoming') opacity = 0.3 + 0.7 * travel;
    else if (phase === 'done') opacity = Math.max(0, 1 - (time - end) / FADE_OUT);
  }

  let trackOpacity = opacity;
  let tagOpacity = 1;
  if (phase === 'upcoming') {
    if (twoStage) {
      trackOpacity = slideTrackFade(time, start, flight);
      tagOpacity = clamp01(travel / TAG_FADE_PORTION);
    } else if (followTrack) {
      trackOpacity = slideTrackFade(time, start, flight);
      tagOpacity = clamp01(trackOpacity * 2);
    }
  }

  return {
    note,
    visible: time >= appear && time <= end + FADE_OUT,
    phase,
    travel,
    grow,
    tailTravel,
    opacity,
    trackOpacity,
    tagOpacity,
    progress,
    slideU,
  };
}

function isTouchKind(note: Note): boolean {
  return note.kind === 'touch' || note.kind === 'touchHold';
}

/**
 * 判定前的 Slide 軌道透明度（時間點照 MajdataPlay SlideDrop）：
 * 判定前 SLIDE_FADE_RATIO 倍飛行時間開始淡入，最多 0.2 秒升到 0.5，維持 0.5，
 * 判定前 0.05 秒完全不透明。兩段變化都是緩入緩出；後面那段不是一格跳上去，
 * 而是提前 SLIDE_SOLID_RAMP 秒開始漸漸亮滿。整段太短時兩段各佔一半，不會重疊。
 */
function slideTrackFade(time: number, start: number, flight: number): number {
  const fadeStart = start - flight * SLIDE_FADE_RATIO;
  const solidAt = start - SLIDE_SOLID_BEFORE;
  if (time <= fadeStart) return 0;
  if (time >= solidAt) return 1;
  const span = Math.max(solidAt - fadeStart, 1e-6);
  const fadeIn = Math.min(SLIDE_FADE_IN, span / 2);
  const ramp = Math.min(SLIDE_SOLID_RAMP, span / 2);
  const rise = easeInOut(clamp01((time - fadeStart) / fadeIn));
  const solid = easeInOut(clamp01((time - (solidAt - ramp)) / ramp));
  return 0.5 * rise + 0.5 * solid;
}

/** 緩入緩出（smoothstep）：頭尾斜率為 0。 */
function easeInOut(x: number): number {
  return x * x * (3 - 2 * x);
}

/**
 * 頭的半徑倍率：起飛前停在內圈（APPROACH_INNER），飛行時等速往外，判定時間之後固定為 1（Rust 座標原位）。
 * Touch 家族固定 1：它們不從盤面中心飛出，而是原地由外往內收攏，見 touchGather。
 */
export function approachScale(visual: NoteVisual): number {
  if (isTouchKind(visual.note)) return 1;
  return APPROACH_INNER + (1 - APPROACH_INNER) * visual.travel;
}

/** Hold 尾端的半徑倍率：和頭同樣從內圈飛到鍵位，結束時間剛好追上頭。 */
export function tailScale(visual: NoteVisual): number {
  return APPROACH_INNER + (1 - APPROACH_INNER) * visual.tailTravel;
}

/**
 * Touch 的收攏進度：0 = 剛出現的位置，1 = 收攏到位，剛好在判定時間合起來。
 * 有進場動畫時走遊戲的曲線（先淡入、慢慢靠近、最後一下子合起來），見 noteVisual。
 */
export function touchGather(visual: NoteVisual): number {
  if (visual.phase !== 'upcoming') return 1;
  return Math.min(1, Math.max(0, visual.travel));
}

export function scalePoint(point: Point, factor: number): Point {
  return { x: point.x * factor, y: point.y * factor };
}
