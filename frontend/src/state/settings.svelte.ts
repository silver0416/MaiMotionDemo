import { MAIN_LINEUP, migrateDraft, promoteMainCards } from '../lib/contract';
import { STORE_SETTINGS, dbGet, dbPut } from '../lib/db';
import type { ConfigDraft } from '../lib/types';
import { session } from './session.svelte';
import { view } from './view.svelte';

const SETTINGS_KEY = 'app';
const SAVE_DELAY_MS = 300;

/** 需要保存的顯示設定欄位；其餘（例如暫時狀態）不寫入資料庫。 */
const VIEW_KEYS = [
  'showNotes',
  'noteMode',
  'approach',
  'approachSeconds',
  'touchSeconds',
  'noteColors',
  'noteSaturation',
  'hitEffects',
  'showLeft',
  'showRight',
  'showFullTrack',
  'showRecentTrail',
  'trailSeconds',
  'showAssignmentTags',
  'sensorMode',
  'showSensorLabels',
  'showFireworks',
  'showHandovers',
  'showPalms',
  'showButtons',
  'showBoardGrid',
  'showCalibrationRing',
  'zoom',
  'calibration',
] as const;

type ViewKey = (typeof VIEW_KEYS)[number];

/**
 * 顯示設定預設值的版號。某一版換了預設值、而且要讓更新上來的人也自動換過去時就加一，
 * 並在 VIEW_RESETS 列出那一代換掉的欄位：讀到比較舊的存檔，這些欄位不沿用存檔的值，改用這一版的預設
 * （之後使用者自己再改的照常保存）。更新後第一次開啟的通知另見 state/discDefaultsIntro.ts。
 * 1＝2026-11-23 使用者指定：照遊戲上色打開、打擊效果打開、盤面格線關掉。
 * 2＝2026-11-25 使用者指定：飛入時間 0.30 秒（範圍改成 0.20～0.40）；Touch 進場時間是新欄位，本來就用預設。
 */
export const VIEW_LINEUP = 2;
const VIEW_RESETS: Record<number, readonly ViewKey[]> = {
  1: ['noteColors', 'hitEffects', 'showBoardGrid'],
  2: ['approachSeconds'],
};

interface StoredSettings {
  view: Partial<Record<ViewKey, unknown>>;
  config: Partial<ConfigDraft>;
  firstSeconds: number;
  /** 存檔時的主卡版號（contract.ts 的 MAIN_LINEUP）；2026-11-05 之前的存檔沒有，視為 1。 */
  lineup?: number;
  /** 存檔時的顯示預設值版號（VIEW_LINEUP）；2026-11-23 之前的存檔沒有，視為 0。 */
  viewLineup?: number;
}

/** 從 from 那一代到現在，預設值換過、不該沿用存檔的欄位。 */
function resetKeysSince(from: number): Set<ViewKey> {
  const keys = new Set<ViewKey>();
  for (let generation = from + 1; generation <= VIEW_LINEUP; generation += 1) {
    for (const key of VIEW_RESETS[generation] ?? []) keys.add(key);
  }
  return keys;
}

function sameShape(value: unknown, reference: unknown): boolean {
  if (typeof value !== typeof reference) return false;
  if (typeof reference === 'number') return Number.isFinite(value as number);
  if (reference && typeof reference === 'object') {
    if (!value || typeof value !== 'object') return false;
    return Object.keys(reference).every((key) =>
      sameShape((value as Record<string, unknown>)[key], (reference as Record<string, unknown>)[key]),
    );
  }
  return true;
}

function snapshot(): StoredSettings {
  const stored: StoredSettings['view'] = {};
  for (const key of VIEW_KEYS) stored[key] = $state.snapshot(view[key]);
  return {
    view: stored,
    config: $state.snapshot(session.config) as ConfigDraft,
    firstSeconds: session.firstSeconds,
    lineup: MAIN_LINEUP,
    viewLineup: VIEW_LINEUP,
  };
}

function apply(stored: StoredSettings): void {
  const target = view as unknown as Record<string, unknown>;
  // 預設值換代前的存檔：換掉的那幾個欄位用新的預設，其餘照存檔。
  const reset = resetKeysSince(stored.viewLineup ?? 0);
  for (const key of VIEW_KEYS) {
    if (reset.has(key)) continue;
    const value = stored.view?.[key];
    // 型別與目前預設不同（舊版欄位、損壞資料）就保留預設值。
    if (value !== undefined && sameShape(value, target[key])) target[key] = value;
  }
  if (stored.config && typeof stored.config === 'object') {
    // 逐欄合併並補上加入 V3 前沒有的 v3 欄位；已選的評分方式與各版數值原樣保留。
    const draft = migrateDraft(stored.config, session.config);
    // 主卡換代前的存檔選的是當時的主卡：一代一代換成這一代對應的主卡（只在讀到舊版號時做一次）。
    const lineup = stored.lineup ?? 1;
    session.config = lineup < MAIN_LINEUP ? promoteMainCards(draft, lineup) : draft;
  }
  if (typeof stored.firstSeconds === 'number' && Number.isFinite(stored.firstSeconds)) {
    session.firstSeconds = stored.firstSeconds;
  }
}

let started = false;

/**
 * 顯示設定與參數寫入資料庫。先讀回上次的值，再開始監看變更，
 * 避免啟動時用預設值蓋掉已保存的設定。
 */
export async function startSettingsPersistence(): Promise<void> {
  if (started) return;
  started = true;
  const stored = await dbGet<StoredSettings>(STORE_SETTINGS, SETTINGS_KEY);
  if (stored) {
    apply(stored);
    // 剛換過代就先寫回去記下新的版號，免得使用者沒再改任何設定時，下次啟動又把那幾個欄位換回預設。
    if ((stored.viewLineup ?? 0) < VIEW_LINEUP) void dbPut(STORE_SETTINGS, snapshot(), SETTINGS_KEY);
  }

  $effect.root(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let first = true;
    $effect(() => {
      const value = snapshot();
      if (first) {
        first = false;
        return;
      }
      clearTimeout(timer);
      timer = setTimeout(() => void dbPut(STORE_SETTINGS, value, SETTINGS_KEY), SAVE_DELAY_MS);
    });
  });
}
