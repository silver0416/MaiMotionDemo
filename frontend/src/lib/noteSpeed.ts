/*
 * 「飛入時間」與「Touch 進場時間」的範圍、預設值，以及和遊戲速度設定（タップスピード／タッチスピード）的換算。
 * 2026-11-25 使用者整理的修改建議：秒數保留，但範圍縮到常用的那一段、每格 0.01 秒，
 * 滑桿上在遊戲速度 5～10 的位置標刻度；Touch 和 Tap 是遊戲裡兩個分開的設定，這裡也分開。
 *
 * SEGA 沒有公開各速度對應的秒數。換算照社群重製版 MajdataPlay 的公式，不是官方數字，只拿來標刻度。
 * 飛入動畫的常數（內圈 1.225、判定線 4.8、Touch 收攏曲線）本來就取自 MajdataPlay（見 notes.ts），單位對得上。
 */

export interface SecondsRange {
  min: number;
  max: number;
  step: number;
  /** 預設值 */
  initial: number;
}

/** 飛入時間（Tap／Hold／Slide 從內圈飛到鍵位）：0.20 約遊戲速度 11.3，0.40 約 5.1；預設 0.30 約 7.2。 */
export const APPROACH_RANGE: SecondsRange = { min: 0.2, max: 0.4, step: 0.01, initial: 0.3 };

/** Touch 進場時間（從出現到判定，前 20% 淡入、後 80% 收攏）：0.30 約速度 12.0，0.70 約 4.9；預設 0.50 約 7.0。 */
export const TOUCH_RANGE: SecondsRange = { min: 0.3, max: 0.7, step: 0.01, initial: 0.5 };

/** 從內圈飛到判定線的距離（MajdataPlay 的單位：判定線 4.8，內圈 1.225）。 */
const FLIGHT_DISTANCE = 4.8 - 1.225;

/** タップスピード → 飛入時間（秒）：MajdataPlay 的移動速度 v = 107.25 / (71.4184491 × (s + 0.9975)^−0.985558604)。 */
export function tapSpeedSeconds(speed: number): number {
  const velocity = 107.25 / (71.4184491 * Math.pow(speed + 0.9975, -0.985558604));
  return FLIGHT_DISTANCE / velocity;
}

/** タッチスピード → Touch 進場時間（秒）：MajdataPlay 的 3.209385682 × s^−0.9549621752。 */
export function touchSpeedSeconds(speed: number): number {
  return 3.209385682 * Math.pow(speed, -0.9549621752);
}

export interface SliderTick {
  value: number;
  label: string;
}

/** 標刻度的遊戲速度；超過 10 的那一段不標。 */
const MARKED_SPEEDS = [5, 6, 7, 8, 9, 10];

/**
 * 滑桿上的遊戲速度刻度。落在範圍外一點點的（飛入時間的速度 5 是 0.407 秒）放在端點上，
 * 離範圍太遠的不標。
 */
export function speedTicks(range: SecondsRange, toSeconds: (speed: number) => number): SliderTick[] {
  const slack = range.step;
  return MARKED_SPEEDS.flatMap((speed) => {
    const seconds = toSeconds(speed);
    if (seconds < range.min - slack || seconds > range.max + slack) return [];
    return [{ value: Math.min(range.max, Math.max(range.min, seconds)), label: String(speed) }];
  });
}
