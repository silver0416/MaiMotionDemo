<script lang="ts">
  import Icon from './Icon.svelte';
  import DiagnosticList from './DiagnosticList.svelte';
  import CopyDebugButton from './CopyDebugButton.svelte';
  import {
    SCORING_LABEL,
    SCORING_V3,
    STATUS_LABEL,
    V3_BREAKDOWN_KEYS,
    V3_SCORE_GROUPS,
  } from '../lib/contract';
  import { formatClock, formatNumber } from '../lib/format';
  import { PALM_APPROX_HINT, coveredTargets, palmSeconds } from '../lib/palm';
  import { playback } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import type { AnalyzeStatus, Hand, PalmPlacement, Solution, V3Solution } from '../lib/types';

  const solutions = $derived<Solution[]>(session.solutions);
  const solution = $derived<Solution | null>(session.solution);
  const status = $derived<AnalyzeStatus | null>((session.response?.status as AnalyzeStatus) ?? null);
  const diagnostics = $derived(session.response?.diagnostics ?? []);

  interface SolutionStats {
    /** 接觸音符：Tap、Hold、Touch、Touch Hold 與有起點的 Slide 起點（part = contact / head） */
    contact: Record<Hand, number>;
    /** 其中由一次手掌動作一起覆蓋的 Touch 顆數 */
    palmNotes: Record<Hand, number>;
    /** 手掌動作次數；一次動作可能覆蓋多顆 Touch */
    palmMoves: Record<Hand, number>;
    palmMoveTotal: number;
    palmNoteTotal: number;
    /** Slide 軌道：實際跟著軌道移動的部分（part = slide）；中途換手時兩手各算一次 */
    slide: Record<Hand, number>;
    slideNotes: number;
    handovers: number;
  }

  /**
   * 分開統計「接觸音符」「一掌覆蓋」與「Slide 軌道」。
   * 只算 head/contact 會讓「右手全程在滑軌道」被寫成「右手 0 顆」，容易誤導；
   * 手掌覆蓋的顆數另外標出，避免把一隻手掌蓋住的多顆 Touch 讀成多隻手分別觸碰。
   * 覆蓋名單一律取自 Rust 的 palmPlacements，前端不重算覆蓋組。
   */
  function statsOf(item: Solution): SolutionStats {
    const stats: SolutionStats = {
      contact: { L: 0, R: 0 },
      palmNotes: { L: 0, R: 0 },
      palmMoves: { L: 0, R: 0 },
      palmMoveTotal: 0,
      palmNoteTotal: 0,
      slide: { L: 0, R: 0 },
      slideNotes: 0,
      handovers: item.handovers.length,
    };
    const seen = new Set<string>();
    const slideNotes = new Set<string>();
    for (const assignment of item.assignments) {
      // 連帶判定不是手的接觸，不列入左右手統計。
      if (assignment.part === 'group') continue;
      const isSlide = assignment.part === 'slide';
      const key = `${isSlide ? 'slide' : 'contact'}:${assignment.noteId}:${assignment.hand}`;
      if (isSlide) slideNotes.add(assignment.noteId);
      if (seen.has(key)) continue;
      seen.add(key);
      if (isSlide) stats.slide[assignment.hand] += 1;
      else stats.contact[assignment.hand] += 1;
    }
    stats.slideNotes = slideNotes.size;
    const palmed = new Set<string>();
    for (const placement of item.palmPlacements) {
      stats.palmMoves[placement.hand] += 1;
      stats.palmMoveTotal += 1;
      for (const noteId of placement.coveredNoteIds) {
        const key = `${placement.hand}:${noteId}`;
        if (palmed.has(key)) continue;
        palmed.add(key);
        stats.palmNotes[placement.hand] += 1;
        stats.palmNoteTotal += 1;
      }
    }
    return stats;
  }

  /** 摘要用的一行文字，讓候選之間可以直接比較。 */
  function summaryOf(item: SolutionStats): string {
    const parts = [`接觸 L${item.contact.L}／R${item.contact.R}`];
    if (item.palmMoveTotal > 0) {
      parts.push(`手掌 ${item.palmMoveTotal} 次覆蓋 ${item.palmNoteTotal} 顆`);
    }
    if (item.slideNotes > 0) parts.push(`軌道 L${item.slide.L}／R${item.slide.R}`);
    parts.push(`換手 ${item.handovers}`);
    return parts.join('・');
  }

  /** 手掌明細的定位：暫停後跳到覆蓋開始，並選取第一顆被覆蓋的音符。 */
  function seekPalm(placement: PalmPlacement): void {
    playback.pause();
    playback.seek(placement.startSeconds);
    const first = placement.coveredNoteIds[0];
    if (first) session.selectNote(first);
  }

  const palmRadiusText = $derived.by(() => {
    if (!solution) return '—';
    const radius = solution.configSnapshot.palmRadius;
    if (!(radius > 0)) return '0（關閉手掌覆蓋）';
    return `${formatNumber(radius, 2)}${radius === 0.5 ? '（四分之一盤面的圓形近似）' : ''}`;
  });

  /** V3 候選清單的一行分數摘要：動作效率 X・姿態 Y・短期負荷 Z。 */
  function v3ScoreLine(item: V3Solution): string {
    return V3_SCORE_GROUPS.map((group) => `${group.short} ${formatNumber(item.score[group.id], 2)}`).join('・');
  }

  /** V3 十項同單位直接加總，長條用十項中的最大值當共同基準，各群之間可以直接比長短。 */
  function v3Peak(item: V3Solution): number {
    return maxOf(V3_BREAKDOWN_KEYS.map((key) => item.scoreBreakdown[key]));
  }

  function maxOf(values: number[]): number {
    return Math.max(...values, 1e-6);
  }
</script>

<div class="stack">
  {#if session.lastFailure && session.phase !== 'analyzing'}
    {@const failure = session.lastFailure}
    <section class="section">
      <div class="alert alert--error">
        <div class="alert-title">分析未完成</div>
        <p class="small">{failure.message}</p>
        {#if session.result}
          <p class="small">盤面仍是上一次成功的結果。</p>
        {/if}
      </div>
      <div class="row row-wrap" style="margin-top: var(--space-3)">
        <CopyDebugButton
          input={() => ({
            context: '分析請求失敗',
            source: failure.request.source,
            config: failure.request.solverConfig,
            firstSeconds: failure.request.firstSeconds,
            requestId: failure.request.requestId,
            error: failure.message,
          })}
        />
      </div>
    </section>
  {/if}

  {#if !session.result}
    <section class="section">
      <p class="small muted">尚未產生結果。</p>
    </section>
  {:else if solutions.length === 0}
    <section class="section">
      <div class="section-title"><span>沒有可用方案</span></div>
      <div class="alert alert--warn">
        <div class="alert-title">{status ? (STATUS_LABEL[status] ?? status) : '無方案'}</div>
        <p>只顯示譜面層，原因見下方診斷。</p>
      </div>
      {#if diagnostics.length > 0}
        <div style="margin-top: var(--space-3)">
          <DiagnosticList {diagnostics} linked />
        </div>
      {/if}
      <p class="small muted" style="margin-top: var(--space-3)">
        可試著調大搜尋寬度、切換交接或縮短片段。找不到方案不等於人類打不出來。
      </p>
      <div class="row row-wrap" style="margin-top: var(--space-3)">
        <CopyDebugButton
          input={() => ({
            context: '方案分頁（沒有可用方案）',
            source: session.result?.source ?? '',
            config: session.result?.config ?? null,
            firstSeconds: session.result?.firstSeconds ?? null,
            response: session.result?.response ?? null,
          })}
        />
      </div>
    </section>
  {:else}
    <section class="section">
      <div class="section-title">
        <span>候選方案</span>
        <span class="muted xsmall">
          依核心排序，越前面越符合目前偏好
        </span>
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
              <span class="xsmall muted">第 {index + 1} 名</span>
            </span>
            <span class="xsmall mono">{v3ScoreLine(item)}</span>
            <span class="xsmall muted">{summaryOf(stats)}</span>
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
              <th style="width: 30%">分工</th>
              <th>接觸音符</th>
              {#if stats.palmMoveTotal > 0}
                <th>一掌覆蓋</th>
              {/if}
              <th>Slide 軌道</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td><span class="badge badge--left">L</span> 左手</td>
              <td class="mono">{stats.contact.L} 顆</td>
              {#if stats.palmMoveTotal > 0}
                <td class="mono">
                  {stats.palmMoves.L > 0 ? `${stats.palmNotes.L} 顆／${stats.palmMoves.L} 次` : '—'}
                </td>
              {/if}
              <td class="mono">{stats.slideNotes > 0 ? `${stats.slide.L} 條` : '—'}</td>
            </tr>
            <tr>
              <td><span class="badge badge--right">R</span> 右手</td>
              <td class="mono">{stats.contact.R} 顆</td>
              {#if stats.palmMoveTotal > 0}
                <td class="mono">
                  {stats.palmMoves.R > 0 ? `${stats.palmNotes.R} 顆／${stats.palmMoves.R} 次` : '—'}
                </td>
              {/if}
              <td class="mono">{stats.slideNotes > 0 ? `${stats.slide.R} 條` : '—'}</td>
            </tr>
          </tbody>
        </table>
        <p class="small" style="margin-top: var(--space-3)">
          換手 {stats.handovers} 次{stats.palmMoveTotal > 0
            ? `・手掌覆蓋 ${stats.palmMoveTotal} 次`
            : ''}
        </p>
        <p class="field-hint" style="margin-top: var(--space-2)">
          「接觸音符」計 Tap、Hold、Touch、Touch Hold 與 Slide 起點（無起點的 ? ! 不算）；
          同一顆音符中途換手時兩手各算一次，各欄不能相加。
          {#if stats.palmMoveTotal > 0}
            「一掌覆蓋」是同一隻手掌一次蓋住的 Touch 顆數與動作次數，這幾顆已計入該手的接觸音符，
            不是多隻手分別觸碰。
          {/if}
          評分與覆蓋都是本 Demo 的啟發式規則，不是官方判定或人體模型。
        </p>
      </section>

      {#if solution}
        {@const v3 = solution}
        {@const peak = v3Peak(v3)}
        <section class="section">
          <div class="section-title">
            <span>評分</span>
            <span class="muted xsmall mono">總分 {formatNumber(v3.score.total)}</span>
          </div>
          <div class="score-groups">
            {#each V3_SCORE_GROUPS as group (group.id)}
              <div class="score-group">
                <div class="score-head">
                  <span class="score-label">{group.label}</span>
                  <span class="mono score-value">{formatNumber(v3.score[group.id])}</span>
                </div>
                <p class="field-hint">{group.hint}</p>
                <details class="score-details">
                  <summary>分項</summary>
                  <table class="table">
                    <tbody>
                      {#each group.items as entry (entry.key)}
                        {@const value = v3.scoreBreakdown[entry.key]}
                        <tr>
                          <td style="width: 42%">
                            <div>{entry.label}</div>
                            <div class="xsmall muted">{entry.hint}</div>
                          </td>
                          <td class="mono" style="width: 22%">{formatNumber(value)}</td>
                          <td>
                            <div class="bar-track">
                              <div class="bar-fill" style={`width:${Math.max(0, value / peak) * 100}%`}></div>
                            </div>
                          </td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </details>
              </div>
            {/each}
          </div>
          <details class="score-details" style="margin-top: var(--space-3)">
            <summary>排序方式</summary>
            <p class="field-hint">
              核心把三群直接相加成總分，總分越低排越前面；沒有額外倍率。
              分項長條以這個候選十項中的最大值為基準，可以跨群比較。
            </p>
          </details>
          <p class="field-hint" style="margin-top: var(--space-3)">
            分數是同一份譜面裡比較候選用的相對值，不是機率、百分比或玩家能力評估；
            公式與預設值是本 Demo 的起點，尚未經玩家校準。
          </p>
          <div class="row row-wrap" style="margin-top: var(--space-3)">
            <CopyDebugButton
              label="複製評分明細"
              input={() => ({
                context: '方案分頁（人類動作 v3 評分）',
                source: session.result?.source ?? '',
                config: session.result?.config ?? null,
                firstSeconds: session.result?.firstSeconds ?? null,
                response: session.result?.response ?? null,
              })}
            />
          </div>
        </section>
      {/if}

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
                      }}><Icon name="crosshair" size={12} />定位</button
                    >
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>

      {#if solution.palmPlacements.length > 0 || session.hasTouchNotes}
        <section class="section">
          <div class="section-title">
            <span>手掌覆蓋</span>
            {#if solution.palmPlacements.length > 0}
              <span class="muted xsmall">{solution.palmPlacements.length} 次手掌動作</span>
            {/if}
          </div>
          {#if solution.palmPlacements.length === 0}
            <p class="small muted">
              這個候選沒有一掌覆蓋多個 Touch，每顆 Touch 都各自接觸。{solution.configSnapshot
                .palmRadius > 0
                ? ''
                : '這次分析的手掌半徑為 0，已關閉覆蓋。'}
            </p>
          {:else}
            <table class="table">
              <thead>
                <tr>
                  <th style="width: 14%">手</th>
                  <th>覆蓋落點</th>
                  <th>區間</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {#each solution.palmPlacements as placement, index (index)}
                  <tr>
                    <td>
                      <span
                        class="badge"
                        class:badge--left={placement.hand === 'L'}
                        class:badge--right={placement.hand === 'R'}>{placement.hand}</span
                      >
                    </td>
                    <td>
                      <div class="mono">{coveredTargets(placement, session.noteById).join('・')}</div>
                      <div class="xsmall muted mono">
                        {placement.coveredNoteIds.length} 顆・{placement.coveredNoteIds.join('、')}
                      </div>
                    </td>
                    <td class="mono xsmall">
                      <div>
                        {formatClock(placement.startSeconds)} – {formatClock(placement.endSeconds)}
                      </div>
                      <div class="muted">{formatNumber(palmSeconds(placement), 2)} 秒</div>
                    </td>
                    <td>
                      <button class="linkish xsmall" onclick={() => seekPalm(placement)}><Icon name="crosshair" size={12} />定位</button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
            <p class="field-hint" style="margin-top: var(--space-3)">
              一次手掌動作只覆蓋同一判定時間的 Touch／Touch Hold，佔用到其中最晚放開的時刻，
              期間該手不接別處；Tap 與 Slide 不會被算進同一掌。
              覆蓋名單、掌心與時間都由核心判定。{PALM_APPROX_HINT}
            </p>
          {/if}
        </section>
      {/if}

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
            <dt>評分方式</dt>
            <dd>
              {SCORING_LABEL} <span class="muted mono">{SCORING_V3}</span>
            </dd>
              <dt>左右分工傾向</dt>
              <dd>{solution.configSnapshot.homePreference}</dd>
              <dt>快速移動容忍</dt>
              <dd>{solution.configSnapshot.travelComfort} 半徑/秒</dd>
              <dt>同點連打容忍</dt>
              <dd>{solution.configSnapshot.jackTolerance}</dd>
              <dt>Slide 換手意願</dt>
              <dd>{solution.configSnapshot.handoverWillingness}</dd>
              <dt>Slide 路線</dt>
              <dd>{solution.configSnapshot.slideShortcut ? '依判定區抄近' : '沿星星畫完'}</dd>
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
            <dt>最晚接上</dt>
            <dd>{solution.configSnapshot.slidePickupSeconds}</dd>
            <dt>滑移距離</dt>
            <dd>{solution.configSnapshot.glideDistance}</dd>
            <dt>連打間隔</dt>
            <dd>{solution.configSnapshot.repetitionSeconds}</dd>
            <dt>手掌半徑</dt>
            <dd>{palmRadiusText}</dd>
          </dl>
          <p class="field-hint" style="margin-top: var(--space-3)">
            手掌半徑 0 表示這次分析沒有啟用手掌覆蓋，每顆 Touch 都要各自接觸。{PALM_APPROX_HINT}
          </p>
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

  .score-groups {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .score-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .score-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .score-label {
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .score-value {
    font-size: var(--fs-md);
  }

  .score-details > summary {
    font-size: var(--fs-xs);
  }

  .warnings {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    color: var(--c-text-dim);
  }
</style>
