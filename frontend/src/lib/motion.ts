import type { MotionMode, MotionSegment, Point } from './types';

export interface HandState {
  point: Point;
  mode: MotionMode;
  noteId: string | null;
  contacting: boolean;
  /** 播放時間仍在資料範圍之外時為 true（例如負預備時間之前）。 */
  clamped: boolean;
}

const CONTACT_MODES = new Set<string>(['tap', 'hold', 'slide', 'handover', 'glide']);

export function isContactMode(mode: string): boolean {
  return CONTACT_MODES.has(mode);
}

/** 找出最後一個 startSeconds <= t 的索引；全部都比 t 大時回傳 0。 */
function lastAtOrBefore(values: number[], t: number): number {
  let lo = 0;
  let hi = values.length - 1;
  let found = 0;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (values[mid] <= t) {
      found = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found;
}

function interpolate(segment: MotionSegment, t: number): Point {
  const samples = segment.samples;
  if (samples.length === 0) return { x: 0, y: 0 };
  const first = samples[0];
  const last = samples[samples.length - 1];
  if (t <= first.timeSeconds) return { x: first.x, y: first.y };
  if (t >= last.timeSeconds) return { x: last.x, y: last.y };
  let lo = 0;
  let hi = samples.length - 1;
  while (lo + 1 < hi) {
    const mid = (lo + hi) >> 1;
    if (samples[mid].timeSeconds <= t) lo = mid;
    else hi = mid;
  }
  const a = samples[lo];
  const b = samples[hi];
  const span = b.timeSeconds - a.timeSeconds;
  // 契約允許重複時間，但重複時間的座標必須相同，直接取後者即可。
  if (span <= 0) return { x: b.x, y: b.y };
  const u = (t - a.timeSeconds) / span;
  return { x: a.x + (b.x - a.x) * u, y: a.y + (b.y - a.y) * u };
}

/**
 * 依絕對播放時間查詢手的狀態。
 * 一律從 segments 直接查詢與插值，不做每幀位置累加，因此 seek 與播放結果完全一致。
 */
export function sampleSegments(segments: MotionSegment[], t: number): HandState | null {
  if (segments.length === 0) return null;
  const starts = segments.map((segment) => segment.startSeconds);
  return sampleWithStarts(segments, starts, t);
}

export function sampleWithStarts(
  segments: MotionSegment[],
  starts: number[],
  t: number,
): HandState | null {
  if (segments.length === 0) return null;
  const first = segments[0];
  const last = segments[segments.length - 1];
  if (t < first.startSeconds) {
    const point = interpolate(first, first.startSeconds);
    return { point, mode: 'idle', noteId: null, contacting: false, clamped: true };
  }
  if (t > last.endSeconds) {
    const point = interpolate(last, last.endSeconds);
    return { point, mode: 'idle', noteId: null, contacting: false, clamped: true };
  }
  const index = lastAtOrBefore(starts, t);
  const segment = segments[index];
  const mode = segment.mode as MotionMode;
  return {
    point: interpolate(segment, t),
    mode,
    noteId: segment.noteId,
    contacting: isContactMode(segment.mode),
    clamped: false,
  };
}

export interface Track {
  times: number[];
  points: Point[];
  segmentStarts: number[];
}

/** 把一隻手的所有動作段攤平成一條連續折線，供軌跡繪製使用。 */
export function buildTrack(segments: MotionSegment[]): Track {
  const times: number[] = [];
  const points: Point[] = [];
  for (const segment of segments) {
    for (const sample of segment.samples) {
      const lastTime = times[times.length - 1];
      const lastPoint = points[points.length - 1];
      if (
        lastPoint !== undefined &&
        lastTime === sample.timeSeconds &&
        lastPoint.x === sample.x &&
        lastPoint.y === sample.y
      ) {
        continue;
      }
      times.push(sample.timeSeconds);
      points.push({ x: sample.x, y: sample.y });
    }
  }
  return { times, points, segmentStarts: segments.map((segment) => segment.startSeconds) };
}

function pointOnTrack(track: Track, t: number): Point {
  const { times, points } = track;
  if (points.length === 0) return { x: 0, y: 0 };
  if (t <= times[0]) return points[0];
  const lastIndex = points.length - 1;
  if (t >= times[lastIndex]) return points[lastIndex];
  const i = lastAtOrBefore(times, t);
  const j = Math.min(i + 1, lastIndex);
  const span = times[j] - times[i];
  if (span <= 0) return points[j];
  const u = (t - times[i]) / span;
  return { x: points[i].x + (points[j].x - points[i].x) * u, y: points[i].y + (points[j].y - points[i].y) * u };
}

/** 取出 [from, to] 區間的軌跡折線（含端點插值），用於「最近軌跡」。 */
export function trackSlice(track: Track, from: number, to: number): Point[] {
  const { times, points } = track;
  if (points.length === 0 || to <= from) return [];
  const result: Point[] = [pointOnTrack(track, from)];
  let index = lastAtOrBefore(times, from);
  while (index < times.length && times[index] <= from) index += 1;
  for (; index < times.length && times[index] < to; index += 1) {
    result.push(points[index]);
  }
  result.push(pointOnTrack(track, to));
  return result;
}

export function trackBounds(segments: MotionSegment[]): { start: number; end: number } | null {
  if (segments.length === 0) return null;
  return {
    start: segments[0].startSeconds,
    end: segments[segments.length - 1].endSeconds,
  };
}
