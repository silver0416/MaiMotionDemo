// 影片視窗「自動預填」的狀態：模型、環點、分析進度與結果。
// 環點與參考格依影片記在本機，同一部影片下次不用重標。

import { isDesktop } from '../lib/api';
import { boardFromRing, runJob, type JobProgress, type JobResult, type RingPoint } from '../lib/handcam/job';
import { apply, invert, type Vec2 } from '../lib/handcam/homography';
import { loadDevFiles, loadInstalled, loadPicked, modelApi, type ModelProgress, type ModelStatus } from '../lib/handcam/models';
import type { PoseFiles } from '../lib/handcam/pose';
import { listenEvent, type HandcamMark, type HandcamNoteData, type VideoLink } from '../lib/video';

const RING_KEY = 'maimotion.handcam-ring.v1';

export type Phase = 'idle' | 'marking' | 'loading-notes' | 'running' | 'done';

interface SavedRing {
  time: number;
  /** 依鍵號 1–8；被擋住的為 null。影片原始像素，已扣掉翻轉。 */
  points: (Vec2 | null)[];
}

function videoKey(link: VideoLink | null): string | null {
  if (!link) return null;
  if (link.kind === 'youtube' && link.id) return `yt:${link.id}`;
  if (link.path) return `file:${link.path}`;
  return null;
}

function readSaved(): Record<string, SavedRing> {
  try {
    const raw = JSON.parse(localStorage.getItem(RING_KEY) ?? '{}') as unknown;
    return raw && typeof raw === 'object' ? (raw as Record<string, SavedRing>) : {};
  } catch {
    return {};
  }
}

export class Handcam {
  phase = $state<Phase>('idle');
  /** 目前影片 */
  key = $state<string | null>(null);
  ring = $state<(Vec2 | null)[]>(Array(8).fill(null));
  referenceTime = $state<number | null>(null);
  /** 標點中：下一個要點的鍵號 index（0–7） */
  cursor = $state(0);

  modelStatus = $state<ModelStatus | null>(null);
  /** 桌面版還沒有模型管理指令（Rust 端未實作）時的說明 */
  modelUnavailable = $state<string | null>(null);
  modelProgress = $state<ModelProgress | null>(null);
  installing = $state(false);
  /** 瀏覽器預覽選好的檔案 */
  picked = $state.raw<PoseFiles | null>(null);
  pickMessage = $state<string | null>(null);
  gpu = $state<boolean | null>(null);

  refine = $state(true);
  progress = $state<JobProgress | null>(null);
  result = $state.raw<JobResult | null>(null);
  error = $state<string | null>(null);
  applied = $state<number | null>(null);
  applying = $state(false);

  #abort: AbortController | null = null;
  #unlisten: (() => void) | null = null;

  /** 至少 4 點才能求透視。 */
  get pointCount(): number {
    return this.ring.filter(Boolean).length;
  }

  get ringPoints(): RingPoint[] {
    const out: RingPoint[] = [];
    this.ring.forEach((p, index) => {
      if (p) out.push({ button: index + 1, x: p.x, y: p.y });
    });
    return out;
  }

  /** 參考格像素 → 盤面；點不足時為 null。 */
  get boardFromRef() {
    return this.pointCount >= 4 ? boardFromRing(this.ringPoints) : null;
  }

  /** 盤面 → 參考格像素（畫預覽環用）。 */
  get refFromBoard() {
    const h = this.boardFromRef;
    return h ? invert(h) : null;
  }

  /** 盤面座標換成參考格像素；沒有校正時回傳 null。 */
  project(x: number, y: number): Vec2 | null {
    const h = this.refFromBoard;
    return h ? apply(h, x, y) : null;
  }

  async init(): Promise<void> {
    const { PoseRuntime } = await import('../lib/handcam/pose');
    this.gpu = await PoseRuntime.gpuAvailable();
    if (isDesktop()) {
      this.#unlisten = await listenEvent<ModelProgress>('handcam-model-progress', (payload) => (this.modelProgress = payload));
      await this.refreshModel();
    } else {
      try {
        const dev = await loadDevFiles();
        if (dev) {
          this.picked = dev;
          this.pickMessage = '開發模式：已從網址載入模型檔。';
        }
      } catch (error) {
        this.pickMessage = error instanceof Error ? error.message : String(error);
      }
    }
  }

  dispose(): void {
    this.#abort?.abort();
    this.#unlisten?.();
  }

  async refreshModel(): Promise<void> {
    try {
      this.modelStatus = await modelApi.status();
      this.modelUnavailable = null;
    } catch (error) {
      this.modelStatus = null;
      this.modelUnavailable = String(error);
    }
  }

  async installModel(): Promise<void> {
    this.installing = true;
    this.modelProgress = null;
    try {
      this.modelStatus = await modelApi.install();
      this.modelUnavailable = null;
    } catch (error) {
      this.error = `模型下載失敗：${String(error)}`;
    } finally {
      this.installing = false;
      this.modelProgress = null;
    }
  }

  async cancelInstall(): Promise<void> {
    try {
      await modelApi.cancel();
    } catch {
      // 已經結束就不用管。
    }
  }

  async removeModel(): Promise<void> {
    try {
      this.modelStatus = await modelApi.remove();
    } catch (error) {
      this.error = `刪除模型失敗：${String(error)}`;
    }
  }

  async pickFiles(files: FileList | null): Promise<void> {
    if (!files || files.length === 0) return;
    const { files: loaded, missing } = await loadPicked(files);
    if (loaded) {
      this.picked = loaded;
      this.pickMessage = '已載入模型檔（只在這次開啟有效）。';
    } else {
      this.pickMessage = `還缺：${missing.join('、')}`;
    }
  }

  get modelReady(): boolean {
    return isDesktop() ? !!this.modelStatus?.installed : !!this.picked;
  }

  /** 換影片時讀回這部影片上次標的環點。 */
  useVideo(link: VideoLink | null): void {
    const key = videoKey(link);
    if (key === this.key) return;
    this.cancel();
    this.key = key;
    this.phase = 'idle';
    this.result = null;
    this.applied = null;
    this.error = null;
    const saved = key ? readSaved()[key] : undefined;
    if (saved && Array.isArray(saved.points) && saved.points.length === 8) {
      this.ring = saved.points.map((p) => (p && Number.isFinite(p.x) && Number.isFinite(p.y) ? { x: p.x, y: p.y } : null));
      this.referenceTime = Number.isFinite(saved.time) ? saved.time : null;
    } else {
      this.ring = Array(8).fill(null);
      this.referenceTime = null;
    }
  }

  #save(): void {
    if (!this.key || this.referenceTime === null) return;
    const all = readSaved();
    all[this.key] = { time: this.referenceTime, points: $state.snapshot(this.ring) as (Vec2 | null)[] };
    try {
      localStorage.setItem(RING_KEY, JSON.stringify(all));
    } catch {
      // 存不了就只在這次有效。
    }
  }

  startMarking(time: number): void {
    this.ring = Array(8).fill(null);
    this.referenceTime = time;
    this.cursor = 0;
    this.result = null;
    this.applied = null;
    this.phase = 'marking';
  }

  /** 點下目前鍵號的位置（影片原始像素，已扣掉翻轉）。 */
  place(point: Vec2): void {
    if (this.phase !== 'marking') return;
    const next = [...this.ring];
    next[this.cursor] = point;
    this.ring = next;
    this.#advance();
  }

  skip(): void {
    if (this.phase !== 'marking') return;
    this.#advance();
  }

  undo(): void {
    if (this.phase !== 'marking' || this.cursor === 0) return;
    this.cursor -= 1;
    const next = [...this.ring];
    next[this.cursor] = null;
    this.ring = next;
  }

  #advance(): void {
    if (this.cursor >= 7) this.finishMarking();
    else this.cursor += 1;
  }

  finishMarking(): void {
    this.phase = 'idle';
    this.#save();
  }

  cancel(): void {
    this.#abort?.abort();
    this.#abort = null;
    if (this.phase === 'running' || this.phase === 'loading-notes') this.phase = 'idle';
  }

  /** 執行分析；notes 由主視窗提供。 */
  async run(url: string, notes: HandcamNoteData[], offset: number, mirror: boolean): Promise<void> {
    if (this.referenceTime === null) return;
    this.error = null;
    this.result = null;
    this.applied = null;
    this.progress = null;
    this.phase = 'running';
    const abort = new AbortController();
    this.#abort = abort;
    try {
      const files = isDesktop() ? await loadInstalled(this.modelStatus!) : this.picked!;
      this.result = await runJob(
        { url, files, notes, offset, mirror, referenceTime: this.referenceTime, ring: this.ringPoints, refine: this.refine },
        (progress) => (this.progress = progress),
        abort.signal,
      );
      this.phase = 'done';
    } catch (error) {
      if (abort.signal.aborted) {
        this.phase = 'idle';
        return;
      }
      this.error = error instanceof Error ? error.message : String(error);
      this.phase = 'idle';
    } finally {
      if (this.#abort === abort) this.#abort = null;
      this.progress = null;
    }
  }

  /** 要送給主視窗的預填：高信心照填，中信心標不確定，低信心不填。 */
  marks(): HandcamMark[] {
    if (!this.result) return [];
    const out: HandcamMark[] = [];
    for (const guess of this.result.guesses) {
      const mark: HandcamMark = { key: guess.key };
      if (guess.hand && guess.tier !== 'low') {
        mark.hand = guess.hand;
        if (guess.tier === 'mid') mark.handUnsure = true;
      }
      if (guess.track && guess.trackTier && guess.trackTier !== 'low') {
        mark.track = guess.track;
        if (guess.trackTier === 'mid') mark.trackUnsure = true;
      }
      if (mark.hand || mark.track) out.push(mark);
    }
    return out;
  }
}
