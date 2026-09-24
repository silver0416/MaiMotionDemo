// 主視窗這一端的影片同步：把譜面狀態、選到的音符與播放狀態送給影片視窗，
// 接收影片視窗的影片連結、跟隨時間與標註快捷鍵。

import { isDesktop } from '../lib/api';
import { progress } from '../lib/annotation';
import { stepLabel } from '../lib/notes';
import { openChannel, searchQuery, videoApi, type ChartState, type ToMain, type ToVideo } from '../lib/video';
import { annotation } from './annotation.svelte';
import { playback } from './playback.svelte';
import { maidataField, recordTitle, records, WIKI_TYPE_LABEL, wikiDifficultyLabel } from './records.svelte';
import { session } from './session.svelte';

const MAIDATA_DIFFICULTIES = ['EASY', 'BASIC', 'ADVANCED', 'EXPERT', 'MASTER', 'Re:MASTER', 'ORIGINAL'];
/** 選到音符之後這段時間內，播放時間的變動視為選取造成，不另外通知影片。 */
const CUE_QUIET_MS = 400;
/** 主視窗播放時，定期讓影片校正時間。 */
const RESYNC_MS = 2000;

/** 搜尋用的曲名與難度。 */
function queryFor(): string {
  const record = records.active;
  if (!record) return '';
  if (record.wiki) {
    const type = record.wiki.chartType === 'deluxe' ? 'でらっくす譜面' : WIKI_TYPE_LABEL[record.wiki.chartType];
    return searchQuery(`${record.wiki.title} ${type}`, wikiDifficultyLabel(record.wiki.difficulty));
  }
  const title = record.majdata?.title || maidataField(record.source, 'title') || recordTitle(record);
  // 解析時取難度最高的 &inote_N。
  let level = -1;
  for (let index = 7; index >= 1; index -= 1) {
    if (new RegExp(`^&inote_${index}=`, 'm').test(record.source)) {
      level = index - 1;
      break;
    }
  }
  return searchQuery(title, MAIDATA_DIFFICULTIES[level] ?? '');
}

class VideoSync {
  #channel: ReturnType<typeof openChannel<ToMain>> | null = null;
  /** 影片視窗最後一次送來的跟隨時間；主視窗因此 seek 時不回送。 */
  #followed = Number.NaN;
  #cueAt = 0;
  #lastSentTime = Number.NaN;
  #resync: ReturnType<typeof setInterval> | undefined;
  /** 影片視窗要求的暫停不再通知回去（影片已經自己停在正確位置）。 */
  #silentPause = false;
  #lastPlaying: boolean | null = null;
  #lastRate = Number.NaN;

  /** 目前要給影片視窗的譜面狀態。 */
  get chartState(): ChartState {
    const record = records.active;
    const notes = annotation.ordered;
    const step = annotation.currentStep;
    const stats = progress(annotation.draft, notes);
    return {
      recordId: record?.id ?? null,
      title: record ? recordTitle(record) : '',
      query: queryFor(),
      ready: !!record && !!session.chart && annotation.keysReady && annotation.recordId === record.id,
      firstTime: notes[0]?.timeSeconds ?? null,
      lastTime: notes.at(-1)?.timeSeconds ?? null,
      range: playback.hasRange ? { start: playback.startSeconds, end: playback.endSeconds } : null,
      selected: step
        ? { id: step.note.id, time: step.time, label: stepLabel(step, session.pathById) }
        : null,
      link: annotation.draft.video ?? null,
      done: stats.done,
      total: stats.total,
    };
  }

  /** App 掛載時呼叫；回傳清理函式。 */
  start(): () => void {
    this.#channel = openChannel<ToMain>('main', (message) => this.#handle(message));
    return () => {
      this.#channel?.close();
      this.#channel = null;
      clearInterval(this.#resync);
    };
  }

  send(message: ToVideo): void {
    this.#channel?.send(message);
  }

  sendState(): void {
    this.send({ type: 'state', state: $state.snapshot(this.chartState) as ChartState });
  }

  /** 開啟（或切到）影片視窗。 */
  async open(): Promise<void> {
    if (isDesktop()) {
      await videoApi.openWindow();
      return;
    }
    const url = new URL(window.location.href);
    url.search = '?view=video';
    url.hash = '';
    window.open(url.toString(), 'maimotion-video', 'popup,width=1120,height=720');
  }

  /** 選到新的音符：影片跳到它。 */
  cue(noteId: string, time: number): void {
    this.#cueAt = performance.now();
    this.send({ type: 'cue', time, noteId });
  }

  /** 主視窗開始或停止播放、改變倍率。 */
  playbackChanged(playing: boolean, rate: number): void {
    const rateChanged = rate !== this.#lastRate;
    const playingChanged = playing !== this.#lastPlaying;
    this.#lastRate = rate;
    this.#lastPlaying = playing;
    if (rateChanged) this.send({ type: 'rate', rate });
    // 暫停中只改倍率：不送播放狀態，免得把影片拉回主視窗的時間。
    if (!playingChanged && !playing) return;
    clearInterval(this.#resync);
    if (this.#silentPause && !playing) {
      this.#silentPause = false;
      return;
    }
    this.#silentPause = false;
    this.send({ type: 'playback', playing, time: playback.time, rate });
    if (playing) {
      this.#resync = setInterval(() => {
        if (playback.playing) this.send({ type: 'playback', playing: true, time: playback.time, rate: playback.rate });
      }, RESYNC_MS);
    }
  }

  /** 主視窗暫停時拖曳時間軸：影片跟著移動。跟隨影片或選取音符造成的變動不回送。 */
  timeChanged(time: number): void {
    if (playback.playing) return;
    if (Math.abs(time - this.#followed) < 1e-4) return;
    if (performance.now() - this.#cueAt < CUE_QUIET_MS) return;
    if (Math.abs(time - this.#lastSentTime) < 1e-4) return;
    this.#lastSentTime = time;
    this.send({ type: 'playback', playing: false, time, rate: playback.rate });
  }

  #pauseQuietly(): void {
    if (!playback.playing) return;
    this.#silentPause = true;
    playback.pause();
  }

  #handle(message: ToMain): void {
    switch (message.type) {
      case 'hello':
        this.sendState();
        this.send({ type: 'rate', rate: playback.rate });
        break;
      case 'link':
        annotation.setVideo(message.link);
        break;
      case 'follow':
        // 影片視窗接手（例如播放中選到音符開始播片段）：主視窗停下改跟影片。
        if (playback.playing) this.#pauseQuietly();
        playback.seek(message.time);
        // 超出譜面範圍時 seek 會夾住；記夾過的值，才不會又回送給影片。
        this.#followed = playback.time;
        break;
      case 'transport':
        if (message.action === 'pause') this.#pauseQuietly();
        playback.seek(message.time);
        this.#followed = playback.time;
        if (message.action === 'play') playback.play();
        break;
      case 'rate':
        playback.setRate(message.rate);
        break;
      case 'key':
        annotation.handleKey({ key: message.key, shiftKey: message.shiftKey } as KeyboardEvent);
        break;
      case 'step':
        annotation.step(message.direction);
        break;
    }
  }
}

export const videoSync = new VideoSync();
