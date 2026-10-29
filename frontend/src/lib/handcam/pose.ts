// 全身姿態偵測（RTMW，COCO-WholeBody 133 點）：YOLOX 找人，RTMW 估關鍵點。
// 前後處理照 rtmlib 1.x（Wholebody balanced）：輸入為 BGR、YOLOX 不正規化、RTMW 以 ImageNet 平均值與標準差
// 依 BGR 通道順序相減；雙線性縮放與仿射裁切對齊 OpenCV。原型以 Python 版驗證過準確度，這裡不改演算法。
//
// 模型與 ORT 的 WebAssembly 檔不隨程式打包，由使用者手動下載（見 docs/CONTRACT.md「手元影片預填」）。

import type * as Ort from 'onnxruntime-web';

/** 左手 91–111、右手 112–132；手內 0 腕、5/9/13/17 指根、4/8/12/16/20 指尖。 */
export const LEFT_HAND = 91;
export const RIGHT_HAND = 112;
export const KEYPOINTS = 133;
/** 選玩家時看的關鍵點：肩肘腕＋雙手。 */
const ARM = [5, 6, 7, 8, 9, 10, ...Array.from({ length: 42 }, (_, k) => 91 + k)];

const DET_SIZE = 640;
const POSE_W = 192;
const POSE_H = 256;
const MEAN = [123.675, 116.28, 103.53];
const STD = [58.395, 57.12, 57.375];
const DET_SCORE = 0.3;
const MAX_PEOPLE = 3;
/** 兩隻手都至少這個分數時，不另外在盤面框裡找手 */
const HAND_OK = 0.3;

export interface PoseFiles {
  det: ArrayBuffer;
  pose: ArrayBuffer;
  /** ort-wasm-simd-threaded.asyncify.wasm */
  wasm: ArrayBuffer;
}

export interface PoseResult {
  /** 133 個關鍵點的 x, y（影像像素） */
  kp: Float32Array;
  score: Float32Array;
  box: [number, number, number, number];
  /** 這格是在盤面框裡估的（沒有可用的人框） */
  fallback?: boolean;
}

export interface RgbaFrame {
  data: Uint8ClampedArray;
  width: number;
  height: number;
}

/** 雙線性取樣（OpenCV 的邊界外為 0；resize 時呼叫端會夾在範圍內）。 */
function sample(frame: RgbaFrame, x: number, y: number, out: Float32Array, offset: number, border: number): void {
  const { data, width, height } = frame;
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const fx = x - x0;
  const fy = y - y0;
  for (let c = 0; c < 3; c += 1) {
    // BGR：通道 0 取 RGBA 的 B
    const ch = 2 - c;
    let acc = 0;
    for (let dy = 0; dy <= 1; dy += 1) {
      const yy = y0 + dy;
      const wy = dy === 0 ? 1 - fy : fy;
      if (wy === 0) continue;
      for (let dx = 0; dx <= 1; dx += 1) {
        const xx = x0 + dx;
        const wx = dx === 0 ? 1 - fx : fx;
        if (wx === 0) continue;
        const value = xx < 0 || yy < 0 || xx >= width || yy >= height ? border : data[(yy * width + xx) * 4 + ch];
        acc += wx * wy * value;
      }
    }
    out[offset + c] = acc;
  }
}

/** YOLOX 輸入：等比縮放貼在左上角，其餘填 114。 */
function detInput(frame: RgbaFrame): { tensor: Float32Array; ratio: number } {
  const ratio = Math.min(DET_SIZE / frame.height, DET_SIZE / frame.width);
  const rw = Math.floor(frame.width * ratio);
  const rh = Math.floor(frame.height * ratio);
  const plane = DET_SIZE * DET_SIZE;
  const tensor = new Float32Array(3 * plane).fill(114);
  const px = new Float32Array(3);
  const sx = frame.width / rw;
  const sy = frame.height / rh;
  for (let v = 0; v < rh; v += 1) {
    // OpenCV INTER_LINEAR：像素中心對齊，邊界夾住
    const y = Math.min(Math.max((v + 0.5) * sy - 0.5, 0), frame.height - 1);
    for (let u = 0; u < rw; u += 1) {
      const x = Math.min(Math.max((u + 0.5) * sx - 0.5, 0), frame.width - 1);
      sample(frame, x, y, px, 0, 0);
      const at = v * DET_SIZE + u;
      tensor[at] = px[0];
      tensor[plane + at] = px[1];
      tensor[2 * plane + at] = px[2];
    }
  }
  return { tensor, ratio };
}

/** RTMW 輸入：bbox 放大 1.25 倍、補成 3:4，仿射裁切成 192×256 並正規化。 */
function poseInput(frame: RgbaFrame, box: [number, number, number, number]) {
  const [x1, y1, x2, y2] = box;
  const cx = (x1 + x2) / 2;
  const cy = (y1 + y2) / 2;
  let sw = (x2 - x1) * 1.25;
  let sh = (y2 - y1) * 1.25;
  const aspect = POSE_W / POSE_H;
  if (sw > sh * aspect) sh = sw / aspect;
  else sw = sh * aspect;
  const s = POSE_W / sw;
  const plane = POSE_W * POSE_H;
  const tensor = new Float32Array(3 * plane);
  const px = new Float32Array(3);
  for (let v = 0; v < POSE_H; v += 1) {
    const y = cy + (v - POSE_H / 2) / s;
    for (let u = 0; u < POSE_W; u += 1) {
      const x = cx + (u - POSE_W / 2) / s;
      sample(frame, x, y, px, 0, 0);
      const at = v * POSE_W + u;
      tensor[at] = (px[0] - MEAN[0]) / STD[0];
      tensor[plane + at] = (px[1] - MEAN[1]) / STD[1];
      tensor[2 * plane + at] = (px[2] - MEAN[2]) / STD[2];
    }
  }
  return { tensor, center: [cx, cy] as const, scale: [sw, sh] as const };
}

export class PoseRuntime {
  #ort: typeof Ort;
  #det: Ort.InferenceSession;
  #pose: Ort.InferenceSession;
  readonly backend: string;

  private constructor(ort: typeof Ort, det: Ort.InferenceSession, pose: Ort.InferenceSession, backend: string) {
    this.#ort = ort;
    this.#det = det;
    this.#pose = pose;
    this.backend = backend;
  }

  /** 是否有 WebGPU（沒有時退回 CPU WebAssembly，會非常慢）。 */
  static async gpuAvailable(): Promise<boolean> {
    try {
      const gpu = (navigator as Navigator & { gpu?: { requestAdapter(o?: object): Promise<unknown> } }).gpu;
      if (!gpu) return false;
      return !!(await gpu.requestAdapter({ powerPreference: 'high-performance' }));
    } catch {
      return false;
    }
  }

  static async create(files: PoseFiles): Promise<PoseRuntime> {
    const ort = await import('onnxruntime-web/webgpu');
    ort.env.wasm.wasmBinary = files.wasm;
    ort.env.wasm.numThreads = 1;
    ort.env.wasm.proxy = false;
    // ORT 的一般警告（節點分配到 CPU 等）會以 console.error 印出，干擾錯誤紀錄
    ort.env.logLevel = 'error';
    // 雙 GPU（例如 NVIDIA＋內顯）時選高效能那顆
    (ort.env as unknown as { webgpu: { powerPreference?: string } }).webgpu.powerPreference = 'high-performance';
    const gpu = await PoseRuntime.gpuAvailable();
    const providers = gpu ? ['webgpu'] : ['wasm'];
    const options: Ort.InferenceSession.SessionOptions = { executionProviders: providers, graphOptimizationLevel: 'all' };
    const det = await ort.InferenceSession.create(new Uint8Array(files.det), options);
    const pose = await ort.InferenceSession.create(new Uint8Array(files.pose), options);
    return new PoseRuntime(ort as unknown as typeof Ort, det, pose, gpu ? 'webgpu' : 'wasm');
  }

  async release(): Promise<void> {
    await this.#det.release();
    await this.#pose.release();
  }

  async #boxes(frame: RgbaFrame): Promise<[number, number, number, number, number][]> {
    const { tensor, ratio } = detInput(frame);
    const input = new this.#ort.Tensor('float32', tensor, [1, 3, DET_SIZE, DET_SIZE]);
    const result = await this.#det.run({ [this.#det.inputNames[0]]: input });
    input.dispose();
    const dets = result[this.#det.outputNames[0]];
    const data = (await dets.getData()) as Float32Array;
    const count = dets.dims[1];
    const out: [number, number, number, number, number][] = [];
    for (let k = 0; k < count; k += 1) {
      const score = data[k * 5 + 4];
      if (score > DET_SCORE) {
        out.push([data[k * 5] / ratio, data[k * 5 + 1] / ratio, data[k * 5 + 2] / ratio, data[k * 5 + 3] / ratio, score]);
      }
    }
    for (const tensorOut of Object.values(result)) tensorOut.dispose();
    out.sort((a, b) => b[4] - a[4]);
    return out.slice(0, MAX_PEOPLE);
  }

  async #keypoints(frame: RgbaFrame, box: [number, number, number, number]): Promise<PoseResult> {
    const { tensor, center, scale } = poseInput(frame, box);
    const input = new this.#ort.Tensor('float32', tensor, [1, 3, POSE_H, POSE_W]);
    const result = await this.#pose.run({ [this.#pose.inputNames[0]]: input });
    input.dispose();
    const [nameX, nameY] = this.#pose.outputNames;
    const simX = (await result[nameX].getData()) as Float32Array;
    const simY = (await result[nameY].getData()) as Float32Array;
    const wx = result[nameX].dims[2];
    const wy = result[nameY].dims[2];
    for (const tensorOut of Object.values(result)) tensorOut.dispose();
    const kp = new Float32Array(KEYPOINTS * 2);
    const score = new Float32Array(KEYPOINTS);
    for (let k = 0; k < KEYPOINTS; k += 1) {
      let bx = 0;
      let mx = -Infinity;
      for (let i = 0; i < wx; i += 1) {
        const value = simX[k * wx + i];
        if (value > mx) {
          mx = value;
          bx = i;
        }
      }
      let by = 0;
      let my = -Infinity;
      for (let i = 0; i < wy; i += 1) {
        const value = simY[k * wy + i];
        if (value > my) {
          my = value;
          by = i;
        }
      }
      const val = (mx + my) / 2;
      let lx = bx / 2;
      let ly = by / 2;
      if (val <= 0) {
        lx = -0.5;
        ly = -0.5;
      }
      kp[k * 2] = (lx / POSE_W) * scale[0] + center[0] - scale[0] / 2;
      kp[k * 2 + 1] = (ly / POSE_H) * scale[1] + center[1] - scale[1] / 2;
      score[k] = val;
    }
    return { kp, score, box };
  }

  /**
   * 偵測畫面中的玩家（手臂與雙手平均分數最高的人）；沒有人時回傳 null。
   * 給了 fallback（盤面附近的框）時：找人找不到、或兩隻手有一隻很弱（機位拍不到肩膀時常見），
   * 改在這個框裡直接估手，取兩手中較弱那隻分數較高的結果。
   */
  async detect(frame: RgbaFrame, fallback?: [number, number, number, number]): Promise<PoseResult | null> {
    const boxes = await this.#boxes(frame);
    let best: PoseResult | null = null;
    let bestScore = -Infinity;
    for (const [x1, y1, x2, y2] of boxes) {
      const result = await this.#keypoints(frame, [x1, y1, x2, y2]);
      let sum = 0;
      for (const k of ARM) sum += result.score[k];
      const mean = sum / ARM.length;
      if (mean > bestScore) {
        bestScore = mean;
        best = result;
      }
    }
    if (!fallback || (best && weakerHand(best) >= HAND_OK)) return best;
    const board = await this.#keypoints(frame, fallback);
    return !best || weakerHand(board) > weakerHand(best) ? { ...board, fallback: true } : best;
  }
}

/** 兩隻手平均分數中較低的那個。 */
function weakerHand(result: PoseResult): number {
  let left = 0;
  let right = 0;
  for (let k = 0; k < 21; k += 1) {
    left += result.score[LEFT_HAND + k];
    right += result.score[RIGHT_HAND + k];
  }
  return Math.min(left, right) / 21;
}
