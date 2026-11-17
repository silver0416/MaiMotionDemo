<script lang="ts">
  import { untrack } from 'svelte';
  import { reducedMotion } from '$app/lib/press';
  import Keycap from '$lib/demos/ui/Keycap.svelte';
  import Stage from '$lib/demos/ui/Stage.svelte';
  import { DemoPlayer, type Step } from '$lib/demos/ui/player.svelte';
  import { TOURS, type Rect, type ShotStep, type Tour, type TourId } from './flows';
  import ExeSketch from './parts/ExeSketch.svelte';
  import TourSay from './parts/TourSay.svelte';

  /**
   * 實際畫面導覽：<QuickTour flow="search" />（quick 章節的 .md 自動 import）。
   * 畫面是實際 App 的截圖（scripts/capture-quick.mjs 拍的），不是重畫的介面；座標也是拍的時候量的。
   *
   * 每一步：鏡頭拉近到要點的地方 → 游標飛過去按下（琥珀色一圈）→ 拉近狀態下換成下一張截圖（交叉淡入）→ 下一步。
   * 只框起來、不按的步驟（highlight）會畫一個框。播完停在最後（拉遠看整個畫面），不循環；按「重播」再播一次。
   * 按鍵盤的步驟（key）不出現游標，截圖框下方那一列的鍵帽亮一下（鍵帽不蓋在截圖上）；連按兩下（double）游標按兩次。
   * 旁白由吉祥物講（parts/TourSay：泡泡在左、吉祥物在右）：寬的時候疊在截圖右下角，擋到這一步要點的地方就改放右上角；
   * 窄的時候放在截圖正下方一列（鍵帽列在它下面）。換句時舊句子往上淡出、新句子一個字一個字很快淡入。
   * Stage 下方那一列只剩控制鈕；旁白那一行仍在（視覺隱藏、aria-live），給輔助技術念。
   * 拍不到的系統畫面用示意畫面（tour.lead，sketch）接在最前面：圖說的徽章跟著畫面切換，示意畫面標「示意」，截圖標「實際畫面」。
   * 鏡頭就是截圖那一層的 transform（translate＋scale），只動 transform 與 opacity。
   *
   * - 拉近倍率依舞台寬度算：App 的字在畫面上至少是原尺寸的 0.95 倍、最多 1.2 倍，
   *   範圍內盡量把 focus 整塊放進來；放不下時以要按的地方為準。
   * - 「上一步／下一步」：跳到那一步開始時的樣子停住，要按的地方畫框（游標只在自動播放時出現）。
   * - prefers-reduced-motion：不拉近、不飛游標、不淡入；停在第一步，一步一張整個畫面＋框，用「上一步／下一步」切換。
   * - 沒有 JS（預先產生的 HTML）時看得到第一張圖與旁白（吉祥物泡泡直接顯示第一句，不播動畫）；文字步驟寫在 .md 裡。
   * - 圖片 lazy 載入；開始播之後才預先載入其他幾張。
   */
  interface Props {
    flow: TourId;
    caption?: string;
  }

  let { flow, caption }: Props = $props();

  const tour: Tour = untrack(() => TOURS[flow]);
  const { manifest } = tour;
  const VW = manifest.viewport.width;
  const VH = manifest.viewport.height;
  /** 示意畫面（lead）在前、截圖在後。 */
  const frames = [...(tour.lead?.frames ?? []), ...manifest.frames];
  const steps = [...(tour.lead?.steps ?? []), ...manifest.steps];
  const frameIndex = new Map(frames.map((frame, i) => [frame.id, i]));

  /** 鏡頭移動的時間（和 CSS 的 --cam-ms 一致）。 */
  const CAM_MS = 700;
  /** 換圖的交叉淡入。 */
  const FADE_MS = 260;
  /** 連按兩下：第二下在第一下之後多久、按完多久換畫面。 */
  const DOUBLE_GAP = 200;
  const DOUBLE_SWAP = 260;
  /** App 的字在畫面上的大小（相對於原尺寸）：拉近時不小於 MIN、不大於 MAX。 */
  const TEXT_MIN = 0.95;
  const TEXT_MAX = 1.2;
  /** focus 四周多留的空間（App 的 CSS 像素）。 */
  const PAD = 20;
  /** 框和要點之間的空隙（舞台像素）。 */
  const MARK_GAP = 5;

  interface Cam {
    z: number;
    x: number;
    y: number;
  }

  const FULL: Cam = { z: 1, x: 0, y: 0 };

  // ---- 狀態 -------------------------------------------------------------------

  let width = $state(0);
  /** 伺服器端算繪時是 false（matchMedia 不存在）；第一張畫面兩邊一樣，不會對不上。 */
  let reduced = $state(reducedMotion());
  /** 目前的畫面（上層，淡入）與正在淡出的畫面（下層）。 */
  let shown = $state(0);
  let under = $state<number | null>(null);
  let fadeKey = $state(0);
  let underTimer: ReturnType<typeof setTimeout> | undefined;
  /** 鏡頭要看的範圍（App 座標）；null 是整個畫面。 */
  let view = $state<{ focus: Rect; must?: Rect } | null>(null);
  /** 畫框的地方；kind 'target' 只在沒有自動播放時（或減少動態）顯示，'highlight' 一直顯示。 */
  let mark = $state<{ rect: Rect; kind: 'target' | 'highlight'; key: number } | null>(null);
  let markKey = 0;
  /** 游標要飛去的點（App 座標）。 */
  let aim = $state<Rect | null>(null);
  let anchor = $state<HTMLElement | null>(null);

  // ---- 鏡頭 -------------------------------------------------------------------

  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

  function camFor(focus: Rect, must: Rect | undefined, w: number): Cam {
    if (w <= 0) return FULL;
    const base = w / VW;
    const zMin = Math.max(1, TEXT_MIN / base);
    const zMax = Math.max(zMin, TEXT_MAX / base);
    const zFit = Math.min(VW / (focus[2] + PAD * 2), VH / (focus[3] + PAD * 2));
    const z = clamp(zFit, zMin, zMax);
    const vw = VW / z;
    const vh = VH / z;
    let x = focus[0] + focus[2] / 2 - vw / 2;
    let y = focus[1] + focus[3] / 2 - vh / 2;
    if (must) {
      // 要按的地方一定要在畫面裡；比畫面還寬（高）時對齊它的左（上）緣，輸入框的字從頭看得到。
      const m = PAD / 2;
      x = must[2] + m * 2 > vw ? must[0] - m : clamp(x, must[0] + must[2] + m - vw, must[0] - m);
      y = must[3] + m * 2 > vh ? must[1] - m : clamp(y, must[1] + must[3] + m - vh, must[1] - m);
    }
    return { z, x: clamp(x, 0, VW - vw), y: clamp(y, 0, VH - vh) };
  }

  const cam = $derived(!view || reduced ? FULL : camFor(view.focus, view.must, width));
  const base = $derived(width > 0 ? width / VW : 0);
  const camTransform = $derived(
    `translate(${(-cam.x * base * cam.z).toFixed(2)}px, ${(-cam.y * base * cam.z).toFixed(2)}px) scale(${cam.z.toFixed(4)})`,
  );

  /** App 座標的方框 → 舞台上的像素位置（目前鏡頭停好之後）。 */
  function place(rect: Rect) {
    const k = base * cam.z;
    return {
      left: (rect[0] - cam.x) * k,
      top: (rect[1] - cam.y) * k,
      width: rect[2] * k,
      height: rect[3] * k,
    };
  }

  // ---- 畫面 -------------------------------------------------------------------

  function setFrame(id: string | undefined) {
    const next = id === undefined ? 0 : (frameIndex.get(id) ?? 0);
    if (next === shown) return;
    // 一口氣跳好幾步（seek）時，淡出的是原本在畫面上的那一張。
    if (under === null) under = shown;
    shown = next;
    fadeKey += 1;
    clearTimeout(underTimer);
    underTimer = setTimeout(() => (under = null), reduced ? 0 : FADE_MS + 60);
  }

  let preloaded = false;
  function preload() {
    if (preloaded || typeof Image === 'undefined') return;
    preloaded = true;
    for (const frame of frames) {
      if (!frame.src) continue;
      const img = new Image();
      img.src = frame.src;
      void img.decode?.().catch(() => {});
    }
  }

  // ---- 腳本 -------------------------------------------------------------------

  const say = (step: ShotStep) => tour.say[step.id] ?? step.id;

  function setMark(rect: Rect | undefined, kind: 'target' | 'highlight') {
    markKey += 1;
    mark = rect ? { rect, kind, key: markKey } : null;
  }

  /** 腳本的一個小動作屬於第幾步（給「上一步／下一步」與步數用）。 */
  const logicalOf: number[] = [];
  /** 每一步的第一個小動作。 */
  const firstOf: number[] = [];
  const script: Step[] = [];
  /** 連按兩下的第二下與換畫面（reset 時要清掉）。 */
  let doubleTimers: ReturnType<typeof setTimeout>[] = [];

  function clearDouble() {
    for (const timer of doubleTimers) clearTimeout(timer);
    doubleTimers = [];
  }

  steps.forEach((step, k) => {
    const line = say(step);
    firstOf.push(script.length);
    // 1. 拉近（或拉遠）到這一步要看的地方，框起要點。
    const focus = tour.wide?.includes(step.id) ? undefined : (step.focus ?? step.target ?? step.highlight);
    script.push({
      say: line,
      run: () => {
        preload();
        clearDouble();
        player.cursor.show = false;
        setFrame(step.frame);
        view = focus ? { focus, must: step.target ?? step.highlight } : null;
        aim = step.target ?? null;
        if (step.target) setMark(step.target, 'target');
        else setMark(step.highlight, 'highlight');
      },
      hold: step.target ? 1200 : step.key ? 1000 : 2000,
    });
    logicalOf.push(k);
    // 2. 游標飛過去按下（或按鍵盤：鍵帽亮一下），換成下一張。
    if (step.target || step.after) {
      const swap = () => {
        if (!step.after) return;
        // 從示意畫面換成截圖（開檔案）時同時拉遠，看整個 App。
        if (step.double) view = null;
        setFrame(step.after);
      };
      script.push({
        say: line,
        target: () => (reduced || !step.target ? null : anchor),
        key: step.key,
        run: () => {
          setMark(undefined, 'target');
          if (step.double && !player.skipping && !reduced) {
            doubleTimers.push(
              setTimeout(() => {
                player.cursor.press += 1;
                doubleTimers.push(setTimeout(swap, DOUBLE_SWAP));
              }, DOUBLE_GAP),
            );
          } else {
            swap();
          }
        },
        hold: step.double ? 1300 + DOUBLE_GAP + DOUBLE_SWAP : 1300,
      });
      logicalOf.push(k);
    }
  });
  // 3. 最後拉遠，看整個畫面。
  script.push({
    say: tour.outro,
    run: () => {
      player.cursor.show = false;
      view = null;
      aim = null;
      setMark(undefined, 'target');
    },
    hold: 600,
  });
  logicalOf.push(steps.length - 1);

  function reset() {
    clearTimeout(underTimer);
    clearDouble();
    view = null;
    aim = null;
    mark = null;
    setFrame(frames[0]?.id);
  }

  const player = new DemoPlayer({
    intro: tour.intro,
    outro: tour.outro,
    reset,
    steps: script,
    reducedStep: 0,
  });

  // ---- 上一步／下一步 ---------------------------------------------------------

  const total = steps.length;
  const at = $derived(player.index < 0 ? -1 : logicalOf[player.index]);

  function prev() {
    if (player.index < 0) return;
    const k = at;
    // 已經在這一步中間（按下去之後）就回到這一步開頭，否則回上一步。
    player.seek(player.index > firstOf[k] ? firstOf[k] : firstOf[Math.max(0, k - 1)]);
  }

  function next() {
    player.seek(firstOf[Math.min(total - 1, at + 1)]);
  }

  const nav = $derived({
    at: player.index < 0 ? 0 : at + 1,
    total,
    prev,
    next,
    canPrev: player.index > 0,
    canNext: at < total - 1 || player.index < 0,
  });

  function onkey(key: string): boolean {
    if (key === 'ArrowLeft' && nav.canPrev) {
      prev();
      return true;
    }
    if (key === 'ArrowRight' && nav.canNext) {
      next();
      return true;
    }
    return false;
  }

  $effect(() => {
    if (typeof matchMedia !== 'function') return;
    const query = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => (reduced = query.matches);
    query.addEventListener('change', update);
    return () => {
      query.removeEventListener('change', update);
      clearTimeout(underTimer);
      clearDouble();
    };
  });

  const markBox = $derived(mark && (mark.kind === 'highlight' || !player.playing || reduced) ? place(mark.rect) : null);
  /** 游標的落點：要點的中心；中心跑出畫面（要點比畫面寬）時拉回畫面裡，但不離開要點。 */
  const aimPoint = $derived.by(() => {
    if (!aim) return null;
    const box = place(aim);
    const h = width * (VH / VW);
    const edge = 24;
    const cx = box.left + box.width / 2;
    const cy = box.top + box.height / 2;
    return {
      x: clamp(clamp(cx, edge, width - edge), box.left, box.left + box.width),
      y: clamp(clamp(cy, edge, h - edge), box.top, box.top + box.height),
    };
  });
  const ratio = `${VW} / ${VH}`;

  /** 吉祥物泡泡要避開的地方：這一句旁白那一步要按（或框起來）的地方；還沒開始與最後一句沒有。 */
  const sayAvoid = $derived.by(() => {
    if (player.index < 0 || player.index >= script.length - 1) return null;
    const step = steps[logicalOf[player.index]];
    const rect = step?.target ?? step?.highlight;
    return rect ? place(rect) : null;
  });

  /** 這一步要按的鍵：自動播放時只在按下的瞬間亮；停住（逐步檢視、減少動態）時一直亮著提示。 */
  const stepKey = $derived(player.index < 0 ? undefined : steps[logicalOf[player.index]]?.key);
  const litKey = $derived(player.lit ?? (!player.playing || reduced ? stepKey : undefined));
  const isSketch = $derived(!!frames[shown]?.sketch);
</script>

{#snippet frameView(i: number, top: boolean)}
  {@const frame = frames[i]}
  {#if frame?.sketch === 'exe-file'}
    <div class="shot" class:is-fading={top && fadeKey > 0 && !reduced} aria-hidden={!top}>
      <ExeSketch />
    </div>
  {:else if frame?.src}
    <img
      class="shot"
      class:is-fading={top && fadeKey > 0 && !reduced}
      src={frame.src}
      alt={top ? tour.label : ''}
      width={VW}
      height={VH}
      loading={top && fadeKey === 0 ? 'lazy' : 'eager'}
    />
  {/if}
{/snippet}

<figure class="quick-tour">
  <Stage label={tour.label} {player} flush steps={nav} narration="hidden" {onkey}>
    <!-- 截圖視窗＋吉祥物泡泡：寬的時候泡泡疊在截圖角落（以這一層定位），窄的時候是截圖下面一列。 -->
    <div class="screen">
      <div class="viewport" style:aspect-ratio={ratio} bind:clientWidth={width}>
        <div class="cam" style:transform={camTransform}>
          {#if under !== null && under !== shown}
            {@render frameView(under, false)}
          {/if}
          {#key fadeKey}
            {@render frameView(shown, true)}
          {/key}
        </div>
        {#if markBox && mark}
          {#key mark.key}
            <span
              class="mark"
              class:is-delayed={!reduced}
              style:left={`${markBox.left - MARK_GAP}px`}
              style:top={`${markBox.top - MARK_GAP}px`}
              style:width={`${markBox.width + MARK_GAP * 2}px`}
              style:height={`${markBox.height + MARK_GAP * 2}px`}
              aria-hidden="true"
            ></span>
          {/key}
        {/if}
        {#if aimPoint}
          <span
            class="anchor"
            bind:this={anchor}
            style:left={`${aimPoint.x}px`}
            style:top={`${aimPoint.y}px`}
            aria-hidden="true"
          ></span>
        {/if}
      </div>
      <TourSay text={player.say} step={at} {reduced} avoid={sayAvoid} {width} height={width * (VH / VW)} />
    </div>
    {#if tour.keys}
      <!-- 鍵帽列：在截圖框外面（不蓋住實際畫面），整個導覽都在，高度固定不跳；按鍵的步驟按下時亮起。 -->
      <div class="keys" aria-hidden="true">
        {#each tour.keys as k (k.cap)}
          <Keycap cap={k.cap} label={k.label} lit={litKey === k.cap} still />
        {/each}
      </div>
    {/if}
  </Stage>
  <figcaption class="quick-caption">
    {#if isSketch}
      <span class="badge badge--sample">示意</span>
    {:else}
      <span class="badge badge--live">實際畫面</span>
    {/if}
    {#if caption}<span>{caption}</span>{/if}
  </figcaption>
</figure>

<style>
  .quick-tour {
    margin: 1.5em 0;
  }

  /* 截圖視窗＋吉祥物泡泡：泡泡寬的時候以這一層定位（疊在截圖角落）。 */
  .screen {
    position: relative;
  }

  /* 截圖的視窗：固定長寬比（截圖比例），載入前就佔好高度。 */
  .viewport {
    position: relative;
    width: 100%;
    overflow: hidden;
    background: var(--c-bg);
  }

  .cam {
    position: absolute;
    inset: 0;
    transform-origin: 0 0;
    transition: transform 700ms var(--ease-out);
  }

  .shot {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 100%;
  }

  .shot.is-fading {
    animation: quick-fade 260ms ease both;
  }

  @keyframes quick-fade {
    from {
      opacity: 0;
    }
  }

  /* 要點的框：琥珀色描邊（換手琥珀＝App 的強調色），鏡頭停好之後才淡入。 */
  .mark {
    position: absolute;
    z-index: 2;
    box-sizing: border-box;
    border: 3px solid var(--c-accent);
    border-radius: var(--radius-item);
    pointer-events: none;
    animation: quick-mark 200ms ease-out both;
  }

  .mark.is-delayed {
    animation-delay: 700ms;
  }

  @keyframes quick-mark {
    from {
      opacity: 0;
    }
  }

  /* 鍵帽列：截圖下方（窄的時候在吉祥物泡泡那一列下方）、控制列上方的一列，置中、不換行（只是提示，不能按）。 */
  .keys {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    justify-content: center;
    gap: var(--space-5);
    min-height: 32px;
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--c-border);
    background: var(--c-panel);
    pointer-events: none;
  }

  .anchor {
    position: absolute;
    width: 1px;
    height: 1px;
    pointer-events: none;
  }

  .quick-caption {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    margin: var(--space-3) auto 0;
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
    text-align: center;
  }

  @media (prefers-reduced-motion: reduce) {
    .cam {
      transition: none;
    }

    .shot.is-fading,
    .mark {
      animation: none;
    }
  }
</style>
