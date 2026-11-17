import type { Rect } from './flows';

/**
 * 示意畫面（拍不到的系統畫面，例如在檔案總管連按兩下下載的檔案）的版面。
 * 座標和截圖一樣是 1280×800 的空間：parts/ExeSketch.svelte 用它排版，flows.ts 用它當游標與鏡頭的位置，兩邊共用一份。
 */
export const SKETCH_VIEWPORT = { width: 1280, height: 800 };

export const EXE_SKETCH = {
  /** 左上角的資料夾名稱 */
  folder: [48, 40, 400, 40] as Rect,
  /** App 圖示（游標連按兩下的地方） */
  icon: [580, 262, 120, 120] as Rect,
  /** 圖示下面的檔名 */
  name: [440, 396, 400, 60] as Rect,
  /** 鏡頭拉近時要放進畫面的範圍 */
  focus: [420, 236, 440, 236] as Rect,
};
