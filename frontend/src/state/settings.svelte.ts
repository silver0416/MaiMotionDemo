import { migrateDraft } from '../lib/contract';
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
  'showCalibrationRing',
  'zoom',
  'calibration',
] as const;

type ViewKey = (typeof VIEW_KEYS)[number];

interface StoredSettings {
  view: Partial<Record<ViewKey, unknown>>;
  config: Partial<ConfigDraft>;
  firstSeconds: number;
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
  };
}

function apply(stored: StoredSettings): void {
  const target = view as unknown as Record<string, unknown>;
  for (const key of VIEW_KEYS) {
    const value = stored.view?.[key];
    // 型別與目前預設不同（舊版欄位、損壞資料）就保留預設值。
    if (value !== undefined && sameShape(value, target[key])) target[key] = value;
  }
  if (stored.config && typeof stored.config === 'object') {
    // 逐欄合併並補上加入 V3 前沒有的 v3 欄位；已選的評分方式與各版數值原樣保留。
    session.config = migrateDraft(stored.config, session.config);
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
  if (stored) apply(stored);

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
