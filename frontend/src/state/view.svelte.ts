import { DEFAULT_CALIBRATION, type Calibration } from '../lib/disc';
import { APPROACH_RANGE, TOUCH_RANGE } from '../lib/noteSpeed';
import { SATURATION_RANGE } from '../lib/gameColors';

export type NoteDisplayMode = 'window' | 'all';

/** Touch 落點參考標記：自動（有 Touch 譜面才顯示）、一律顯示、關閉。 */
export type SensorDisplayMode = 'auto' | 'all' | 'off';

/**
 * 顯示設定，會寫入本機資料庫（state/settings.svelte.ts）。
 * 這裡的任何變更都只影響畫面，不重新分析，也不改變任何時間資料。
 */
export class ViewSettings {
  showNotes = $state(true);
  noteMode = $state<NoteDisplayMode>('window');
  /** 音符飛入動畫，純視覺效果，不影響判定時間。 */
  approach = $state(true);
  /**
   * 飛入時間（Tap／Hold／Slide）與 Touch 進場時間，範圍與預設值見 lib/noteSpeed.ts。
   * 2026-11-25 使用者指定改成 0.30／0.50 秒（約遊戲速度 7），更新上來的人也換過去（settings.svelte.ts 的 VIEW_LINEUP 2）。
   */
  approachSeconds = $state(APPROACH_RANGE.initial);
  touchSeconds = $state(TOUCH_RANGE.initial);
  /**
   * 音符照遊戲的配色上色；關掉是中性的白與灰。只換顏色，形狀與時間都不變。
   * 預設打開（2026-11-23 使用者指定；換預設值時要一併調 settings.svelte.ts 的 VIEW_LINEUP）。
   */
  noteColors = $state(true);
  /**
   * 照遊戲上色時的彩度（百分比）：100 是遊戲原本的顏色，越低越接近同樣明暗的灰，
   * 和介面比較搭（2026-11-25 使用者要求）。只在照遊戲上色時有作用；換算在 lib/gameColors.ts。
   */
  noteSaturation = $state<number>(SATURATION_RANGE.initial);
  /** 打擊效果：判定那一刻在原地擴散的光暈。預設打開（同上）；時間完全由播放時鐘決定。 */
  hitEffects = $state(true);
  showLeft = $state(true);
  showRight = $state(true);
  /** 整段軌跡預設關閉：長譜面會在盤面留下滿場細線，反而看不清楚目前的動作。 */
  showFullTrack = $state(false);
  showRecentTrail = $state(true);
  trailSeconds = $state(1.2);
  showAssignmentTags = $state(true);
  /** 預設關閉：盤面維持乾淨，需要對照落點時再到「顯示」打開。 */
  sensorMode = $state<SensorDisplayMode>('off');
  /** 落點編號；關掉只留外形，密集譜面可少一層文字。 */
  showSensorLabels = $state(true);
  /** 煙火修飾的擴散效果，時間完全由播放時鐘決定。 */
  showFireworks = $state(true);
  showHandovers = $state(true);
  /** 手掌覆蓋區：畫面開關而已，覆蓋範圍與時間一律由 Rust 的 palmPlacements 決定。 */
  showPalms = $state(true);
  showButtons = $state(true);
  /** 盤面裡面的格線（內圈 16 塊感應區磚與外區的點線）。預設關掉（同上），只留外環與八個鍵位白點。 */
  showBoardGrid = $state(false);
  showCalibrationRing = $state(false);
  zoom = $state(1);
  calibration = $state<Calibration>({ ...DEFAULT_CALIBRATION });

  resetCalibration(): void {
    this.calibration = { ...DEFAULT_CALIBRATION };
    this.zoom = 1;
  }
}

export const view = new ViewSettings();
