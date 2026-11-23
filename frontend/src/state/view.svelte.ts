import { DEFAULT_CALIBRATION, type Calibration } from '../lib/disc';

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
  approachSeconds = $state(0.85);
  /** 音符照遊戲的配色上色；關掉是中性的白與灰。只換顏色，形狀與時間都不變。 */
  noteColors = $state(false);
  /** 打擊效果：判定那一刻在原地擴散的光環。預設關閉；時間完全由播放時鐘決定。 */
  hitEffects = $state(false);
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
  /** 盤面裡面的格線（內圈 16 塊感應區磚與外區的點線）。關掉只留外環與八個鍵位白點。 */
  showBoardGrid = $state(true);
  showCalibrationRing = $state(false);
  zoom = $state(1);
  calibration = $state<Calibration>({ ...DEFAULT_CALIBRATION });

  resetCalibration(): void {
    this.calibration = { ...DEFAULT_CALIBRATION };
    this.zoom = 1;
  }
}

export const view = new ViewSettings();
