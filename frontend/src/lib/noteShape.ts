import { toStage, type Calibration } from './disc';
import type { Point } from './types';

/*
 * 音符外形的尺寸與輪廓，量自遊戲畫面。App 的盤面（DiscStage）和文件站的演示共用這一份。
 * 半徑是盤面單位（鍵位半徑為 1）；結尾是 _PX 的是 viewBox 像素的筆寬。
 * 只決定「長什麼樣」，位置與時間一律來自 Rust。
 */

/*
 * Tap 與 Hold 的大小和粗細：照使用者 2026-11-27 給的遊戲截圖，用程式量徑向剖面（半高處當邊緣）。
 * 那張截圖的盤面半徑 442.3 px；兩顆已長到全尺寸的 Tap 剖面完全一樣：
 *   外緣 47.1 px（0.1065）、洞的邊緣 28.3 px（0.064），環寬 18.8 px（0.0425），內外白邊各約 3.15 px。
 * Hold：截圖裡那條正被按住，橫切面框的中線離軸 38.4 px（0.0869，和 Tap 環帶的中心半徑幾乎一樣）、
 *   框寬 21.9 px（0.0495），比 Tap 多 3.1 px，邊緣是柔的（按住時的發光）。所以 Hold 平常和 Tap 一樣粗，
 *   判定中（按住）加粗 BAND_EMPHASIS_PX，剛好是截圖裡按住的樣子。
 * App 預設校準的盤面半徑是 480 px，下面的 _PX 都是照這個換算的。
 * （2026-11-25 一度因為「比別的模擬器粗」改細到 13 px，看了截圖才知道遊戲本來就這麼粗，照截圖改回來。）
 */

/** Tap 環帶的中心半徑（盤面單位）：外緣 0.1065、洞的邊緣 0.064 的中間；也是 Hold 長條的半寬，兩者外緣對齊。 */
export const NOTE_RADIUS = 0.0852;

/** Tap 正中心的小點（實心）；Hold 尾端也用同一顆。 */
export const NOTE_DOT = 0.016;

/** 星星正中心的小點：星形孔比 Tap 的洞小，點也小一號。 */
export const STAR_DOT = 0.013;

/** 環帶的寬度（Tap 與 Hold 相同；0.0425 × 480）。 */
export const BAND_PX = 20.4;

/** 判定中（Hold 按住）或選中時，環帶加粗多少：截圖裡按住的 Hold 比 Tap 粗 0.0071（× 480）。 */
export const BAND_EMPHASIS_PX = 3.4;

/** 環帶兩側深色描邊各露出多寬；音符疊在箭頭或別的音符上時靠它分開。 */
export const EDGE_PX = 2;

/** 照遊戲上色時，環帶外緣那一圈白邊的寬度（截圖約 3.15 px，換算成 480 是 3.4）。 */
export const RIM_PX = 3.4;

/**
 * EX：音符底下墊一圈同形狀的筆觸，整圈再模糊，變成貼著音符外緣的柔和光暈
 * （照遊戲的樣子，量自使用者 2026-11-23 給的截圖）。光暈只往外：洞裡面是乾淨的。
 * 那一筆從環帶的中線畫到外緣外面 SPREAD，內側整段壓在環帶底下，模糊之後也滲不進洞裡。
 * 量到的樣子：亮的那一段到外緣外 7px 左右，再用 2.8px 的模糊淡到沒有；最亮處約 0.88。
 */
export const EX_GLOW_SPREAD_PX = 7;
export const EX_GLOW_BLUR_PX = 2.8;

/** 環帶筆寬：平常、或判定中／選中。 */
export function bandWidth(emphasis: boolean): number {
  return emphasis ? BAND_PX + BAND_EMPHASIS_PX : BAND_PX;
}

/** 深色描邊那一層的筆寬（墊在環帶下面，兩側各露出 EDGE_PX）。 */
export function edgeWidth(band: number): number {
  return band + EDGE_PX * 2;
}

/** 白邊的中心離環帶中心多遠（像素）：貼齊環帶外緣、往內一圈。 */
export function rimOffset(band: number): number {
  return (band - RIM_PX) / 2;
}

/** EX 光暈那一筆的筆寬（模糊之前）：環帶的外半邊，加上外面的 SPREAD。 */
export function exGlowWidth(band: number): number {
  return band / 2 + EX_GLOW_SPREAD_PX;
}

/** EX 光暈那一筆的中心離環帶中線多遠（像素，往外）。 */
export function exGlowOffset(band: number): number {
  return exGlowWidth(band) / 2;
}

/** 照遊戲上色時，環帶內緣（洞的邊緣）也有一圈白邊；截圖裡和外緣一樣寬。 */
export const RIM_INNER_PX = 3.4;

/** 內緣白邊的中心離環帶中心多遠（像素，往內是負的）。 */
export function rimInnerOffset(band: number): number {
  return -(band - RIM_INNER_PX) / 2;
}

/*
 * 照遊戲上色時的明暗。遊戲的環帶不是一個顏色塗滿：
 * Tap 一圈切成四段，每一段順時針由深到淺，分界落在「盤面中心 → 音符」的方向上（量自遊戲畫面）。
 * 深淺是連續的漸層（使用者 2026-11-23 要求，不要一階一階的色塊）：每一段各畫一條弧，
 * 筆觸用線性漸層，從那一段的起點漸到終點。
 */

/** 每一段開頭有一小段維持最深的顏色，之後才均勻漸到最淺（量自同一張截圖：約前 17° ／ 90°）。 */
export const SHADE_FLAT = 0.17;

/** 環帶的一段：弧的路徑與漸層的起訖點，都以音符中心為原點、分界朝 +x（畫的時候整組再轉到分界的方向）。 */
export interface RingQuarter {
  d: string;
  from: Point;
  to: Point;
}

/** Tap 環帶的四段。radiusPx 是環帶中心半徑（像素）。y 向下，所以角度增加就是順時針。 */
export function ringQuarters(radiusPx: number): RingQuarter[] {
  const corner = (index: number): Point => {
    const theta = (index * Math.PI) / 2;
    return { x: Math.cos(theta) * radiusPx, y: Math.sin(theta) * radiusPx };
  };
  const r = radiusPx.toFixed(2);
  return Array.from({ length: 4 }, (_, index) => {
    const from = corner(index);
    const to = corner(index + 1);
    return {
      d: `M${from.x.toFixed(2)} ${from.y.toFixed(2)}A${r} ${r} 0 0 1 ${to.x.toFixed(2)} ${to.y.toFixed(2)}`,
      from,
      to,
    };
  });
}

/** 多邊形外框（Hold 的六邊形長條）的明暗：每一條邊各自由深到淺，回傳每一條邊的起訖點。 */
export function outlineEdges(points: Point[]): { from: Point; to: Point }[] {
  return points.map((from, index) => ({ from, to: points[(index + 1) % points.length] }));
}

/** 星星外角的半徑。 */
export const STAR_OUTER = 0.122;

/** 內凹頂點的半徑是外角的幾倍；遊戲的星星比正五角星（0.38）胖。 */
export const STAR_INNER_RATIO = 0.5;

/** 星星中間挖掉的星形孔，外角半徑是整顆的幾倍（量自遊戲畫面；2026-11-25 一度改成 0.6，11-27 跟著 Tap 改回）。 */
export const STAR_HOLE_RATIO = 0.48;

/**
 * 星形每條邊離中心的距離，是外角半徑的幾倍。
 * 同心縮放的星形，對應的邊互相平行，所以環帶的寬度處處相同＝這個倍數 × 兩個外角半徑的差。
 */
const STAR_EDGE =
  (STAR_INNER_RATIO * Math.sin(Math.PI / 5)) /
  Math.sqrt(1 + STAR_INNER_RATIO * STAR_INNER_RATIO - 2 * STAR_INNER_RATIO * Math.cos(Math.PI / 5));

/** 上色時白邊在外、顏色在內：顏色那一層的外角半徑（盤面單位），radiusPx 是盤面半徑的像素。 */
export function starFillOuter(radiusPx: number): number {
  return STAR_OUTER - RIM_PX / (STAR_EDGE * radiusPx);
}

/** 上色時星形孔的邊緣也有一圈白邊：顏色那一層的孔要大一圈（外角半徑，盤面單位）。 */
export function starFillHole(radiusPx: number): number {
  return STAR_OUTER * STAR_HOLE_RATIO + RIM_INNER_PX / (STAR_EDGE * radiusPx);
}

/** 星星深色描邊的筆寬（一半壓在星星底下，露出來的是另一半）。 */
export const STAR_EDGE_PX = 3;

/** 星星最外面那一圈輪廓（不含中間的孔）：EX 的光暈沿著它畫。 */
export function starOutlinePath(calibration: Calibration, at: Point, angle: number, outer = STAR_OUTER): string {
  return starLoop(calibration, at, outer, angle);
}

function starLoop(calibration: Calibration, at: Point, outer: number, angle: number): string {
  let d = '';
  for (let i = 0; i < 10; i += 1) {
    const r = i % 2 === 0 ? outer : outer * STAR_INNER_RATIO;
    const theta = angle + (i * Math.PI) / 5;
    const p = toStage(calibration, { x: at.x + Math.cos(theta) * r, y: at.y + Math.sin(theta) * r });
    d += `${i === 0 ? 'M' : 'L'}${p.x.toFixed(2)} ${p.y.toFixed(2)}`;
  }
  return `${d}Z`;
}

/**
 * 星形環帶：一顆星中間挖掉一顆小星（要配 fill-rule: evenodd）。
 * angle 是其中一個外角指向的方向（弧度）：起點星星朝盤面外側，滑行中的星星朝前進方向。
 * hole 是星形孔的外角半徑；預設照整顆的比例。
 */
export function starBandPath(
  calibration: Calibration,
  at: Point,
  angle: number,
  outer = STAR_OUTER,
  hole = STAR_OUTER * STAR_HOLE_RATIO,
): string {
  return starLoop(calibration, at, outer, angle) + starLoop(calibration, at, hole, angle);
}

/** 六邊形帽子：肩膀離帽子中心的距離是半寬的幾倍（正六邊形，尖端朝飛行方向）。 */
const HOLD_CAP = Math.tan(Math.PI / 6);

/**
 * Hold 長條的輪廓：兩條長邊，兩端各收成正六邊形的尖角，整條是一圈連續的空心外框。
 * head／tail 是頭、尾的半徑倍率（notes.ts 的 approachScale／tailScale），
 * 兩者是兩端六邊形帽子的中心（照 MajdataPlay HoldDrop 的「+ 一顆音符長」），
 * 所以長條比頭尾的距離多出一顆音符的長度；時值很短或還在原地放大時是一個完整的正六邊形，
 * 長寬比和遊戲相同（長是寬的 1.155 倍）。
 * halfWidth 換成別的值就是整圈平行往外（或往內）移，形狀不變：白邊用。
 */
export function holdOutline(
  position: Point,
  head: number,
  tail: number,
  halfWidth = NOTE_RADIUS,
): Point[] {
  const headAt = { x: position.x * head, y: position.y * head };
  const tailAt = { x: position.x * tail, y: position.y * tail };
  // 由盤面中心指向鍵位的單位向量；長邊沿它、肩寬沿它的垂直方向。
  const base = Math.hypot(position.x, position.y) || 1;
  const ux = position.x / base;
  const uy = position.y / base;
  const w = halfWidth;
  const s = halfWidth * HOLD_CAP;
  const apex = s * 2;
  return [
    { x: headAt.x + ux * apex, y: headAt.y + uy * apex },
    { x: headAt.x + ux * s - uy * w, y: headAt.y + uy * s + ux * w },
    { x: tailAt.x - ux * s - uy * w, y: tailAt.y - uy * s + ux * w },
    { x: tailAt.x - ux * apex, y: tailAt.y - uy * apex },
    { x: tailAt.x - ux * s + uy * w, y: tailAt.y - uy * s - ux * w },
    { x: headAt.x + ux * s + uy * w, y: headAt.y + uy * s - ux * w },
  ];
}
