<script lang="ts">
  import { COST_HINT, COST_KEYS, COST_LABEL, STATUS_LABEL, WEIGHT_OF_COST, costSum } from '../lib/contract';
  import { formatClock, formatDelta, formatNumber } from '../lib/format';
  import { playback } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import type { AnalyzeStatus, Hand, Solution } from '../lib/types';

  const solutions = $derived<Solution[]>(session.solutions);
  const solution = $derived<Solution | null>(session.solution);
  const status = $derived<AnalyzeStatus | null>((session.response?.status as AnalyzeStatus) ?? null);
  const best = $derived<number>(solutions.length > 0 ? solutions[0].totalCost : 0);

  interface SolutionStats {
    /** 起始觸碰：Tap、Hold 與 Slide 起點（part = contact / head） */
    touch: Record<Hand, number>;
    /** Slide 軌道：實際跟著軌道移動的部分（part = slide）；中途換手時兩手各算一次 */
    slide: Record<Hand, number>;
    slideNotes: number;
    handovers: number;
  }

  /**
   * 分開統計「起始觸碰」與「Slide 軌道」。
   * 只算 head/contact 會讓「右手全程在滑軌道」被寫成「右手 0 顆」，容易誤導。
   */
  function statsOf(item: Solution): SolutionStats {
    const stats: SolutionStats = {
      touch: { L: 0, R: 0 },
      slide: { L: 0, R: 0 },
      slideNotes: 0,
      handovers: item.handovers.length,
    };
    const seen = new Set<string>();
    const slideNotes = new Set<string>();
    for (const assignment of item.assignments) {
      const isSlide = assignment.part === 'slide';
      const key = `${isSlide ? 'slide' : 'touch'}:${assignment.noteId}:${assignment.hand}`;
      if (isSlide) slideNotes.add(assignment.noteId);
      if (seen.has(key)) continue;
      seen.add(key);
      if (isSlide) stats.slide[assignment.hand] += 1;
      else stats.touch[assignment.hand] += 1;
    }
    stats.slideNotes = slideNotes.size;
    return stats;
  }

  const maxCost = $derived.by(() => {
    if (!solution) return 1;
    return Math.max(...COST_KEYS.map((key) => solution.costBreakdown[key]), 1e-6);
  });

  const sumCheck = $derived.by(() => {
    if (!solution) return null;
    const sum = costSum(solution);
    return { sum, diff: Math.abs(sum - solution.totalCost) };
  });
</script>

<div class="stack">
  {#if !session.result}
    <section class="section">
      <p class="small muted">尚未產生結果。</p>
    </section>
  {:else if solutions.length === 0}
    <section class="section">
      <div class="section-title"><span>沒有可用方案</span></div>
      <div class="alert alert--warn">
        <div class="alert-title">{status ? (STATUS_LABEL[status] ?? status) : '無方案'}</div>
        <p>只顯示譜面層，原因見「編輯」分頁的診斷。</p>
      </div>
      <p class="small muted" style="margin-top: var(--space-3)">
        可試著調大搜尋寬度、切換交接或縮短片段。找不到方案不等於人類打不出來。
      </p>
    </section>
  {:else}
    <section class="section">
      <div class="section-title">
        <span>候選方案</span>
        <span class="muted xsmall">成本越低越偏好</span>
      </div>
      <div class="candidates" role="radiogroup" aria-label="候選方案">
        {#each solutions as item, index (item.id)}
          {@const stats = statsOf(item)}
          <button
            class="candidate"
            class:is-selected={session.solutionIndex === index}
            role="radio"
            aria-checked={session.solutionIndex === index}
            onclick={() => session.selectSolution(index)}
          >
            <span class="candidate-head">
              <span class="mono">{item.id}</span>
              <span class="mono cost">{formatNumber(item.totalCost)}</span>
            </span>
            <span class="xsmall muted">
              {formatDelta(item.totalCost - best)}・觸碰 L{stats.touch.L}／R{stats.touch.R}{stats.slideNotes >
              0
                ? `・軌道 L${stats.slide.L}／R${stats.slide.R}`
                : ''}・換手 {stats.handovers}
            </span>
          </button>
        {/each}
      </div>
    </section>

    {#if solution}
      {@const stats = statsOf(solution)}
      <section class="section">
        <div class="section-title"><span>模型建議打法</span></div>
        <table class="table table--fixed">
          <thead>
            <tr>
              <th style="width: 32%">分工</th>
              <th>起始觸碰</th>
              <th>Slide 軌道</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td><span class="badge badge--left">L</span> 左手</td>
              <td class="mono">{stats.touch.L} 顆</td>
              <td class="mono">{stats.slideNotes > 0 ? `${stats.slide.L} 條` : '—'}</td>
            </tr>
            <tr>
              <td><span class="badge badge--right">R</span> 右手</td>
              <td class="mono">{stats.touch.R} 顆</td>
              <td class="mono">{stats.slideNotes > 0 ? `${stats.slide.R} 條` : '—'}</td>
            </tr>
          </tbody>
        </table>
        <p class="small" style="margin-top: var(--space-3)">
          換手 {stats.handovers} 次・總成本 {formatNumber(solution.totalCost)}
        </p>
        <p class="field-hint" style="margin-top: var(--space-2)">
          「起始觸碰」只算 Tap、Hold 與 Slide 起點；中途換手時兩手各算一次參與，兩欄不能相加。
          成本是本 Demo 的啟發式偏好，不是官方判定或人體模型。
        </p>
      </section>

      <section class="section">
        <div class="section-title">
          <span>成本拆解</span>
          <span class="muted xsmall mono">總計 {formatNumber(solution.totalCost)}</span>
        </div>
        <table class="table">
          <thead>
            <tr>
              <th style="width: 38%">項目</th>
              <th style="width: 22%">數值</th>
              <th>佔比</th>
            </tr>
          </thead>
          <tbody>
            {#each COST_KEYS as key (key)}
              {@const value = solution.costBreakdown[key]}
              <tr>
                <td>
                  <div>{COST_LABEL[key]}</div>
                  <div class="xsmall muted">
                    權重 {formatNumber(solution.configSnapshot[WEIGHT_OF_COST[key]], 2)}
                  </div>
                </td>
                <td class="mono">{formatNumber(value)}</td>
                <td>
                  <div class="bar-track" title={COST_HINT[key]}>
                    <div class="bar-fill" style={`width:${(value / maxCost) * 100}%`}></div>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if sumCheck && sumCheck.diff >= 1e-6}
          <p class="xsmall" style="margin-top: var(--space-2); color: var(--c-danger)">
            各項加總 {formatNumber(sumCheck.sum)} 與 totalCost 不符，請回報核心。
          </p>
        {/if}
      </section>

      <section class="section">
        <div class="section-title"><span>換手</span></div>
        {#if solution.handovers.length === 0}
          <p class="small muted">這個候選沒有換手。</p>
        {:else}
          <table class="table">
            <thead>
              <tr>
                <th>音符</th>
                <th>方向</th>
                <th>區間</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {#each solution.handovers as handover, index (index)}
                <tr>
                  <td class="mono">{handover.noteId}</td>
                  <td>
                    <span class="badge" class:badge--left={handover.from === 'L'} class:badge--right={handover.from === 'R'}>
                      {handover.from}
                    </span>
                    →
                    <span class="badge" class:badge--left={handover.to === 'L'} class:badge--right={handover.to === 'R'}>
                      {handover.to}
                    </span>
                  </td>
                  <td class="mono xsmall">
                    {handover.swap
                      ? `${formatClock(handover.startSeconds)} 碰頭互換`
                      : `${formatClock(handover.startSeconds)} – ${formatClock(handover.endSeconds)}`}
                  </td>
                  <td>
                    <button
                      class="linkish xsmall"
                      onclick={() => {
                        playback.pause();
                        playback.seek(handover.startSeconds);
                        session.selectNote(handover.noteId);
                      }}>定位</button
                    >
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>

      {#if solution.warnings.length > 0}
        <section class="section">
          <div class="section-title"><span>核心提醒</span></div>
          <ul class="small warnings">
            {#each solution.warnings as warning (warning)}
              <li>{warning}</li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class="section">
        <details>
          <summary>這份結果使用的參數</summary>
          <dl class="kv">
            <dt>搜尋寬度</dt>
            <dd>{solution.configSnapshot.beamWidth}</dd>
            <dt>候選數</dt>
            <dd>{solution.configSnapshot.topK}</dd>
            <dt>允許換手</dt>
            <dd>{solution.configSnapshot.allowHandover ? '是' : '否'}</dd>
            <dt>取樣間隔</dt>
            <dd>{solution.configSnapshot.checkpointSeconds}</dd>
            <dt>接觸時間</dt>
            <dd>{solution.configSnapshot.contactSeconds}</dd>
            <dt>交接重疊</dt>
            <dd>{solution.configSnapshot.handoverSeconds}</dd>
            <dt>交接冷卻</dt>
            <dd>{solution.configSnapshot.handoverCooldown}</dd>
            <dt>預備時間</dt>
            <dd>{solution.configSnapshot.preparationSeconds}</dd>
            <dt>速度基準</dt>
            <dd>{solution.configSnapshot.speedReference}</dd>
            <dt>連打間隔</dt>
            <dd>{solution.configSnapshot.repetitionSeconds}</dd>
          </dl>
        </details>
      </section>
    {/if}
  {/if}
</div>

<style>
  .candidates {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .candidate {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    text-align: left;
    background: var(--c-control);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .candidate:hover {
    background: var(--c-control-hover);
  }

  .candidate.is-selected {
    border-color: var(--c-text);
    background: var(--c-control-hover);
  }

  .candidate-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .cost {
    font-size: var(--fs-md);
  }

  .warnings {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    color: var(--c-text-dim);
  }
</style>
