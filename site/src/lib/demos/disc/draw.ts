/**
 * 文件站盤面演示共用的繪製函式：照 App 的 DiscStage.svelte 搬過來，
 * 校準固定用 App 的預設值（DEFAULT_CALIBRATION），不縮放。
 * 只負責「畫在哪裡、長什麼樣」，不計算左右手；手與音符的資料一律來自 Rust 核心。
 */
import {
  DEFAULT_CALIBRATION,
  IMAGE_HEIGHT,
  IMAGE_WIDTH,
  centerPx,
  chevronFan,
  chevronStrip,
  directionOnSamples,
  eachArcPath,
  firstRemaining,
  pointOnSamples,
  polylinePath,
  radiusPx,
  toStage,
  toStageLength,
  type ChevronStrip,
} from '$app/lib/disc';
import { NOTE_RADIUS, holdOutline, starBandPath, starOutlinePath } from '$app/lib/noteShape';
import {
  TOUCH_DOT,
  TOUCH_DOT_RING,
  TOUCH_FAN_HOLE,
  TOUCH_HOLD_BAND,
  fanLoop,
  touchHoldBorder,
  touchJustBorder,
  touchPolygon,
  type TouchFan,
} from '$app/lib/touch';
import { palmLabelAnchor, palmLabelUpward } from '$app/lib/palm';
import type { PalmPlacement, PathSample, Point, SlidePath } from '$app/lib/types';

// 起點星星的轉角、Touch Hold 外框亮到哪裡、落點參考標記的大小，都和 App 共用同一份。
export { starSpin } from '$app/lib/notes';
export { touchHoldBorderFill as borderFill, sensorMarkRadius as sensorRadius } from '$app/lib/touch';

// 音符外形的尺寸與 App 共用同一份（frontend/src/lib/noteShape.ts）。
export {
  EX_GLOW_BLUR_PX,
  EX_GLOW_SPREAD_PX,
  NOTE_DOT,
  NOTE_RADIUS,
  STAR_DOT,
  bandWidth,
  edgeWidth,
  exGlowOffset,
  exGlowWidth,
} from '$app/lib/noteShape';

export const CALIBRATION = DEFAULT_CALIBRATION;
export const CENTER = centerPx(CALIBRATION);
export const DISC_RADIUS = radiusPx(CALIBRATION);

// 盤面圓幾乎佔滿畫布，額外留白避免標記與文字被裁掉（同 DiscStage）。
export const MARGIN_X = 90;
export const MARGIN_TOP = 105;
export const MARGIN_BOTTOM = 105;
export const VIEW_BOX = `${-MARGIN_X} ${-MARGIN_TOP} ${IMAGE_WIDTH + MARGIN_X * 2} ${
  IMAGE_HEIGHT + MARGIN_TOP + MARGIN_BOTTOM
}`;

/** 盤面座標 → viewBox 像素。 */
export function px(point: Point): Point {
  return toStage(CALIBRATION, point);
}

/** 盤面長度 → viewBox 像素長度。 */
export function len(value: number): number {
  return toStageLength(CALIBRATION, value);
}

export function linePath(points: Point[]): string {
  return polylinePath(points, CALIBRATION);
}

/** 盤面座標的封閉多邊形 → 畫面路徑，Touch 的三角形與方框共用。 */
export function closedPath(points: Point[]): string {
  return `${linePath(points)}Z`;
}

/** Touch 外形：A／D 四邊、B／E 三角、C 八邊，朝向以落點的半徑方向為基準。 */
export function shapePath(area: string | null, at: Point, radius: number, angle: number): string {
  return closedPath(touchPolygon(area, at, radius, angle));
}

/** 星形環帶（同 DiscStage.starBand）：中間挖掉一顆小星；angle 是其中一個外角指的方向。 */
export function starBand(at: Point, angle: number): string {
  return starBandPath(CALIBRATION, at, angle);
}

/** 星星最外面那一圈輪廓（不含孔）：EX 的光暈沿著它畫。 */
export function starOutline(at: Point, angle: number): string {
  return starOutlinePath(CALIBRATION, at, angle);
}

/** 軌道上的星星（同 DiscStage.starOnPath）：位置取路徑取樣點，一個角朝前進方向。 */
export function starOnPath(samples: PathSample[], u: number): string {
  return starBand(pointOnSamples(samples, u), directionOnSamples(samples, u));
}

const strips = new WeakMap<PathSample[], ChevronStrip>();

function stripFor(samples: PathSample[]): ChevronStrip {
  let strip = strips.get(samples);
  if (!strip) {
    strip = chevronStrip(samples, CALIBRATION);
    strips.set(samples, strip);
  }
  return strip;
}

/**
 * 一條箭頭帶：每個箭頭兩層（strip.unders[i] 是黑色的外框＋往後錯開的陰影，strip.pieces[i] 是箭頭本身），
 * 從 first 開始畫（星星走過的不畫）。
 */
export interface SlideTrack {
  strip: ChevronStrip;
  first: number;
}

function trackLayers(strip: ChevronStrip, slideU: number): SlideTrack {
  return { strip, first: firstRemaining(strip, slideU) };
}

/**
 * Slide 的箭頭帶（同 DiscStage 的 slide-path）：每條路徑只算一次，之後依 slideU 取還沒走過的那段。
 * WiFi（剛好兩條側線）是一整面扇形；其餘形狀每條側線各一條箭頭帶，最後是中央那條。
 * 每個箭頭兩層（slide-under 在下、slide-chevrons 在上），一個一個交錯疊上去，
 * 疊在別條軌道上、或同一條軌道繞回來交疊時，箭頭一個一個分得開。
 */
export function slideTracks(path: SlidePath, slideU: number): SlideTrack[] {
  if (path.branches.length === 2) {
    let strip = strips.get(path.samples);
    if (!strip) {
      strip = chevronFan(path.samples, path.branches[0], path.branches[1], CALIBRATION);
      strips.set(path.samples, strip);
    }
    return [trackLayers(strip, slideU)];
  }
  return [...path.branches, path.samples].map((samples) => trackLayers(stripFor(samples), slideU));
}

// ---- Touch（同 DiscStage 的 touch-fans／touch-just／touch-hold-border） ----

/** 幾個封閉輪廓接成一條路徑（配 fill-rule: evenodd 就是挖洞）。 */
function loopsPath(loops: Point[][]): string {
  return loops.map((loop) => closedPath(loop)).join('');
}

/** 一片三角形：外輪廓加上中間挖掉的小三角形（演示不上色，Touch Hold 也是挖空的）。 */
export function fanBody(at: Point, fan: TouchFan): string {
  return loopsPath([fanLoop(at, fan), fanLoop(at, fan, TOUCH_FAN_HOLE)]);
}

/** Touch 收到位那一刻亮起的圓角方框。 */
export function touchJustPath(at: Point, size: number): string {
  return closedPath(touchJustBorder(at, size));
}

/** Touch Hold 的圓角菱形外框，四段（從正上方順時針），各自一條路徑。 */
export function touchHoldQuarters(at: Point, size: number): string[] {
  return touchHoldBorder(at, size).map((quarter) => linePath(quarter));
}

/**
 * Touch Hold 外框底下那層深色描邊，兩側各露出多寬（像素）。
 * App 的 DiscStage.HOLD_BAND_EDGE_PX 是元件內的常數、沒有匯出，這裡照抄同一個值。
 */
const HOLD_BAND_EDGE_PX = 2;

/** Touch Hold 外框的筆寬（像素）：亮起來的那層，和墊在底下、兩側各寬一點的深色那層。 */
export function touchHoldBand(size: number): { fill: number; edge: number } {
  const fill = len(size * TOUCH_HOLD_BAND);
  return { fill, edge: fill + HOLD_BAND_EDGE_PX * 2 };
}

/** 落點正中心的圓點（像素半徑）：外面那圈白邊，和裡面跟著音符顏色的那顆。 */
export const TOUCH_DOT_RIM_RADIUS = len(TOUCH_DOT + TOUCH_DOT_RING);
export const TOUCH_DOT_RADIUS = len(TOUCH_DOT);

/** each 連線（同 DiscStage）：同時的兩顆外圈音符之間一段細圓弧，半徑跟著音符一起飛。 */
export function eachArc(a: Point, b: Point, factor: number): string {
  return eachArcPath(CALIBRATION, a, b, factor);
}

/**
 * Hold 長條的輪廓（同 DiscStage.holdPath）：頭在 head 倍的鍵位上，尾在 tail 倍的鍵位上。
 * 演示裡頭、尾倍率取 notes.ts 的 approachScale／tailScale；靜態圖例直接給兩個倍率。
 * offset 是整圈平行外移多少像素（EX 的光暈用）。
 */
export function holdPath(position: Point, head: number, tail: number, offset = 0): string {
  return closedPath(holdOutline(position, head, tail, NOTE_RADIUS + offset / DISC_RADIUS));
}

/** 換手菱形，以 (0,0) 為中心，再用 transform 移到位置。 */
export function diamondPath(): string {
  const r = len(0.06);
  return `M0 ${-r} L${r} 0 L0 ${r} L${-r} 0 Z`;
}

/** 掌心十字：中間留空，手標記蓋上去之後四個端點仍看得見。 */
export function palmCrossPath(at: Point): string {
  const tickIn = len(0.13);
  const tickOut = len(0.21);
  return `M${at.x + tickIn} ${at.y} H${at.x + tickOut} M${at.x - tickIn} ${at.y} H${at.x - tickOut} M${at.x} ${at.y + tickIn} V${at.y + tickOut} M${at.x} ${at.y - tickIn} V${at.y - tickOut}`;
}

const PALM_TITLE_SIZE = 27;
const PALM_LIST_SIZE = 23;

/** 手掌標籤的兩行基線位置（同 DiscStage.palmLabel），夾回 viewBox 之內。 */
export function palmLabel(placement: PalmPlacement): { x: number; title: number; list: number } {
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
  const x = Math.min(Math.max(anchor.x, -MARGIN_X + 110), IMAGE_WIDTH + MARGIN_X - 110);
  return { x, title, list };
}
