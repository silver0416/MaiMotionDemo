// 手元影片預填的模型檔：不隨程式打包，由使用者手動下載（約 340 MB）。
// 桌面版由 Rust 下載與保存（指令見 docs/CONTRACT.md「手元影片預填」）；瀏覽器預覽改為自己選檔案。

import { isDesktop } from '../api';
import type { PoseFiles } from './pose';

export type ModelFileId = 'det' | 'pose' | 'wasm';

export interface ModelFileSpec {
  id: ModelFileId;
  /** 存檔名（也用來在瀏覽器預覽辨認選的檔案） */
  name: string;
  /** 來源；.zip 內含 end2end.onnx */
  url: string;
  /** 解壓後檔案的 SHA-256 */
  sha256: string;
  bytes: number;
}

/** 與可行性研究驗證過的版本相同（rtmlib Wholebody balanced；ORT Web 1.29.0）。 */
export const MODEL_FILES: ModelFileSpec[] = [
  {
    id: 'det',
    name: 'yolox_m_8xb8-300e_humanart-c2c7a14a.onnx',
    url: 'https://download.openmmlab.com/mmpose/v1/projects/rtmposev1/onnx_sdk/yolox_m_8xb8-300e_humanart-c2c7a14a.zip',
    sha256: '3dea6513388889f0fff4b77bf7a26013600321b9eb9ceb0e9a400a82572f5f23',
    bytes: 101400344,
  },
  {
    id: 'pose',
    name: 'rtmw-dw-x-l_simcc-cocktail14_270e-256x192_20231122.onnx',
    url: 'https://download.openmmlab.com/mmpose/v1/projects/rtmw/onnx_sdk/rtmw-dw-x-l_simcc-cocktail14_270e-256x192_20231122.zip',
    sha256: '9a9bc17c13ff0a1e37507f45a037a21bc410cd08374b0e664c57d0080843aa58',
    bytes: 228708578,
  },
  {
    id: 'wasm',
    name: 'ort-wasm-simd-threaded.asyncify.wasm',
    url: 'https://cdn.jsdelivr.net/npm/onnxruntime-web@1.29.0/dist/ort-wasm-simd-threaded.asyncify.wasm',
    sha256: '503d17cb7411b79781b9fad1cf0978f03cf06b050c7d399c730e914f473bf549',
    bytes: 25749873,
  },
];

export const MODEL_TOTAL_BYTES = MODEL_FILES.reduce((sum, file) => sum + file.bytes, 0);

/** Rust 回報的模型狀態。 */
export interface ModelStatus {
  installed: boolean;
  /** 已下載的檔案：id → 本機完整路徑 */
  files: Partial<Record<ModelFileId, string>>;
  bytes: number;
}

export interface ModelProgress {
  file: ModelFileId;
  downloaded: number;
  total: number | null;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return await invoke<T>(command, args);
}

/** 桌面版的模型管理（Rust 指令，Codex 實作；尚未實作時呼叫會 reject）。 */
export const modelApi = {
  status: () => call<ModelStatus>('handcam_model_status'),
  install: () => call<ModelStatus>('handcam_model_install'),
  cancel: () => call<void>('handcam_model_cancel'),
  remove: () => call<ModelStatus>('handcam_model_remove'),
};

/** 桌面版：讀已下載的模型檔（asset 協定）。 */
export async function loadInstalled(status: ModelStatus): Promise<PoseFiles> {
  const { convertFileSrc } = await import('@tauri-apps/api/core');
  const read = async (id: ModelFileId) => {
    const path = status.files[id];
    if (!path) throw new Error('模型檔不完整，請重新下載。');
    const response = await fetch(convertFileSrc(path));
    if (!response.ok) throw new Error(`讀不到模型檔（HTTP ${response.status}）`);
    return await response.arrayBuffer();
  };
  const [det, pose, wasm] = await Promise.all([read('det'), read('pose'), read('wasm')]);
  return { det, pose, wasm };
}

/** 瀏覽器預覽：使用者選的檔案依檔名對應；缺的回傳在 missing。 */
export async function loadPicked(files: FileList | File[]): Promise<{ files: PoseFiles | null; missing: string[] }> {
  const list = [...files];
  const found = new Map<ModelFileId, File>();
  for (const spec of MODEL_FILES) {
    const hit = list.find((file) => file.name === spec.name) ?? (spec.id === 'wasm' ? list.find((file) => file.name.endsWith('.wasm')) : undefined);
    if (hit) found.set(spec.id, hit);
  }
  const missing = MODEL_FILES.filter((spec) => !found.has(spec.id)).map((spec) => spec.name);
  if (missing.length > 0) return { files: null, missing };
  const [det, pose, wasm] = await Promise.all(['det', 'pose', 'wasm'].map((id) => found.get(id as ModelFileId)!.arrayBuffer()));
  return { files: { det, pose, wasm }, missing };
}

/**
 * 開發用：網址加上 ?handcam-dev=<網址前綴> 時，從該前綴讀三個檔（需允許跨來源）。
 * 只在 vite 開發模式有效，正式建置不會走到。
 */
export async function loadDevFiles(): Promise<PoseFiles | null> {
  if (!import.meta.env.DEV || isDesktop()) return null;
  const base = new URLSearchParams(window.location.search).get('handcam-dev');
  if (!base) return null;
  const read = async (spec: ModelFileSpec) => {
    const response = await fetch(`${base.replace(/\/$/, '')}/${spec.name}`);
    if (!response.ok) throw new Error(`開發用模型讀取失敗：${spec.name}`);
    return await response.arrayBuffer();
  };
  const [det, pose, wasm] = await Promise.all(MODEL_FILES.map(read));
  return { det, pose, wasm };
}
