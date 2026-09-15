import { DEFAULT_CALIBRATION, type Calibration } from '../lib/disc';

export type NoteDisplayMode = 'window' | 'all';

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
  showFullTrack = $state(true);
  showRecentTrail = $state(true);
  trailSeconds = $state(1.2);
  showAssignmentTags = $state(true);
  showHandovers = $state(true);
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
