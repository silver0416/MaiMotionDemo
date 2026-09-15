export const RATES = [0.25, 0.5, 1, 2] as const;

/**
 * 單一播放時鐘。
 * 時間以 anchor（牆鐘時刻 + 對應播放時間）推算，不做每幀累加，
 * 暫停、seek 與倍率變更都重新設定 anchor，因此播放與拖曳到同一時間的姿態一致。
 * 倍率只影響這個時鐘，不影響任何分析結果。
 */
export class Playback {
  startSeconds = $state(0);
  endSeconds = $state(1);
  time = $state(0);
  playing = $state(false);
  rate = $state<number>(1);
  loopEnabled = $state(false);
  loopStart = $state(0);
  loopEnd = $state(1);

  #anchorWall = 0;
  #anchorTime = 0;

  get lowerBound(): number {
    return this.loopEnabled ? Math.max(this.startSeconds, this.loopStart) : this.startSeconds;
  }

  get upperBound(): number {
    return this.loopEnabled ? Math.min(this.endSeconds, this.loopEnd) : this.endSeconds;
  }

  get hasRange(): boolean {
    return this.endSeconds > this.startSeconds;
  }

  setRange(start: number, end: number): void {
    const safeEnd = end > start ? end : start + 1;
    this.startSeconds = start;
    this.endSeconds = safeEnd;
    this.loopStart = Math.min(Math.max(this.loopStart, start), safeEnd);
    this.loopEnd = Math.min(Math.max(this.loopEnd, this.loopStart), safeEnd);
    if (this.loopEnd - this.loopStart < 0.05) {
      this.loopStart = start;
      this.loopEnd = safeEnd;
    }
    this.seek(Math.min(Math.max(this.time, start), safeEnd));
  }

  resetRange(start: number, end: number): void {
    const safeEnd = end > start ? end : start + 1;
    this.startSeconds = start;
    this.endSeconds = safeEnd;
    this.loopEnabled = false;
    this.loopStart = start;
    this.loopEnd = safeEnd;
    this.playing = false;
    this.seek(start);
  }

  #anchor(now: number): void {
    this.#anchorWall = now;
    this.#anchorTime = this.time;
  }

  seek(time: number): void {
    const clamped = Math.min(Math.max(time, this.startSeconds), this.endSeconds);
    this.time = clamped;
    this.#anchor(typeof performance === 'undefined' ? 0 : performance.now());
  }

  nudge(delta: number): void {
    this.seek(this.time + delta);
  }

  reset(): void {
    this.playing = false;
    this.seek(this.lowerBound);
  }

  play(): void {
    if (!this.hasRange) return;
    const lo = this.lowerBound;
    const hi = this.upperBound;
    if (this.time >= hi - 1e-6 || this.time < lo) this.time = lo;
    this.playing = true;
    this.#anchor(performance.now());
  }

  pause(): void {
    if (!this.playing) return;
    this.playing = false;
    this.#anchor(performance.now());
  }

  toggle(): void {
    if (this.playing) this.pause();
    else this.play();
  }

  setRate(rate: number): void {
    if (rate <= 0 || !Number.isFinite(rate)) return;
    this.rate = rate;
    this.#anchor(performance.now());
  }

  setLoop(enabled: boolean): void {
    this.loopEnabled = enabled;
    if (enabled) {
      if (this.loopEnd - this.loopStart < 0.05) {
        this.loopStart = this.startSeconds;
        this.loopEnd = this.endSeconds;
      }
      if (this.time < this.loopStart || this.time > this.loopEnd) this.seek(this.loopStart);
      else this.#anchor(performance.now());
    }
  }

  setLoopStart(time: number): void {
    const value = Math.min(Math.max(time, this.startSeconds), this.endSeconds - 0.05);
    this.loopStart = value;
    if (this.loopEnd < value + 0.05) this.loopEnd = Math.min(value + 0.05, this.endSeconds);
    if (this.loopEnabled && this.time < value) this.seek(value);
  }

  setLoopEnd(time: number): void {
    const value = Math.min(Math.max(time, this.startSeconds + 0.05), this.endSeconds);
    this.loopEnd = value;
    if (this.loopStart > value - 0.05) this.loopStart = Math.max(value - 0.05, this.startSeconds);
    if (this.loopEnabled && this.time > value) this.seek(this.loopStart);
  }

  /** 由單一 requestAnimationFrame 迴圈呼叫。 */
  frame(now: number): void {
    if (!this.playing) return;
    const lo = this.lowerBound;
    const hi = this.upperBound;
    let time = this.#anchorTime + ((now - this.#anchorWall) / 1000) * this.rate;
    if (time >= hi) {
      if (this.loopEnabled && hi - lo > 1e-6) {
        const span = hi - lo;
        time = lo + ((time - lo) % span);
        this.#anchorWall = now;
        this.#anchorTime = time;
      } else {
        time = hi;
        this.playing = false;
        this.#anchorWall = now;
        this.#anchorTime = time;
      }
    }
    if (time < lo) time = lo;
    this.time = time;
  }
}

export const playback = new Playback();
