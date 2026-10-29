<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import { isDesktop } from '../lib/api';
  import { formatClock } from '../lib/format';
  import { MODEL_TOTAL_BYTES } from '../lib/handcam/models';
  import type { ChartState, HandcamNoteData, VideoLink } from '../lib/video';
  import type { Handcam } from './handcam.svelte';

  interface Props {
    handcam: Handcam;
    chart: ChartState | null;
    link: VideoLink | null;
    src: string | null;
    offset: number | null;
    /** 影片目前時間 */
    current: number;
    paused: boolean;
    requestNotes: () => Promise<HandcamNoteData[]>;
    apply: () => Promise<void>;
    showReference: () => void;
  }

  let { handcam, chart, link, src, offset, current, paused, requestNotes, apply, showReference }: Props = $props();

  let fileInput: HTMLInputElement | null = $state(null);
  const desktop = isDesktop();
  const BUTTON_HINT = ['右上', '右', '右下', '下偏右', '下偏左', '左下', '左', '左上'];

  const mb = (bytes: number) => `${Math.round(bytes / 1024 / 1024)} MB`;
  const pct = (value: number) => `${Math.round(value * 100)}%`;
  const atReference = $derived(handcam.referenceTime !== null && Math.abs(current - handcam.referenceTime) < 0.02);

  const blockers = $derived.by(() => {
    const out: string[] = [];
    if (!chart?.ready) out.push('主視窗要先開啟譜面並完成分析。');
    if (!src) out.push('還沒有影片。');
    if (offset === null) out.push('影片還沒對齊（在「對齊與跟隨」按「設為第一顆」）。');
    if (!handcam.modelReady) out.push('還沒有模型檔。');
    if (handcam.pointCount < 4) out.push('還沒標盤面環點（至少 4 點）。');
    return out;
  });

  async function start(): Promise<void> {
    if (!src || offset === null) return;
    handcam.phase = 'loading-notes';
    const notes = await requestNotes();
    if (notes.length === 0) {
      handcam.error = '主視窗沒有可預填的音符（譜面還沒分析完成，或沒有定位鍵）。';
      handcam.phase = 'idle';
      return;
    }
    await handcam.run(src, notes, offset, !!link?.mirror);
  }

  const tiers = $derived.by(() => {
    const result = handcam.result;
    if (!result) return null;
    const count = { high: 0, mid: 0, low: 0, trackHigh: 0, trackMid: 0, trackLow: 0, heads: 0, slides: 0 };
    for (const guess of result.guesses) {
      if (guess.hand !== null) {
        count.heads += 1;
        count[guess.tier] += 1;
      }
      if (guess.trackTier) {
        count.slides += 1;
        if (guess.trackTier === 'high') count.trackHigh += 1;
        else if (guess.trackTier === 'mid') count.trackMid += 1;
        else count.trackLow += 1;
      }
    }
    return count;
  });

  const eta = $derived.by(() => {
    const p = handcam.progress;
    if (!p || p.phase !== 'frames' || p.rate <= 0) return null;
    return Math.max(0, (p.framesTotal - p.framesDone) / p.rate);
  });
</script>

<section class="section stack-sm" aria-labelledby="hc-intro">
  <div class="section-title" id="hc-intro">從手元影片預填左右手</div>
  <p class="small muted">
    偵測影片裡玩家的雙手，推出每顆音符是哪隻手打的，填成「預填」草稿；確認之後才算真人資料，已經標過的不會被改。
    看得到玩家肩膀和手臂的機位最準；拍不到肩膀時會改用手在盤面上的位置分左右，錯誤會多一些。
  </p>
  {#if handcam.gpu === false}
    <div class="alert alert--warn small">這台電腦的顯示卡無法使用 WebGPU，分析會非常慢。</div>
  {/if}
</section>

<section class="section stack-sm" aria-labelledby="hc-model">
  <div class="section-title" id="hc-model">1. 模型</div>
  {#if desktop}
    {#if handcam.modelUnavailable}
      <p class="small muted">這個版本還不能下載模型（程式核心的下載功能尚未完成）。</p>
    {:else if handcam.modelStatus?.installed}
      <div class="row row-wrap">
        <span class="small">已下載・{mb(handcam.modelStatus.bytes)}</span>
        <span class="spacer"></span>
        <button class="btn btn--ghost" onclick={() => void handcam.removeModel()} disabled={handcam.phase === 'running'}>
          <Icon name="trash" size={14} />刪除
        </button>
      </div>
    {:else if handcam.installing}
      <div class="bar-track" role="progressbar" aria-label="模型下載進度">
        <div
          class="bar-fill"
          style={`width:${handcam.modelProgress?.total ? Math.round((100 * handcam.modelProgress.downloaded) / handcam.modelProgress.total) : 0}%`}
        ></div>
      </div>
      <div class="row">
        <span class="xsmall muted mono">{handcam.modelProgress ? mb(handcam.modelProgress.downloaded) : '準備中'}</span>
        <span class="spacer"></span>
        <button class="btn btn--ghost" onclick={() => void handcam.cancelInstall()}>取消</button>
      </div>
    {:else}
      <p class="small muted">需要先下載姿態偵測模型（約 {mb(MODEL_TOTAL_BYTES)}），只下載一次，存在本機。</p>
      <button class="btn btn--primary" onclick={() => void handcam.installModel()}><Icon name="download" size={14} />下載模型</button>
    {/if}
  {:else}
    <p class="small muted">瀏覽器預覽：一次選取三個檔（兩個 .onnx 模型與 ORT 的 .wasm），只在這次開啟有效。</p>
    <div class="row row-wrap">
      <button class="btn" class:btn--primary={!handcam.picked} onclick={() => fileInput?.click()}>
        <Icon name="folder-open" size={14} />選擇模型檔
      </button>
      {#if handcam.picked}<span class="small">已載入</span>{/if}
    </div>
    {#if handcam.pickMessage}<p class="xsmall muted">{handcam.pickMessage}</p>{/if}
    <input class="sr-only" type="file" multiple accept=".onnx,.wasm" bind:this={fileInput} onchange={(e) => void handcam.pickFiles(e.currentTarget.files)} tabindex="-1" />
  {/if}
</section>

<section class="section stack-sm" aria-labelledby="hc-ring">
  <div class="section-title" id="hc-ring">
    2. 盤面校正
    <span class="xsmall mono" class:muted={handcam.pointCount < 4}>{handcam.pointCount}／8 點</span>
  </div>
  {#if handcam.phase === 'marking'}
    <p class="small">
      在影片上點 <strong>{handcam.cursor + 1} 號鍵</strong>的環點（通常在{BUTTON_HINT[handcam.cursor]}）。
    </p>
    <p class="xsmall muted">判定環上有 8 個白點，1 號在右上，順時針到 8 號。被手擋住就按「跳過」。</p>
    <div class="row row-wrap">
      <button class="btn" onclick={() => handcam.skip()}>跳過這點</button>
      <button class="btn" onclick={() => handcam.undo()} disabled={handcam.cursor === 0}>上一點</button>
      <span class="spacer"></span>
      <button class="btn btn--primary" onclick={() => handcam.finishMarking()} disabled={handcam.pointCount < 4}>完成</button>
    </div>
  {:else}
    <ol class="hc-steps small">
      <li>暫停在判定環上白點清楚、手沒擋住的一格（例如開始前的空白盤面）。</li>
      <li>按「開始標點」，依序點 1–8 號鍵的白點；至少 4 點。</li>
      <li>確認畫面上的黃色圓與鍵號對得上實際盤面。</li>
    </ol>
    <div class="row row-wrap">
      <button
        class="btn"
        class:btn--primary={handcam.pointCount < 4}
        onclick={() => handcam.startMarking(current)}
        disabled={!src || !paused || handcam.phase === 'running'}
        title={paused ? '' : '先暫停影片'}
      >
        <Icon name="crosshair" size={14} />{handcam.pointCount > 0 ? '重新標點' : '開始標點'}
      </button>
      {#if handcam.referenceTime !== null && handcam.pointCount > 0 && !atReference}
        <button class="btn btn--ghost" onclick={showReference}>回到標點那一格（{formatClock(handcam.referenceTime)}）</button>
      {/if}
    </div>
    {#if !paused}<p class="xsmall muted">要先暫停影片才能標點。</p>{/if}
  {/if}
</section>

<section class="section stack-sm" aria-labelledby="hc-run">
  <div class="section-title" id="hc-run">3. 分析</div>
  {#if handcam.phase === 'running' || handcam.phase === 'loading-notes'}
    {@const p = handcam.progress}
    <div class="bar-track" role="progressbar" aria-label="分析進度" aria-valuenow={Math.round((p?.fraction ?? 0) * 100)} aria-valuemin={0} aria-valuemax={100}>
      <div class="bar-fill" style={`width:${Math.round((p?.fraction ?? 0) * 100)}%`}></div>
    </div>
    <div class="row">
      <span class="xsmall muted mono">
        {#if !p || p.phase === 'loading'}載入模型中…
        {:else if p.phase === 'reference'}準備參考畫面…
        {:else if p.phase === 'finishing'}整理結果…
        {:else}{p.framesDone}／{p.framesTotal} 格・每秒 {p.rate.toFixed(1)} 格{eta !== null ? `・約剩 ${Math.ceil(eta / 60)} 分` : ''}{/if}
      </span>
      <span class="spacer"></span>
      <button class="btn btn--ghost" onclick={() => handcam.cancel()}>取消</button>
    </div>
    <p class="xsmall muted">分析時影片視窗要保持顯示（縮小或被完全遮住時瀏覽器會暫停讀取畫面）。</p>
  {:else}
    <label class="check">
      <input type="checkbox" bind:checked={handcam.refine} />
      <span>自動微調對齊<span class="field-hint">依手的位置把偏移修正到最吻合的值（建議開啟）</span></span>
    </label>
    {#if blockers.length > 0}
      <ul class="hc-steps xsmall muted">
        {#each blockers as text (text)}<li>{text}</li>{/each}
      </ul>
    {/if}
    <button class="btn btn--primary" onclick={() => void start()} disabled={blockers.length > 0}>
      <Icon name="play" size={14} />開始分析
    </button>
  {/if}
  {#if handcam.error}
    <div class="alert alert--error small" role="alert">{handcam.error}</div>
  {/if}
</section>

{#if handcam.result && tiers}
  {@const r = handcam.result}
  <section class="section stack-sm" aria-labelledby="hc-result">
    <div class="section-title" id="hc-result">4. 結果</div>
    {#if r.warnings.length > 0}
      <div class="alert alert--warn small" role="status">
        <div class="alert-title">這部影片的機位不太適合</div>
        <ul class="hc-steps">
          {#each r.warnings as text (text)}<li>{text}</li>{/each}
        </ul>
        <span class="xsmall">推估仍可參考，但錯誤會比平常多，請多抽查。</span>
      </div>
    {/if}
    {#if r.shiftAtEdge}
      <div class="alert alert--warn small">自動微調找到的修正量在搜尋範圍邊緣，原本的對齊可能差太多，請重新「設為第一顆」再分析。</div>
    {/if}
    <dl class="kv">
      <dt>高信心</dt>
      <dd>{tiers.high} 顆（照填）</dd>
      <dt>中信心</dt>
      <dd>{tiers.mid} 顆（填入並標不確定）</dd>
      <dt>判斷不了</dt>
      <dd>{tiers.low} 顆（不填）</dd>
      {#if tiers.slides > 0}
        <dt>Slide 滑行</dt>
        <dd>高 {tiers.trackHigh}・中 {tiers.trackMid}・不填 {tiers.trackLow}</dd>
      {/if}
      <dt>雙手同時入鏡</dt>
      <dd>{pct(r.stats.bothHands)}</dd>
      {#if r.fallback >= 0.01}
        <dt>直接在盤面找手</dt>
        <dd>{pct(r.fallback)} 的畫格（找不到完整的玩家時）</dd>
      {/if}
      <dt>盤面追蹤</dt>
      <dd>{pct(r.stats.tracked)}</dd>
      <dt>對齊偏移</dt>
      <dd>{r.offsetUsed.toFixed(3)} 秒{r.shift !== 0 ? `（修正 ${r.shift >= 0 ? '+' : ''}${r.shift.toFixed(3)}）` : ''}</dd>
      <dt>耗時</dt>
      <dd>{Math.round(r.seconds)} 秒・{r.frames} 格・{r.backend === 'webgpu' ? 'GPU' : 'CPU'}</dd>
    </dl>
    <p class="xsmall muted">
      研究中機位良好的影片：高信心這層約 98% 正確，高＋中合計約 95%。不同機位會有差異，套用後請照影片抽查幾顆。
    </p>
    <div class="row row-wrap">
      <button class="btn btn--primary" onclick={() => void apply()} disabled={handcam.applying || !chart?.ready}>
        <Icon name="check" size={14} />套用為預填
      </button>
      {#if handcam.applied !== null}
        <span class="small">已填入 {handcam.applied} 顆</span>
      {/if}
    </div>
    <p class="xsmall muted">只填還沒標的步驟；原本是預填的會改成影片的結果；真人標註不動。</p>
  </section>
{/if}

<style>
  .hc-steps {
    margin: 0;
    padding-left: var(--space-4);
    color: var(--c-text-dim);
  }

  .alert .hc-steps {
    color: inherit;
  }
</style>
