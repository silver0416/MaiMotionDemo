// 逐格讀影片：用一個不顯示的 <video>，每取到一格就暫停、交給呼叫端處理，再繼續播放。
// requestVideoFrameCallback 給的 mediaTime 是該格的精確影片時間。

import type { RgbaFrame } from './pose';

/** 分析時的畫面寬度上限；較大的影片先縮小，加快偵測與追蹤。 */
const MAX_WIDTH = 1280;

type FrameCallback = (now: number, meta: { mediaTime: number }) => void;
type VideoWithFrames = HTMLVideoElement & {
  requestVideoFrameCallback(callback: FrameCallback): number;
  cancelVideoFrameCallback(handle: number): void;
};

export function supportsFrameCallback(): boolean {
  return typeof HTMLVideoElement !== 'undefined' && 'requestVideoFrameCallback' in HTMLVideoElement.prototype;
}

export class FrameReader {
  #video: VideoWithFrames;
  #canvas: HTMLCanvasElement;
  #context: CanvasRenderingContext2D;
  width = 0;
  height = 0;
  /** 分析畫面像素 / 影片原始像素 */
  scale = 1;
  duration = 0;

  constructor(url: string) {
    const video = document.createElement('video') as VideoWithFrames;
    video.crossOrigin = 'anonymous';
    video.muted = true;
    video.playsInline = true;
    video.preload = 'auto';
    video.src = url;
    // 不在文件裡的 <video> 不會合成畫面，requestVideoFrameCallback 就不會觸發；
    // 放進文件、縮成 1×1 擺在角落（不可 display:none 或完全透明，否則一樣不合成）。
    video.setAttribute('aria-hidden', 'true');
    video.style.cssText = 'position:fixed;left:0;bottom:0;width:1px;height:1px;pointer-events:none;z-index:-1;';
    document.body.appendChild(video);
    this.#video = video;
    this.#canvas = document.createElement('canvas');
    const context = this.#canvas.getContext('2d', { willReadFrequently: true });
    if (!context) throw new Error('無法建立畫布');
    this.#context = context;
  }

  async open(): Promise<void> {
    const video = this.#video;
    if (video.readyState < 1) {
      await new Promise<void>((resolve, reject) => {
        video.addEventListener('loadedmetadata', () => resolve(), { once: true });
        video.addEventListener('error', () => reject(new Error('影片無法讀取')), { once: true });
      });
    }
    this.scale = Math.min(1, MAX_WIDTH / video.videoWidth);
    this.width = Math.round(video.videoWidth * this.scale);
    this.height = Math.round(video.videoHeight * this.scale);
    this.duration = video.duration;
    this.#canvas.width = this.width;
    this.#canvas.height = this.height;
  }

  #grab(): RgbaFrame {
    this.#context.drawImage(this.#video, 0, 0, this.width, this.height);
    const image = this.#context.getImageData(0, 0, this.width, this.height);
    return { data: image.data, width: image.width, height: image.height };
  }

  /** 跳到 time 並取那一格。 */
  async seek(time: number, _signal?: AbortSignal): Promise<{ time: number; frame: RgbaFrame }> {
    const video = this.#video;
    video.pause();
    const presented = new Promise<number>((resolve) => {
      video.requestVideoFrameCallback((_now, meta) => resolve(meta.mediaTime));
    });
    const seeked = new Promise<void>((resolve) => video.addEventListener('seeked', () => resolve(), { once: true }));
    video.currentTime = Math.max(0, time);
    await seeked;
    // 有些情況 seek 後不會再呈現新的一格；等不到就以 currentTime 為準。
    const mediaTime = await Promise.race([presented, new Promise<number>((r) => setTimeout(() => r(video.currentTime), 300))]);
    return { time: mediaTime, frame: this.#grab() };
  }

  /**
   * 從目前位置往後播，取第一個時間 ≥ after 的格。影片結束時回傳 null。
   * 取到後立刻暫停，呼叫端處理完再呼叫下一次。
   */
  async next(after: number, signal?: AbortSignal): Promise<{ time: number; frame: RgbaFrame } | null> {
    const video = this.#video;
    if (video.ended || video.currentTime >= this.duration - 1e-3) return null;
    return new Promise((resolve, reject) => {
      let handle = 0;
      const finish = (value: { time: number; frame: RgbaFrame } | null) => {
        video.removeEventListener('ended', onEnded);
        signal?.removeEventListener('abort', onAbort);
        resolve(value);
      };
      const onEnded = () => finish(null);
      const onAbort = () => {
        video.cancelVideoFrameCallback(handle);
        video.pause();
        video.removeEventListener('ended', onEnded);
        reject(new DOMException('cancelled', 'AbortError'));
      };
      const onFrame: FrameCallback = (_now, meta) => {
        if (meta.mediaTime + 1e-4 < after) {
          handle = video.requestVideoFrameCallback(onFrame);
          return;
        }
        video.pause();
        finish({ time: meta.mediaTime, frame: this.#grab() });
      };
      video.addEventListener('ended', onEnded, { once: true });
      signal?.addEventListener('abort', onAbort, { once: true });
      handle = video.requestVideoFrameCallback(onFrame);
      void video.play().catch((error: unknown) => {
        video.cancelVideoFrameCallback(handle);
        reject(error instanceof Error ? error : new Error(String(error)));
      });
    });
  }

  close(): void {
    this.#video.pause();
    this.#video.removeAttribute('src');
    this.#video.load();
    this.#video.remove();
  }
}

/** 逐格讀影片的共同介面（WebCodecs 或 <video>）。 */
export interface VideoFrames {
  width: number;
  height: number;
  /** 分析畫面像素 / 影片原始像素 */
  scale: number;
  duration: number;
  /** 讀取方式：webcodecs 不受視窗顯示影響；video 需要視窗保持顯示 */
  readonly kind: 'webcodecs' | 'video';
  seek(time: number, signal?: AbortSignal): Promise<{ time: number; frame: RgbaFrame }>;
  next(after: number, signal?: AbortSignal): Promise<{ time: number; frame: RgbaFrame } | null>;
  close(): void;
}

/** 優先用 WebCodecs 解 mp4；其他格式或不支援時退回 <video> 逐格播放。 */
export async function openFrames(url: string): Promise<VideoFrames> {
  const { WebCodecsReader } = await import('./webcodecs');
  if (WebCodecsReader.supported()) {
    const reader = new WebCodecsReader(url);
    try {
      if (await reader.open()) return Object.assign(reader, { kind: 'webcodecs' as const });
    } catch {
      // 解析失敗就改用 <video>
    }
    reader.close();
  }
  const reader = new FrameReader(url);
  await reader.open();
  return Object.assign(reader, { kind: 'video' as const });
}
