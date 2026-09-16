import { DEFAULT_CALIBRATION, type Calibration } from '../lib/disc';

export type NoteDisplayMode = 'window' | 'all';

/** Touch 落點參考標記：自動（有 Touch 譜面才顯示）、一律顯示、關閉。 */
export type SensorDisplayMode = 'auto' | 'all' | 'off';

/**
 * 顯示設定。
 * 這裡的任何變更都只影響畫面，不重新分析，也不改變任何時間資料。
 */
export class ViewSettings {
  showNotes = $state(true);
  noteMode = $state<NoteDisplayMode>('window');
  /** 音符飛入動畫，純視覺效果，不影響判定時間。 */
  approach = $state(true);
  approachSeconds = $state(0.85);
  showLeft = $state(true);
  showRight = $state(true);
  /** 整段軌跡預設關閉：長譜面會在盤面留下滿場細線，反而看不清楚目前的動作。 */
  showFullTrack = $state(false);
  showRecentTrail = $state(true);
  trailSeconds = $state(1.2);
  showAssignmentTags = $state(true);
  /** 預設自動：一般 Tap 譜面不會被 33 個落點標籤蓋住。 */
  sensorMode = $state<SensorDisplayMode>('auto');
  /** 落點編號；關掉只留外形，密集譜面可少一層文字。 */
  showSensorLabels = $state(true);
  /** 煙火修飾的擴散效果，時間完全由播放時鐘決定。 */
  showFireworks = $state(true);
  showHandovers = $state(true);
  /** 手掌覆蓋區：畫面開關而已，覆蓋範圍與時間一律由 Rust 的 palmPlacements 決定。 */
  showPalms = $state(true);
  showButtons = $state(true);
  showCalibrationRing = $state(false);
  zoom = $state(1);
  calibration = $state<Calibration>({ ...DEFAULT_CALIBRATION });

  resetCalibration(): void {
    this.calibration = { ...DEFAULT_CALIBRATION };
    this.zoom = 1;
  }
}

export const view = new ViewSettings();
