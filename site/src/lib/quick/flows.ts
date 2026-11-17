import annotate from './shots/annotate.json';
import download from './shots/download.json';
import exportFlow from './shots/export.json';
import open from './shots/open.json';
import search from './shots/search.json';
import { EXE_SKETCH } from './sketch';

/**
 * 「基礎操作演示」各頁的實際畫面導覽。
 * - 畫面與座標：scripts/capture-quick.mjs 驅動實際的 App（下載頁是網站本身）拍下來，存成 ./shots/<flow>.json（不要手改，重拍會蓋掉）。
 * - 旁白：寫在這裡，依步驟 id 對應（改字不用重拍）。按鈕名稱要和 App 畫面一字不差。
 * - 拍不到的系統畫面（例如在檔案總管連按兩下）用 lead 接在最前面：示意畫面（sketch）＋步驟，版面在 ./sketch.ts。
 * 新增一頁：capture-quick.mjs 加流程並重拍 → 這裡 import 新的 json、加一筆 TOURS → .md 裡寫 <QuickTour flow="…" />。
 */

/** x, y, 寬, 高：App 視窗（manifest 的 viewport，1280×800）裡的 CSS 像素。 */
export type Rect = [number, number, number, number];

/** 示意畫面的種類（QuickTour 依這個畫對應的元件，不是截圖）。 */
export type SketchId = 'exe-file';

export interface ShotFrame {
  id: string;
  /** static/ 底下的網址；示意畫面沒有 */
  src?: string;
  bytes?: number;
  /** 沿用別的流程拍的圖（不另存一張） */
  reused?: boolean;
  /** 示意畫面：不是截圖，圖說標「示意」 */
  sketch?: SketchId;
}

/**
 * 一步的種類由欄位決定：
 * - target：游標飛過去按下（double 是連按兩下）；有 after 就換成那張畫面。
 * - key：不出現游標，畫面上的鍵帽亮一下（按鍵盤），有 after 就換成那張畫面。
 * - highlight：不按，只框起來給人看。
 */
export interface ShotStep {
  id: string;
  /** 這一步開始時的畫面 */
  frame: string;
  /** 按下去之後換成的畫面 */
  after?: string;
  /** 要按的地方（游標飛過去按下） */
  target?: Rect;
  /** 連按兩下（開檔案） */
  double?: boolean;
  /** 按鍵盤的哪個鍵（鍵帽上的字，要和 Tour.keys 的 cap 一致） */
  key?: string;
  /** 鏡頭拉近時要放進畫面的範圍（沒給就用 target 或 highlight；都沒有就看整個畫面） */
  focus?: Rect;
  /** 不按、只框起來給人看的地方 */
  highlight?: Rect;
}

export interface ShotManifest {
  flow: string;
  app: string | null;
  viewport: { width: number; height: number; scale: number };
  frames: ShotFrame[];
  steps: ShotStep[];
  meta?: Record<string, unknown>;
}

export interface Tour {
  manifest: ShotManifest;
  /** 接在截圖前面的示意畫面與步驟（拍不到的系統畫面） */
  lead?: { frames: ShotFrame[]; steps: ShotStep[] };
  /** 有 key 步驟時，截圖下方（截圖框之外）一列的鍵帽 */
  keys?: { cap: string; label: string }[];
  /** 整個導覽的名稱（aria-label） */
  label: string;
  /** 還沒開始時的旁白 */
  intro: string;
  /** 播完時的旁白 */
  outro: string;
  /** 各步驟的旁白，key 是步驟 id */
  say: Record<string, string>;
  /** 這幾步不拉近，拉遠看整個畫面（例如看結果） */
  wide?: string[];
}

const ANNOTATE_KEYS = [
  { cap: 'A', label: '左手' },
  { cap: 'D', label: '右手' },
];

/**
 * 按鍵步驟的旁白：照擷取時實際按的鍵（manifest 的 step.key，由面板上的模型建議決定）產生，不寫死。
 * 第一顆多說一句「自動跳到下一步」，最後一顆說「一步一步標下去」。
 */
function keySay(manifest: ShotManifest): Record<string, string> {
  const steps = manifest.steps.filter((step) => step.key);
  return Object.fromEntries(
    steps.map((step, i) => {
      const hand = ANNOTATE_KEYS.find((k) => k.cap === step.key)?.label ?? '';
      const line = `按 ${step.key} 標${hand}`;
      if (i === 0) return [step.id, `${line}，自動跳到下一步`];
      if (i === steps.length - 1 && steps.length > 2) return [step.id, `再${line}，一步一步標下去`];
      return [step.id, line];
    }),
  );
}

export const TOURS = {
  download: {
    manifest: download as unknown as ShotManifest,
    label: '實際畫面：在下載頁下載 Windows 版',
    intro: '到下載頁，按一個按鈕就好。',
    outro: '檔案會存到「下載」資料夾。',
    say: {
      button: '按最上面的「下載 Windows 版」',
    },
  },
  open: {
    manifest: open as unknown as ShotManifest,
    lead: {
      frames: [{ id: 'file', sketch: 'exe-file' }],
      steps: [{ id: 'launch', frame: 'file', target: EXE_SKETCH.icon, double: true, focus: EXE_SKETCH.focus, after: 'home' }],
    },
    label: '開啟 App：連按兩下下載的檔案，認識主畫面',
    intro: '打開 App，認識畫面上的三個區塊。',
    outro: '這就是 App 的主畫面。',
    say: {
      launch: '連按兩下剛下載的檔案',
      records: '左邊是譜面紀錄',
      disc: '中間是盤面與播放列',
      tabs: '右邊是資訊分頁',
    },
    wide: ['records', 'disc', 'tabs'],
  },
  search: {
    manifest: search as unknown as ShotManifest,
    label: '實際畫面：搜尋並載入第一張譜面',
    intro: '從搜尋譜面到載入，一步一步看。',
    outro: '完成。想再看一次就按重播。',
    say: {
      open: '按左邊的「搜尋譜面」',
      type: '在搜尋框輸入歌名',
      search: '按「搜尋」',
      song: '點一下找到的歌',
      load: '在「BASIC」旁邊按「載入」',
      done: '譜面出現在左邊的譜面紀錄',
      play: '按「播放」就能看兩隻手怎麼動',
    },
    wide: ['done'],
  },
  annotate: {
    manifest: annotate as unknown as ShotManifest,
    keys: ANNOTATE_KEYS,
    label: '實際畫面：用鍵盤標出左右手',
    intro: '用鍵盤一顆一顆標出左右手。',
    outro: '完成。標註存在這筆譜面紀錄裡。',
    say: {
      tab: '按右邊的「標註」分頁',
      first: '在清單點第一顆音符',
      ...keySay(annotate as unknown as ShotManifest),
      progress: '上面的數字是已確認的數量，標註會自動存起來',
    },
  },
  export: {
    manifest: exportFlow as unknown as ShotManifest,
    label: '實際畫面：選取兩筆紀錄並匯出',
    intro: '把兩筆紀錄一起匯出成一個檔案。',
    outro: '接著選存檔位置，就會存成一個標記檔，可以拿來分享或備份。',
    say: {
      more: '在紀錄上按右鍵，或按右邊的「⋯」',
      select: '選「選取」',
      pick: '再點另一筆，加入選取',
      export: '按選取工具列的「匯出標記檔」（下載圖示）',
    },
  },
} satisfies Record<string, Tour>;

export type TourId = keyof typeof TOURS;
