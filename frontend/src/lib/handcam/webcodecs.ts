// 用 WebCodecs 直接解碼 mp4：不必即時播放，也不受視窗是否顯示影響（<video> 在視窗縮小或被遮住時不合成畫面）。
// 解出的時間依 edit list 換成與 <video> 相同的播放時間軸，使用者對好的 offset 才能直接沿用。

import type { RgbaFrame } from './pose';

const MAX_WIDTH = 1280;
/** 解碼器輸出、還沒被取走的格數上限（VideoFrame 佔顯示卡記憶體，要盡快關閉） */
const MAX_QUEUED = 4;

interface EncodedSample {
  /** 播放時間（秒） */
  time: number;
  /** 微秒，交給解碼器 */
  timestamp: number;
  duration: number;
  key: boolean;
  data: Uint8Array;
}

export class WebCodecsReader {
  width = 0;
  height = 0;
  scale = 1;
  duration = 0;

  #url: string;
  #samples: EncodedSample[] = [];
  #config: VideoDecoderConfig | null = null;
  #decoder: VideoDecoder | null = null;
  #feed = 0;
  #queue: VideoFrame[] = [];
  #waiter: (() => void) | null = null;
  #error: Error | null = null;
  #flushed = false;
  /** 早於這個時間的輸出直接關掉（seek 後從關鍵格開始解碼） */
  #skipBefore = 0;
  #canvas: HTMLCanvasElement;
  #context: CanvasRenderingContext2D;

  constructor(url: string) {
    this.#url = url;
    this.#canvas = document.createElement('canvas');
    const context = this.#canvas.getContext('2d', { willReadFrequently: true });
    if (!context) throw new Error('無法建立畫布');
    this.#context = context;
  }

  static supported(): boolean {
    return typeof VideoDecoder !== 'undefined' && typeof VideoFrame !== 'undefined';
  }

  /** 讀檔並解析；不是 mp4 或解碼器不支援時回傳 false（呼叫端改用 <video> 取格）。 */
  async open(): Promise<boolean> {
    const response = await fetch(this.#url);
    if (!response.ok) throw new Error(`讀不到影片（HTTP ${response.status}）`);
    const bytes = await response.arrayBuffer();
    const head = new Uint8Array(bytes, 4, 4);
    if (String.fromCharCode(...head) !== 'ftyp') return false;
    const mp4 = await import('mp4box');
    const file = mp4.createFile();
    const parsed = await new Promise<boolean>((resolve) => {
      file.onError = () => resolve(false);
      file.onReady = (info) => {
        const track = info.videoTracks[0];
        if (!track) {
          resolve(false);
          return;
        }
        const trak = file.getTrackById(track.id);
        let description: Uint8Array | undefined;
        for (const entry of trak.mdia.minf.stbl.stsd.entries) {
          const box = (entry as unknown as Record<string, { write(stream: unknown): void } | undefined>).avcC
            ?? (entry as unknown as Record<string, { write(stream: unknown): void } | undefined>).hvcC
            ?? (entry as unknown as Record<string, { write(stream: unknown): void } | undefined>).vpcC
            ?? (entry as unknown as Record<string, { write(stream: unknown): void } | undefined>).av1C;
          if (box) {
            const stream = new mp4.DataStream(undefined, 0, 1);
            box.write(stream);
            description = new Uint8Array(stream.buffer as ArrayBuffer, 8);
            break;
          }
        }
        // edit list：開頭的空白段延後播放，第一個非空白段的 media_time 是起點
        let shift = 0;
        let mediaStart = 0;
        for (const entry of trak.edts?.elst?.entries ?? []) {
          if (entry.media_time === -1) shift += entry.segment_duration / info.timescale;
          else {
            mediaStart = entry.media_time;
            break;
          }
        }
        const timescale = track.timescale;
        this.width = track.video?.width ?? track.track_width;
        this.height = track.video?.height ?? track.track_height;
        this.duration = track.duration / timescale;
        this.#config = {
          codec: track.codec.startsWith('vp08') ? 'vp8' : track.codec,
          codedWidth: this.width,
          codedHeight: this.height,
          hardwareAcceleration: 'prefer-hardware',
          ...(description ? { description } : {}),
        };
        file.onSamples = (_id, _user, samples) => {
          for (const sample of samples) {
            if (!sample.data) continue;
            const time = (sample.cts - mediaStart) / timescale + shift;
            this.#samples.push({
              time,
              timestamp: Math.round(time * 1e6),
              duration: Math.round((sample.duration / timescale) * 1e6),
              key: sample.is_sync,
              data: sample.data,
            });
          }
        };
        file.setExtractionOptions(track.id, undefined, { nbSamples: track.nb_samples });
        file.start();
        resolve(true);
      };
      const buffer = mp4.MP4BoxBuffer.fromArrayBuffer(bytes, 0);
      file.appendBuffer(buffer);
      file.flush();
    });
    if (!parsed || !this.#config || this.#samples.length === 0) return false;
    // 解碼順序依 dts（mp4 樣本本來就是解碼順序），播放時間另外記
    const support = await VideoDecoder.isConfigSupported(this.#config);
    if (!support.supported) return false;
    this.scale = Math.min(1, MAX_WIDTH / this.width);
    this.#canvas.width = Math.round(this.width * this.scale);
    this.#canvas.height = Math.round(this.height * this.scale);
    this.width = this.#canvas.width;
    this.height = this.#canvas.height;
    return true;
  }

  #reset(): void {
    for (const frame of this.#queue) frame.close();
    this.#queue = [];
    this.#decoder?.close();
    this.#error = null;
    this.#flushed = false;
    this.#decoder = new VideoDecoder({
      output: (frame) => {
        if (frame.timestamp / 1e6 < this.#skipBefore - 1e-4) {
          frame.close();
        } else {
          this.#queue.push(frame);
        }
        this.#wake();
      },
      error: (error) => {
        this.#error = error instanceof Error ? error : new Error(String(error));
        this.#wake();
      },
    });
    this.#decoder.configure(this.#config!);
    this.#decoder.addEventListener('dequeue', () => this.#wake());
  }

  #wake(): void {
    const waiter = this.#waiter;
    this.#waiter = null;
    waiter?.();
  }

  #fill(): void {
    const decoder = this.#decoder!;
    while (this.#feed < this.#samples.length && decoder.decodeQueueSize < 4 && this.#queue.length < MAX_QUEUED) {
      const sample = this.#samples[this.#feed];
      this.#feed += 1;
      decoder.decode(new EncodedVideoChunk({ type: sample.key ? 'key' : 'delta', timestamp: sample.timestamp, duration: sample.duration, data: sample.data }));
    }
  }

  /** 取下一個解好的格；全部解完回傳 null。 */
  async #pull(signal?: AbortSignal): Promise<VideoFrame | null> {
    for (;;) {
      if (signal?.aborted) throw new DOMException('cancelled', 'AbortError');
      if (this.#error) throw this.#error;
      const frame = this.#queue.shift();
      if (frame) {
        this.#fill();
        return frame;
      }
      if (this.#feed >= this.#samples.length) {
        if (this.#flushed) return null;
        this.#flushed = true;
        await this.#decoder!.flush();
        continue;
      }
      const waiting = new Promise<void>((resolve) => (this.#waiter = resolve));
      this.#fill();
      await waiting;
    }
  }

  #grab(frame: VideoFrame): RgbaFrame {
    this.#context.drawImage(frame, 0, 0, this.width, this.height);
    frame.close();
    const image = this.#context.getImageData(0, 0, this.width, this.height);
    return { data: image.data, width: image.width, height: image.height };
  }

  /** 跳到 time：從之前最近的關鍵格開始解碼，取第一個時間 ≥ time 的格。 */
  async seek(time: number, signal?: AbortSignal): Promise<{ time: number; frame: RgbaFrame }> {
    let start = 0;
    for (let k = 0; k < this.#samples.length; k += 1) {
      if (this.#samples[k].key && this.#samples[k].time <= time + 1e-6) start = k;
      if (this.#samples[k].time > time + 5) break;
    }
    this.#feed = start;
    // 半格的容差：time 落在兩格之間時取後一格，與 <video> 停在 currentTime 時顯示的格一致
    this.#skipBefore = time - 0.004;
    this.#reset();
    const result = await this.next(time - 0.004, signal);
    if (!result) throw new Error('影片在這個時間沒有畫面');
    return result;
  }

  async next(after: number, signal?: AbortSignal): Promise<{ time: number; frame: RgbaFrame } | null> {
    for (;;) {
      const frame = await this.#pull(signal);
      if (!frame) return null;
      const time = frame.timestamp / 1e6;
      if (time + 1e-4 < after) {
        frame.close();
        continue;
      }
      return { time, frame: this.#grab(frame) };
    }
  }

  close(): void {
    for (const frame of this.#queue) frame.close();
    this.#queue = [];
    if (this.#decoder && this.#decoder.state !== 'closed') this.#decoder.close();
    this.#samples = [];
  }
}
