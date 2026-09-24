<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import Icon from '../components/Icon.svelte';
  import VideoFinder from './VideoFinder.svelte';
  import DownloadError from './DownloadError.svelte';
  import { openExternalUrl } from '../lib/api';
  import { formatClock, formatDelta } from '../lib/format';
  import {
    fileSource,
    openChannel,
    pickVideoFile,
    sameVideo,
    videoApi,
    youtubeUrl,
    type ChartState,
    type ToVideo,
    type VideoEntry,
    type VideoLink,
  } from '../lib/video';
  import { VideoLibrary } from '../state/videoLibrary.svelte';

  type Side = 'sync' | 'find';
  type CueMode = 'segment' | 'jump' | 'off';
  interface CueSettings {
    mode: CueMode;
    /** 判定前播放幾秒 */
    pre: number;
    /** 判定後播放幾秒 */
    post: number;
    /** 片段播放速度 */
    rate: number;
    /** 片段播完停回判定那一格 */
    land: boolean;
  }

  const CUE_KEY = 'maimotion.video-cue.v1';
  const DEFAULT_CUE: CueSettings = { mode: 'segment', pre: 0.4, post: 0.3, rate: 0.5, land: true };
  /** 與主視窗相同的倍率選項；兩邊共用同一個倍率。 */
  const RATES = [0.25, 0.5, 1, 2];
  const CUE_MODES: { id: CueMode; label: string; hint: string }[] = [
    { id: 'segment', label: '播放片段', hint: '從判定前一點慢速播到判定後' },
    { id: 'jump', label: '只跳過去', hint: '停在判定那一格' },
    { id: 'off', label: '不跟隨', hint: '選音符時影片不動' },
  ];
  /** 找不到主視窗的等待時間 */
  const HELLO_WAIT_MS = 1500;
  const FOLLOW_INTERVAL_MS = 33;
  /**
   * 主視窗帶動播放時的對時：小偏差用微調播放速度追上（不跳格），
   * 超過 HARD_DRIFT 秒（例如循環跳回）才直接跳過去。
   */
  const HARD_DRIFT = 0.3;
  /** 偏差超過 DRIFT_START 開始微調，回到 DRIFT_STOP 以內才恢復原速，避免在門檻附近來回切換。 */
  const DRIFT_START = 0.02;
  const DRIFT_STOP = 0.005;
  const DRIFT_GAIN = 1;
  const DRIFT_MAX_ADJUST = 0.1;
  /** 使用者在影片視窗操作後，這段時間內不接受主視窗的時間校正 */
  const TOUCH_QUIET_MS = 600;

  function loadCue(): CueSettings {
    try {
      const raw = JSON.parse(localStorage.getItem(CUE_KEY) ?? 'null') as Partial<CueSettings> | null;
      if (raw && typeof raw === 'object') {
        const number = (value: unknown, fallback: number, min: number, max: number) =>
          typeof value === 'number' && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
        return {
          mode: raw.mode === 'jump' || raw.mode === 'off' ? raw.mode : 'segment',
          pre: number(raw.pre, DEFAULT_CUE.pre, 0, 3),
          post: number(raw.post, DEFAULT_CUE.post, 0, 3),
          rate: number(raw.rate, DEFAULT_CUE.rate, 0.1, 1),
          land: raw.land !== false,
        };
      }
    } catch {
      // 讀不到就用預設值。
    }
    return { ...DEFAULT_CUE };
  }

  const library = new VideoLibrary();
  let channel: ReturnType<typeof openChannel<ToVideo>> | null = null;

  let chart = $state<ChartState | null>(null);
  let lonely = $state(false);
  let link = $state<VideoLink | null>(null);
  let side = $state<Side>('sync');
  let autoSide = true;

  let video: HTMLVideoElement | null = $state(null);
  let src = $state<string | null>(null);
  let srcKey = '';
  let problem = $state<string | null>(null);
  let missing = $state(false);
  let current = $state(0);
  let duration = $state(0);
  let paused = $state(true);
  let userRate = $state(1);
  let cue = $state<CueSettings>(loadCue());
  let fileInput: HTMLInputElement | null = $state(null);

  /** 跳轉進行中時只記最新的目標，這次完成後再跳，避免連續跳轉互相中斷、畫面卡住不更新。 */
  let pendingSeek: number | null = null;
  /** 主視窗帶動時，主視窗的時鐘：at（Date.now()）那一刻的譜面時間與倍率。 */
  let mainClock: { time: number; at: number; rate: number } | null = null;
  let correcting = false;
  /** 正按著進度條拖曳：不用播放位置覆寫拇指，免得和滑鼠互相拉扯。 */
  let scrubbing = false;

  /** 片段播放中：播到 end 停下，視設定停回 hit。 */
  let segment: { end: number; hit: number } | null = null;
  /** 誰在帶動播放：影片自己、主視窗、還是選音符的片段。主視窗帶動時不回送跟隨時間。 */
  let leader: 'video' | 'main' | 'cue' | null = null;
  let lastCue: number | null = null;
  /** 使用者最後一次在影片視窗接手或拖動的時刻；之前送出、晚到的主視窗校正不理會。 */
  let touchedAt = 0;
  let followedAt = 0;
  let followedTime = Number.NaN;
  /** 瀏覽器預覽選的本機檔：代號 → object URL（只在這次開啟有效）。 */
  const browserFiles = new Map<string, string>();

  const offset = $derived(link?.offset ?? null);
  const synced = $derived(offset !== null);
  const fps = $derived(link?.fps && link.fps > 0 ? link.fps : 60);
  const chartTime = $derived(offset === null ? null : current - offset);

  $effect(() => {
    const value = $state.snapshot(cue);
    try {
      localStorage.setItem(CUE_KEY, JSON.stringify(value));
    } catch {
      // 無法保存只影響下次開啟的預設值。
    }
  });

  $effect(() => {
    document.title = chart?.title ? `影片同步比對 - ${chart.title}` : '影片同步比對';
  });

  // 連結或已下載清單變了就重新找影片檔。
  $effect(() => {
    const target = link;
    void library.entries;
    untrack(() => void resolve(target));
  });

  async function resolve(target: VideoLink | null): Promise<void> {
    const key = target ? (target.kind === 'youtube' ? `y:${target.id}` : `l:${target.path}`) : '';
    let next: string | null = null;
    let nextProblem: string | null = null;
    let nextMissing = false;
    if (target?.kind === 'youtube' && target.id) {
      if (!library.desktop) {
        nextProblem = '瀏覽器預覽無法播放下載的影片，請改用桌面版或開啟本機影片。';
      } else {
        const entry = library.entry(target.id) ?? (await videoApi.get(target.id).catch(() => null));
        if (entry) {
          next = await fileSource(entry.path);
          if (!target.fps && entry.fps) setLink({ ...target, fps: entry.fps });
        } else {
          nextMissing = true;
        }
      }
    } else if (target?.kind === 'local' && target.path) {
      if (library.desktop) {
        try {
          await videoApi.openLocal(target.path);
          next = await fileSource(target.path);
        } catch (error) {
          nextProblem = `找不到本機影片：${target.path}（${typeof error === 'string' ? error : String(error)}）`;
        }
      } else {
        next = browserFiles.get(target.path) ?? null;
        if (!next) nextProblem = '瀏覽器預覽需要重新選擇本機影片檔。';
      }
    }
    // 解析期間連結又換了：以最新的為準。
    if (target !== link) return;
    problem = nextProblem;
    missing = nextMissing;
    if (key !== srcKey || next !== src) {
      srcKey = key;
      src = next;
      segment = null;
      leader = null;
    }
  }

  function setLink(next: VideoLink | null): void {
    link = next;
    channel?.send({ type: 'link', link: next ? ($state.snapshot(next) as VideoLink) : null });
  }

  function onMessage(message: ToVideo): void {
    switch (message.type) {
      case 'state': {
        const firstState = chart === null;
        chart = message.state;
        lonely = false;
        // 內容相同就不換物件，免得重新找影片檔。
        if (JSON.stringify(message.state.link) !== JSON.stringify($state.snapshot(link))) link = message.state.link;
        if (autoSide && (firstState || !link)) side = link ? 'sync' : 'find';
        break;
      }
      case 'cue':
        lastCue = message.time;
        runCue(message.time);
        break;
      case 'playback':
        followMain(message.playing, message.time, message.rate, message.at);
        break;
      case 'rate':
        userRate = message.rate;
        if (video && !segment) video.playbackRate = message.rate;
        break;
    }
  }

  onMount(() => {
    channel = openChannel<ToVideo>('video', onMessage);
    channel.send({ type: 'hello' });
    const lonelyTimer = setTimeout(() => {
      if (!chart) lonely = true;
    }, HELLO_WAIT_MS);
    void library.init();
    let raf = requestAnimationFrame(function loop() {
      tick();
      raf = requestAnimationFrame(loop);
    });
    // 視窗被遮住時 requestAnimationFrame 會停，片段仍要準時停下。
    const timer = setInterval(tick, 50);
    return () => {
      clearTimeout(lonelyTimer);
      cancelAnimationFrame(raf);
      clearInterval(timer);
      channel?.close();
      library.dispose();
      for (const url of browserFiles.values()) URL.revokeObjectURL(url);
    };
  });

  /** 目前（或即將）顯示的影片時間：跳轉排隊中時為最後要求的位置。 */
  function position(): number {
    return pendingSeek ?? video?.currentTime ?? 0;
  }

  function mainExpected(): number | null {
    if (!mainClock || offset === null) return null;
    return mainClock.time + ((Date.now() - mainClock.at) / 1000) * mainClock.rate + offset;
  }

  /** 主視窗帶動播放中：依偏差微調播放速度追上主視窗，偏差太大才跳。 */
  function trackMain(): void {
    if (!video || leader !== 'main' || !mainClock || video.paused || video.seeking || segment) return;
    const expected = mainExpected();
    if (expected === null) return;
    const drift = video.currentTime - expected;
    const base = mainClock.rate;
    if (Math.abs(drift) > HARD_DRIFT * Math.max(1, base)) {
      video.playbackRate = base;
      seek(expected);
      return;
    }
    if (Math.abs(drift) > DRIFT_START) correcting = true;
    else if (Math.abs(drift) < DRIFT_STOP) correcting = false;
    const adjust = correcting ? Math.max(-DRIFT_MAX_ADJUST, Math.min(DRIFT_MAX_ADJUST, -drift * DRIFT_GAIN)) : 0;
    const rate = base * (1 + adjust);
    if (Math.abs(video.playbackRate - rate) > 0.002) video.playbackRate = rate;
  }

  function tick(): void {
    if (!video) return;
    if (!scrubbing) current = position();
    paused = video.paused;
    trackMain();
    if (segment && current >= segment.end - 1e-3) {
      video.pause();
      if (cue.land) video.currentTime = segment.hit;
      segment = null;
      video.playbackRate = userRate;
    }
    // 只有使用者操作影片或播放片段時，主視窗才跟著影片；載入、被主視窗帶動時不回送。
    if ((leader !== 'video' && leader !== 'cue') || offset === null) return;
    const now = performance.now();
    // followedTime 一開始是 NaN：用「不是很接近」判斷，第一次一定會送。
    if (!(Math.abs(current - followedTime) <= 5e-4) && now - followedAt >= FOLLOW_INTERVAL_MS) {
      followedAt = now;
      followedTime = current;
      channel?.send({ type: 'follow', time: current - offset });
    }
  }

  function seek(time: number): void {
    if (!video) return;
    const end = Number.isFinite(video.duration) ? video.duration : Infinity;
    const target = Math.min(Math.max(0, time), end);
    if (video.seeking) {
      pendingSeek = target;
      return;
    }
    pendingSeek = null;
    video.currentTime = target;
  }

  function onSeeked(): void {
    if (!video || pendingSeek === null) return;
    const target = pendingSeek;
    pendingSeek = null;
    if (Math.abs(video.currentTime - target) > 1e-4) video.currentTime = target;
  }

  function runCue(time: number): void {
    if (!video || !src || offset === null || cue.mode === 'off') return;
    const target = time + offset;
    leader = 'cue';
    if (cue.mode === 'jump') {
      segment = null;
      video.pause();
      video.playbackRate = userRate;
      seek(target);
      return;
    }
    video.playbackRate = cue.rate;
    seek(target - cue.pre);
    const current = { end: target + cue.post, hit: target };
    segment = current;
    void video.play().catch(() => {
      // 播放被打斷（例如視窗被縮小時系統暫停影片）：放棄這段片段。
      if (segment === current) segment = null;
    });
  }

  /** 片段還沒播完就停了（使用者或系統暫停）：放棄片段，恢復正常操作。 */
  function onPause(): void {
    if (segment && video && video.currentTime < segment.end - 0.01) {
      segment = null;
      video.playbackRate = userRate;
    }
  }

  function replay(): void {
    const time = lastCue ?? chart?.selected?.time;
    if (time !== undefined && time !== null) runCue(time);
  }

  function followMain(playing: boolean, time: number, rate: number, at: number): void {
    userRate = rate;
    if (!video || !src || offset === null) return;
    const recent = performance.now() - touchedAt < TOUCH_QUIET_MS;
    if (playing) {
      // 剛接手（暫停、逐格）時，途中的播放校正是舊的。
      if (recent && leader !== 'main') return;
      const starting = leader !== 'main' || video.paused;
      // 剛在影片視窗拖動或點進度條：本地時鐘已經改到新位置，途中送來的舊校正不理會。
      if (recent && !starting) return;
      leader = 'main';
      segment = null;
      // 扣掉訊息傳遞的時間；兩個視窗的 Date.now() 是同一個時鐘。
      mainClock = { time, at: Math.min(at, Date.now()), rate };
      const expected = mainExpected() ?? time + offset;
      if (starting) {
        video.playbackRate = rate;
        correcting = false;
        if (Math.abs(position() - expected) > DRIFT_STOP) seek(expected);
        if (video.paused) void video.play().catch(() => {});
      } else {
        trackMain();
      }
      return;
    }
    const target = time + offset;
    mainClock = null;
    if (leader === 'main') {
      video.pause();
      video.playbackRate = userRate;
      leader = null;
    }
    if (segment) return;
    // 影片自己在播：主視窗停下的通知不把影片拉回。
    if (leader === 'video' && !video.paused) return;
    // 改由主視窗決定位置（拖時間軸）：影片不再回報，免得較舊的位置把主視窗拉回去。
    leader = null;
    if (Math.abs(position() - target) > 0.005) {
      video.pause();
      seek(target);
    }
  }

  // ---- 播放控制

  /** 主視窗正在帶動播放。 */
  function mainLeading(): boolean {
    return leader === 'main' && offset !== null;
  }

  /** 主視窗播放中在影片視窗操作：先停下主視窗，改由影片帶動。 */
  function takeOver(): void {
    touchedAt = performance.now();
    if (mainLeading() && video) {
      channel?.send({ type: 'transport', action: 'pause', time: position() - (offset ?? 0) });
    }
    leader = 'video';
    segment = null;
    if (video) video.playbackRate = userRate;
  }

  /** 已對齊且畫面在主視窗時間軸範圍內：播放交給主視窗帶動，兩邊的播放狀態一致。 */
  function mainCanLead(): boolean {
    const range = chart?.range;
    if (!video || offset === null || !range || lonely) return false;
    const time = position() - offset;
    return time >= range.start && time < range.end - 1e-3;
  }

  function togglePlay(): void {
    if (!video || !src) return;
    if (video.paused) {
      if (mainCanLead()) {
        leader = 'main';
        segment = null;
        video.playbackRate = userRate;
        channel?.send({ type: 'transport', action: 'play', time: position() - (offset ?? 0) });
        void video.play().catch(() => {});
        return;
      }
      takeOver();
      void video.play().catch(() => {});
    } else if (mainLeading()) {
      // 主視窗帶動中按暫停：兩邊一起停。
      takeOver();
      video.pause();
    } else {
      video.pause();
    }
  }

  /** 主視窗播放中拖動影片：主視窗跳到對應時間並繼續一起播放。 */
  function seekWhileMainLeads(time: number): void {
    if (!video) return;
    touchedAt = performance.now();
    seek(time);
    const chartTime = position() - (offset ?? 0);
    // 主視窗會跳到同一個位置繼續播；先把本地的主視窗時鐘改過去，對時才不會拉回舊位置。
    if (mainClock) mainClock = { time: chartTime, at: Date.now(), rate: mainClock.rate };
    channel?.send({ type: 'transport', action: 'seek', time: chartTime });
  }

  function stepFrame(direction: 1 | -1): void {
    if (!video || !src) return;
    takeOver();
    video.pause();
    seek(position() + direction / fps);
  }

  function jump(seconds: number): void {
    if (!video || !src) return;
    if (mainLeading()) {
      seekWhileMainLeads(position() + seconds);
      return;
    }
    takeOver();
    seek(position() + seconds);
  }

  function setRate(rate: number): void {
    userRate = rate;
    if (video && !segment) video.playbackRate = rate;
    channel?.send({ type: 'rate', rate });
  }

  function onSeekInput(event: Event): void {
    const time = Number((event.currentTarget as HTMLInputElement).value);
    current = time;
    if (mainLeading()) {
      seekWhileMainLeads(time);
      return;
    }
    takeOver();
    seek(time);
  }

  // ---- 對齊

  function alignTo(time: number | null | undefined): void {
    if (!link || !video || time === null || time === undefined) return;
    const next = Math.round((position() - time) * 10000) / 10000;
    setLink({ ...link, offset: next });
    // 對齊當下就讓主視窗跳到對應時間，不必等影片再動一次。
    takeOver();
    followedTime = Number.NaN;
  }

  function nudge(delta: number): void {
    if (!link || offset === null) return;
    const next = Math.round((offset + delta) * 10000) / 10000;
    setLink({ ...link, offset: next });
    const anchor = chart?.selected?.time ?? chart?.firstTime;
    if (anchor !== null && anchor !== undefined && video) {
      takeOver();
      video.pause();
      seek(anchor + next);
    }
  }

  function showAt(time: number | null | undefined): void {
    if (time === null || time === undefined || offset === null || !video) return;
    takeOver();
    video.pause();
    seek(time + offset);
  }

  // ---- 選影片

  function useEntry(entry: VideoEntry): void {
    const keep = link && link.kind === 'youtube' && link.id === entry.id ? link : null;
    setLink({
      kind: 'youtube',
      id: entry.id,
      title: entry.title,
      ...(entry.channel ? { channel: entry.channel } : {}),
      ...(entry.fps ? { fps: entry.fps } : {}),
      offset: keep?.offset ?? null,
      ...(keep?.mirror ? { mirror: true } : {}),
    });
    autoSide = false;
    side = 'sync';
  }

  async function openLocal(): Promise<void> {
    if (!library.desktop) {
      fileInput?.click();
      return;
    }
    try {
      const path = await pickVideoFile();
      if (!path) return;
      const info = await videoApi.openLocal(path);
      const candidate: VideoLink = { kind: 'local', path: info.path, title: info.name, offset: null };
      const keep = sameVideo(link, candidate) ? link : null;
      setLink({ ...candidate, offset: keep?.offset ?? null, ...(keep?.mirror ? { mirror: true } : {}) });
      autoSide = false;
      side = 'sync';
    } catch (error) {
      problem = typeof error === 'string' ? error : String(error);
    }
  }

  function pickBrowserFile(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    const path = `browser:${file.name}`;
    const old = browserFiles.get(path);
    if (old) URL.revokeObjectURL(old);
    browserFiles.set(path, URL.createObjectURL(file));
    const candidate: VideoLink = { kind: 'local', path, title: file.name, offset: null };
    const keep = sameVideo(link, candidate) ? link : null;
    // 同一個檔案重新選：保留對齊。
    setLink({ ...candidate, offset: keep?.offset ?? null, ...(keep?.mirror ? { mirror: true } : {}) });
    if (keep) void resolve(link);
    autoSide = false;
    side = 'sync';
  }

  async function downloadLinked(): Promise<void> {
    if (link?.kind !== 'youtube' || !link.id) return;
    await library.download({ id: link.id, title: link.title, channel: link.channel });
  }

  function unlink(): void {
    setLink(null);
    autoSide = false;
    side = 'find';
  }

  // ---- 鍵盤

  function isTyping(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    return ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName) || target.isContentEditable;
  }

  function onKeydown(event: KeyboardEvent): void {
    if (isTyping(event.target) || event.ctrlKey || event.metaKey || event.altKey) return;
    const key = event.key.toLowerCase();
    let handled = true;
    switch (event.key) {
      case ' ':
        togglePlay();
        break;
      case 'ArrowLeft':
        if (event.shiftKey) jump(-1);
        else stepFrame(-1);
        break;
      case 'ArrowRight':
        if (event.shiftKey) jump(1);
        else stepFrame(1);
        break;
      case 'ArrowUp':
        channel?.send({ type: 'step', direction: -1 });
        break;
      case 'ArrowDown':
        channel?.send({ type: 'step', direction: 1 });
        break;
      case 'Enter':
        if (event.target instanceof HTMLButtonElement) handled = false;
        else channel?.send({ type: 'key', key: 'Enter', shiftKey: event.shiftKey });
        break;
      case 'Delete':
      case 'Backspace':
        channel?.send({ type: 'key', key: event.key, shiftKey: event.shiftKey });
        break;
      default:
        if (key === 'r') replay();
        else if (['a', 'd', 's', 'n'].includes(key)) channel?.send({ type: 'key', key, shiftKey: event.shiftKey });
        else handled = false;
    }
    if (handled) event.preventDefault();
  }

  function onLoaded(): void {
    if (!video) return;
    duration = Number.isFinite(video.duration) ? video.duration : 0;
    video.playbackRate = userRate;
    // 已對齊時先停在目前選取的音符（沒有就第一顆）。
    const anchor = chart?.selected?.time ?? chart?.firstTime;
    if (offset !== null && anchor !== null && anchor !== undefined) seek(anchor + offset);
  }

  function onVideoError(): void {
    if (src) problem = '影片無法播放（格式不支援或檔案損壞）。';
  }

  const downloadPercent = $derived.by(() => {
    const id = link?.kind === 'youtube' ? link.id : undefined;
    const progress = id ? library.downloads[id] : undefined;
    if (!progress) return null;
    if (!progress.total || progress.downloaded === null) return 0;
    return Math.min(100, Math.round((100 * progress.downloaded) / progress.total));
  });
</script>

<!-- 在進度條外放開滑鼠也要結束拖曳。 -->
<svelte:window onkeydown={onKeydown} onpointerup={() => (scrubbing = false)} />

<div class="video-app">
  <main class="stage">
    <header class="top">
      <div class="top-title">
        <strong class="truncate">{chart?.title || '影片同步比對'}</strong>
        {#if chart && chart.total > 0}
          <span class="xsmall muted mono">已確認 {chart.done}／{chart.total}</span>
        {/if}
      </div>
      {#if chart?.selected}
        <div class="current-note small">
          <span class="mono muted">{formatClock(chart.selected.time)}</span>
          <span>{chart.selected.label}</span>
        </div>
      {/if}
    </header>

    <div class="screen">
      {#if src}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video
          bind:this={video}
          class:is-mirrored={link?.mirror}
          {src}
          muted
          playsinline
          preload="auto"
          onloadedmetadata={onLoaded}
          onseeked={onSeeked}
          onpause={onPause}
          onerror={onVideoError}
          onclick={togglePlay}
        ></video>
      {:else}
        <div class="placeholder stack-sm">
          {#if !chart}
            {#if lonely}
              <p>找不到主視窗。</p>
              <p class="small muted">請從主視窗「標註」分頁的「影片同步」開啟這個視窗。</p>
            {:else}
              <p class="small muted">連線到主視窗…</p>
            {/if}
          {:else if !chart.recordId}
            <p>主視窗還沒有開啟譜面。</p>
            <p class="small muted">先在主視窗左側開啟一筆譜面紀錄，影片會跟著那份譜面的標註保存。</p>
          {:else if missing && link}
            <p>這部影片還沒下載</p>
            <p class="small muted truncate">{link.title}</p>
            {#if downloadPercent !== null}
              <div class="progress" role="progressbar" aria-label="下載進度" aria-valuenow={downloadPercent} aria-valuemin={0} aria-valuemax={100}>
                <div class="progress-fill" style={`width:${downloadPercent}%`}></div>
              </div>
              <p class="xsmall muted mono">{downloadPercent}%</p>
            {:else}
              <div class="row row-wrap center">
                <button class="btn btn--primary" onclick={() => void downloadLinked()} disabled={!library.status?.ytDlp}>
                  <Icon name="download" size={14} />下載
                </button>
                {#if link.id}
                  <button class="btn" onclick={() => void openExternalUrl(youtubeUrl(link!.id!))}>
                    <Icon name="external-link" size={14} />在 YouTube 開啟
                  </button>
                {/if}
              </div>
              {#if !library.status?.ytDlp}
                <p class="xsmall muted">需要先在右側「找影片」下載 yt-dlp。</p>
              {/if}
              {#if link.id}
                <DownloadError {library} id={link.id} />
              {/if}
            {/if}
          {:else if problem}
            <p>{problem}</p>
            <button class="btn" onclick={() => void openLocal()}><Icon name="folder-open" size={14} />重新選擇影片</button>
          {:else}
            <p>還沒有選影片</p>
            <p class="small muted">搜尋 YouTube 上的手元影片，或開啟自己錄的影片。</p>
            <div class="row row-wrap center">
              <button class="btn btn--primary" onclick={() => ((side = 'find'), (autoSide = false))}>
                <Icon name="search" size={14} />找影片
              </button>
              <button class="btn" onclick={() => void openLocal()}><Icon name="folder-open" size={14} />開啟本機影片</button>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="controls" aria-label="影片控制">
      <input
        class="seek"
        type="range"
        min="0"
        max={duration || 0}
        step="0.001"
        value={current}
        oninput={onSeekInput}
        onpointerdown={() => (scrubbing = true)}
        onpointercancel={() => (scrubbing = false)}
        disabled={!src}
        aria-label="影片位置"
      />
      <div class="control-row">
        <button class="btn btn--icon" onclick={togglePlay} disabled={!src} aria-label={paused ? '播放' : '暫停'} title="播放／暫停（Space）">
          <Icon name={paused ? 'play' : 'pause'} size={14} />
        </button>
        <button class="btn btn--icon" onclick={() => stepFrame(-1)} disabled={!src} aria-label="上一格" title="上一格（←）">
          <Icon name="chevron-left" size={14} />
        </button>
        <button class="btn btn--icon" onclick={() => stepFrame(1)} disabled={!src} aria-label="下一格" title="下一格（→）">
          <Icon name="chevron-right" size={14} />
        </button>
        <button class="btn btn--icon" onclick={replay} disabled={!src || !synced} aria-label="重播這顆" title="重播這顆（R）">
          <Icon name="rotate-ccw" size={14} />
        </button>
        <div class="rates" role="group" aria-label="播放速度">
          {#each RATES as rate (rate)}
            <button class="btn rate" aria-pressed={userRate === rate} onclick={() => setRate(rate)} disabled={!src}>{rate}×</button>
          {/each}
        </div>
        <span class="spacer"></span>
        <div class="clock mono small">
          <span title="影片時間">影片 {formatClock(current)}</span>
          {#if chartTime === null}
            <span class="unsynced" title="按右側「設為第一顆」對齊後，拖動影片時盤面才會跟著動">譜面未對齊，盤面不會跟著動</span>
          {:else}
            <span title="換算成譜面時間">譜面 {formatClock(chartTime)}</span>
          {/if}
        </div>
      </div>
    </div>
  </main>

  <aside class="side">
    <div class="tabs" role="group" aria-label="影片面板">
      <button class="btn tab" aria-pressed={side === 'sync'} onclick={() => ((side = 'sync'), (autoSide = false))}>對齊與跟隨</button>
      <button class="btn tab" aria-pressed={side === 'find'} onclick={() => ((side = 'find'), (autoSide = false))}>找影片</button>
    </div>

    <div class="side-body scroll">
      {#if side === 'find'}
        <VideoFinder
          {library}
          query={chart?.query ?? ''}
          currentId={link?.kind === 'youtube' ? link.id : undefined}
          onUse={useEntry}
          onLocal={() => void openLocal()}
        />
      {:else}
        <section class="section stack-sm" aria-labelledby="sync-video">
          <div class="section-title" id="sync-video">影片</div>
          {#if link}
            <div class="linked">
              <div class="small linked-title" title={link.title}>{link.title || '（無標題）'}</div>
              <div class="xsmall muted">
                {link.kind === 'youtube' ? `YouTube${link.channel ? `・${link.channel}` : ''}` : '本機影片'}
              </div>
            </div>
            <div class="row row-wrap">
              <label class="check">
                <input type="checkbox" checked={!!link.mirror} onchange={(e) => setLink({ ...link!, mirror: e.currentTarget.checked || undefined })} />
                <span>左右翻轉</span>
              </label>
              <span class="spacer"></span>
              {#if link.kind === 'youtube' && link.id}
                <button class="btn btn--ghost btn--icon" onclick={() => void openExternalUrl(youtubeUrl(link!.id!))} aria-label="在 YouTube 開啟" title="在 YouTube 開啟">
                  <Icon name="external-link" size={14} />
                </button>
              {/if}
              <button class="btn" onclick={unlink}>換影片</button>
            </div>
          {:else}
            <p class="small muted">還沒有選影片。</p>
            <button class="btn" onclick={() => ((side = 'find'), (autoSide = false))}><Icon name="search" size={14} />找影片</button>
          {/if}
        </section>

        <section class="section stack-sm" aria-labelledby="sync-align">
          <div class="section-title" id="sync-align">
            對齊
            <span class="xsmall mono" class:muted={!synced}>{synced ? `偏移 ${formatDelta(offset ?? 0)} 秒` : '尚未對齊'}</span>
          </div>
          {#if !synced}
            <ol class="steps small">
              <li>播放或逐格（← →）把影片停在第一顆音符被打到的那一格。</li>
              <li>按「設為第一顆」。之後在主視窗選任何音符，影片都會跳到對應位置。</li>
            </ol>
          {/if}
          <div class="row row-wrap">
            <button class="btn" class:btn--primary={!synced} onclick={() => alignTo(chart?.firstTime)} disabled={!src || !link || chart?.firstTime === null || chart?.firstTime === undefined}>
              設為第一顆
            </button>
            <button class="btn" onclick={() => alignTo(chart?.selected?.time)} disabled={!src || !link || !chart?.selected} title={chart?.selected ? `把目前畫面對到 ${chart.selected.label}` : '主視窗沒有選取音符'}>
              設為選取的音符
            </button>
          </div>
          {#if synced}
            <div class="field">
              <span class="field-label">微調</span>
              <div class="row row-wrap">
                <button class="btn" onclick={() => nudge(-1 / fps)} title="影片往前一格對齊">−1 格</button>
                <button class="btn" onclick={() => nudge(1 / fps)} title="影片往後一格對齊">+1 格</button>
                <button class="btn" onclick={() => nudge(-0.01)}>−10ms</button>
                <button class="btn" onclick={() => nudge(0.01)}>+10ms</button>
              </div>
              <span class="field-hint">判定那一格看起來還沒打到就按「+1 格」，已經打完就按「−1 格」。</span>
            </div>
            <div class="field">
              <span class="field-label">檢查</span>
              <div class="row row-wrap">
                <button class="btn" onclick={() => showAt(chart?.firstTime)} disabled={chart?.firstTime === null}>第一顆</button>
                <button class="btn" onclick={() => showAt(chart?.selected?.time)} disabled={!chart?.selected}>選取的音符</button>
                <button class="btn" onclick={() => showAt(chart?.lastTime)} disabled={chart?.lastTime === null}>最後一顆</button>
              </div>
              <span class="field-hint">最後一顆也對得上，整首就沒有跑掉。</span>
            </div>
          {/if}
        </section>

        <section class="section stack-sm" aria-labelledby="sync-cue">
          <div class="section-title" id="sync-cue">選音符時</div>
          <div class="modes" role="radiogroup" aria-label="選音符時影片的動作">
            {#each CUE_MODES as mode (mode.id)}
              <label class="check">
                <input type="radio" name="cue-mode" value={mode.id} bind:group={cue.mode} />
                <span>{mode.label}<span class="field-hint">{mode.hint}</span></span>
              </label>
            {/each}
          </div>
          {#if cue.mode === 'segment'}
            <div class="cue-grid">
              <label class="field">
                <span class="field-label">判定前</span>
                <select class="select" bind:value={cue.pre}>
                  {#each [0.2, 0.3, 0.4, 0.6, 0.8, 1, 1.5] as value (value)}
                    <option {value}>{value} 秒</option>
                  {/each}
                </select>
              </label>
              <label class="field">
                <span class="field-label">判定後</span>
                <select class="select" bind:value={cue.post}>
                  {#each [0, 0.1, 0.2, 0.3, 0.5, 0.8] as value (value)}
                    <option {value}>{value} 秒</option>
                  {/each}
                </select>
              </label>
              <label class="field">
                <span class="field-label">速度</span>
                <select class="select" bind:value={cue.rate}>
                  {#each [0.25, 0.5, 0.75, 1] as value (value)}
                    <option {value}>{value}×</option>
                  {/each}
                </select>
              </label>
            </div>
            <label class="check">
              <input type="checkbox" bind:checked={cue.land} />
              <span>播完停回判定那一格</span>
            </label>
          {/if}
        </section>

        <section class="section stack-sm" aria-labelledby="sync-keys">
          <div class="section-title" id="sync-keys">快捷鍵</div>
          <p class="xsmall muted keys">
            <kbd>Space</kbd> 播放　<kbd>←</kbd><kbd>→</kbd> 逐格　<kbd>Shift</kbd>+<kbd>←</kbd><kbd>→</kbd> 1 秒　<kbd>R</kbd> 重播這顆<br />
            <kbd>↑</kbd><kbd>↓</kbd> 上／下一顆音符　<kbd>A</kbd> 左手　<kbd>D</kbd> 右手　<kbd>S</kbd> 信心　<kbd>Enter</kbd> 確認預填　<kbd>N</kbd> 下一顆未確認
          </p>
          <p class="xsmall muted">標註快捷鍵會送到主視窗，在這裡看影片就能直接標。</p>
        </section>
      {/if}
    </div>
  </aside>

  <input class="sr-only" type="file" accept="video/*" bind:this={fileInput} onchange={pickBrowserFile} tabindex="-1" />
</div>

<style>
  .video-app {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    height: 100vh;
    background: var(--c-bg);
  }

  .stage {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--c-border);
    background: var(--c-panel);
  }

  .top-title {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }

  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .current-note {
    display: flex;
    gap: var(--space-2);
    white-space: nowrap;
  }

  .screen {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    justify-content: center;
    min-height: 200px;
    overflow: hidden;
  }

  video {
    display: block;
    max-width: 100%;
    max-height: 100%;
    cursor: pointer;
  }

  video.is-mirrored {
    transform: scaleX(-1);
  }

  .placeholder {
    align-items: center;
    max-width: 420px;
    padding: var(--space-5);
    text-align: center;
  }

  .center {
    justify-content: center;
  }

  .progress {
    width: 240px;
    height: 4px;
    background: var(--c-control);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--c-right);
  }

  .controls {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4) var(--space-3);
    border-top: 1px solid var(--c-border);
    background: var(--c-panel);
  }

  .control-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  .rates {
    display: flex;
    gap: var(--space-1);
    margin-left: var(--space-2);
  }

  .rate {
    padding: 0 var(--space-2);
  }

  .clock {
    display: flex;
    gap: var(--space-3);
    white-space: nowrap;
  }

  .unsynced {
    font-family: var(--font-sans);
    color: var(--c-accent);
  }

  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--c-border);
    background: var(--c-panel);
  }

  .tabs {
    display: flex;
    gap: var(--space-1);
    padding: var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }

  .tab {
    flex: 1 1 0;
    min-width: 0;
  }

  .side-body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
  }

  .linked {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: var(--lh-tight);
  }

  .linked-title {
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .steps {
    margin: 0;
    padding-left: var(--space-4);
    color: var(--c-text-dim);
  }

  .modes {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .modes .field-hint {
    display: block;
  }

  .cue-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-2);
  }

  .keys {
    line-height: 2;
  }

  kbd {
    padding: 0 var(--space-1);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-sm);
    background: var(--c-control);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }

  @media (max-width: 820px) {
    .video-app {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(320px, 60vh) auto;
      height: auto;
      min-height: 100vh;
    }

    .side {
      border-left: none;
      border-top: 1px solid var(--c-border);
    }

    .side-body {
      overflow: visible;
    }
  }
</style>
