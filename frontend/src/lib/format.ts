/** 時間顯示；支援負的預備時間。 */
export function formatClock(seconds: number): string {
  if (!Number.isFinite(seconds)) return '--:--.---';
  const sign = seconds < 0 ? '-' : '';
  const abs = Math.abs(seconds);
  const minutes = Math.floor(abs / 60);
  const rest = abs - minutes * 60;
  const whole = Math.floor(rest);
  const millis = Math.round((rest - whole) * 1000);
  const carrySecond = millis === 1000 ? whole + 1 : whole;
  const shownMillis = millis === 1000 ? 0 : millis;
  return `${sign}${minutes}:${String(carrySecond).padStart(2, '0')}.${String(shownMillis).padStart(3, '0')}`;
}

export function formatSeconds(seconds: number, digits = 3): string {
  if (!Number.isFinite(seconds)) return '—';
  return `${seconds.toFixed(digits)} 秒`;
}

export function formatNumber(value: number, digits = 3): string {
  if (!Number.isFinite(value)) return '—';
  if (value !== 0 && Math.abs(value) < 0.0005) return value.toExponential(1);
  return value.toFixed(digits);
}

export function formatDelta(value: number, digits = 3): string {
  if (!Number.isFinite(value)) return '—';
  if (Math.abs(value) < 5e-4) return '±0';
  return `${value > 0 ? '+' : ''}${value.toFixed(digits)}`;
}
