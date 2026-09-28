<script lang="ts">
  import Icon from './Icon.svelte';
  import LaneGraph, { laneColumnWidth } from './LaneGraph.svelte';
  import LineChips from './LineChips.svelte';
  import {
    assignLanes,
    branchDepths,
    laneRows,
    LINE_COLORS,
    lineAt,
    MAIN_LINE,
    MAIN_NAME,
    nextBranchName,
    type AnnotationDraft,
    type LaneRun,
  } from '../lib/annotation';
  import { formatClock } from '../lib/format';
  import { annotation } from '../state/annotation.svelte';
  import { playback } from '../state/playback.svelte';
  import { records } from '../state/records.svelte';
  import { toasts } from '../state/toasts.svelte';

  /**
   * 打法分支：像電車路線圖一樣，從某條線在某段分出其他打法，段落結束後併入某條線（可以和分出的線不同）。
   * 選一條線就是選一條路線，逐顆標註、進度與比對都照這條路線。範圍一律以「第幾顆」表示。
   */

  const branches = $derived(annotation.draft.branches);
  const activeId = $derived(annotation.draft.active);
  const active = $derived(annotation.branch(activeId) ?? null);
  const lanes = $derived(assignLanes(branches));
  const laneCount = $derived(Math.max(1, ...lanes.values()) + 1);
  const total = $derived(annotation.ordered.length);
  const allLines = $derived([MAIN_LINE, ...branches.map((branch) => branch.id)]);

  type Station = { key: string; time: number; kind: 'head' | 'fork' | 'merge' | 'tail'; line: string; depth: number };
  const KIND_ORDER = { head: 0, fork: 1, merge: 2, tail: 3 };

  /** 路線圖上的站：開頭、每條分支的分出與併回、結尾。 */
  const stations = $derived.by<Station[]>(() => {
    const first = annotation.ordered[0]?.timeSeconds ?? 0;
    const last = annotation.ordered.at(-1)?.timeSeconds ?? 0;
    const out: Station[] = [{ key: 'head', time: first, kind: 'head', line: MAIN_LINE, depth: 0 }];
    const depths = branchDepths(branches);
    for (const branch of branches) {
      const depth = depths.get(branch.id) ?? 1;
      out.push({ key: `${branch.id}:fork`, time: branch.from, kind: 'fork', line: branch.id, depth });
      out.push({ key: `${branch.id}:merge`, time: branch.to, kind: 'merge', line: branch.id, depth });
    }
    out.push({ key: 'tail', time: last, kind: 'tail', line: MAIN_LINE, depth: 0 });
    // 同一時間：先分出（外層先）再併回（內層先），分支圖才不會交叉打結。
    return out.sort(
      (a, b) =>
        a.time - b.time ||
        KIND_ORDER[a.kind] - KIND_ORDER[b.kind] ||
        (a.kind === 'merge' ? b.depth - a.depth : a.depth - b.depth),
    );
  });

  const rows = $derived.by(() => {
    const spans = new Map<string, LaneRun[]>();
    stations.forEach((station, index) => {
      if (station.kind === 'fork') spans.set(station.line, [[index, index]]);
      if (station.kind === 'merge') {
        const span = spans.get(station.line)?.[0];
        if (span) span[1] = index;
      }
    });
    const times = stations.map((station) => station.time);
    return laneRows(
      times,
      spans,
      branches,
      lanes,
      times.map((time) => lineAt(branches, activeId, time)),
    );
  });

  /** 每條分支的範圍文字、步數與和上一層不同的步數。 */
  const stats = $derived.by(() => {
    void annotation.draft.updatedAt;
    const map = new Map<string, { first: string; last: string; steps: number; diff: number }>();
    for (const branch of branches) {
      const steps = annotation.stepsIn(branch.from, branch.to);
      map.set(branch.id, {
        first: steps[0] ? annotation.stepRef(steps[0]) : '',
        last: steps.length > 0 ? annotation.stepRef(steps[steps.length - 1]) : '',
        steps: steps.length,
        diff: annotation.branchDiff(branch.id),
      });
    }
    return map;
  });

  function stationRef(station: Station): string {
    if (station.kind === 'head') return '1';
    if (station.kind === 'tail') return String(total);
    const info = stats.get(station.line);
    return (station.kind === 'fork' ? info?.first : info?.last) ?? '';
  }

  function stationText(station: Station): string {
    const name = annotation.lineName(station.line);
    const branch = annotation.branch(station.line);
    const parent = branch ? annotation.lineName(branch.parent) : MAIN_NAME;
    const into = branch ? annotation.lineName(branch.merge) : MAIN_NAME;
    switch (station.kind) {
      case 'head':
        return `${MAIN_NAME}・開頭`;
      case 'tail':
        return '結尾';
      case 'fork':
        return `從${parent}分出「${name}」`;
      default:
        return `「${name}」併入${into}`;
    }
  }

  function choose(station: Station) {
    annotation.setActive(station.line);
    if (station.kind === 'fork' || station.kind === 'merge') playback.seek(station.time);
  }

  /** 目前這一步的編號（Slide 滑行那一步為「↳編號」）；沒有選取時用播放時間之前最近的一步。 */
  function hereRef(): string {
    const step = annotation.currentStep ?? annotation.steps.filter((item) => item.time <= playback.time + 1e-6).at(-1);
    return step ? annotation.stepRef(step) : '1';
  }

  // ---- 新增分支（起點和「逐顆標註」清單的右鍵標記共用） ----
  let newEnd = $state('');
  let newName = $state('');

  const pendingRef = $derived(annotation.pendingStep ? annotation.stepRef(annotation.pendingStep) : null);
  const endStep = $derived(annotation.parseStepRef(newEnd, 'end'));
  /** 新分支併入目前路線在終點走的線。 */
  const intoLine = $derived(endStep ? annotation.lineOfStep(endStep.note, endStep.part) : MAIN_LINE);

  function setStart(value: string) {
    const step = annotation.parseStepRef(value, 'start');
    if (step) annotation.markStart(step);
    else annotation.cancelMark();
  }

  function useLoop() {
    if (!playback.loopEnabled) return;
    const inside = annotation.stepsIn(playback.loopStart, playback.loopEnd);
    if (inside.length === 0) return;
    annotation.markStart(inside[0]);
    newEnd = annotation.stepRef(inside[inside.length - 1]);
  }

  const createCount = $derived.by(() => {
    const start = annotation.pendingStep;
    if (!start || !endStep) return 0;
    return annotation.stepsIn(Math.min(start.time, endStep.time), Math.max(start.time, endStep.time)).length;
  });

  function create() {
    if (!endStep) return;
    const branch = annotation.finishMark(endStep, newName);
    if (!branch) return;
    newEnd = '';
    newName = '';
    toasts.show({
      id: 'branch-create',
      tone: 'ok',
      title: `已建立「${branch.name}」`,
      body: `${annotation.rangeText(branch.from, branch.to)}，從${annotation.lineName(branch.parent)}分出、併入${annotation.lineName(branch.merge)}，已複製目前的手順。現在的路線走這條分支。`,
    });
  }

  // ---- 目前分支 ----
  let editFrom = $state('');
  let editTo = $state('');

  const activeInfo = $derived(active ? (stats.get(active.id) ?? null) : null);

  // 切到別條分支或範圍被改動時，欄位跟著更新。
  $effect(() => {
    editFrom = activeInfo?.first ?? '';
    editTo = activeInfo?.last ?? '';
  });

  const editFromStep = $derived(annotation.parseStepRef(editFrom, 'start'));
  const editToStep = $derived(annotation.parseStepRef(editTo, 'end'));
  const rangeChanged = $derived(
    !!active &&
      !!editFromStep &&
      !!editToStep &&
      (Math.abs(editFromStep.time - active.from) > 1e-4 || Math.abs(editToStep.time - active.to) > 1e-4),
  );

  function undoable(id: string, title: string, before: AnnotationDraft | null, body = '按「復原」可以還原。') {
    if (!before) return;
    toasts.show({ id, tone: 'ok', title, body, action: { label: '復原', run: () => annotation.restore(before) } });
  }

  function applyRange() {
    const from = editFromStep;
    const to = editToStep;
    if (!active || !from || !to) return;
    const before = annotation.setBranchRange(active.id, from.time, to.time);
    if (!before) {
      toasts.show({ id: 'branch-range', tone: 'error', title: '範圍內沒有音符', body: '分支至少要包含一顆音符。' });
      return;
    }
    undoable('branch-range', '已調整範圍', before, '新涵蓋的步驟沿用上一層的手順，移出範圍的手順已刪除。');
  }

  function setLink(id: string, links: { parent?: string; merge?: string }) {
    if (annotation.setBranchLinks(id, links)) return;
    toasts.show({ id: 'branch-link', tone: 'error', title: '無法這樣接', body: '那條線在這一步不存在，或會造成繞圈。' });
  }

  function swap() {
    if (!active) return;
    const parent = annotation.lineName(active.parent);
    undoable(
      'branch-swap',
      `已和${parent}交換`,
      annotation.swapWithParent(active.id),
      `這段的打法現在是${parent}的，原本${parent}的打法留在「${active.name}」。`,
    );
  }

  function remove() {
    if (!active) return;
    const name = active.name;
    undoable('branch-remove', `已刪除「${name}」`, annotation.removeBranch(active.id));
  }

  const activeColor = $derived(annotation.lineColor(activeId));
</script>

<section class="section stack-sm">
  <p class="small muted">
    同一段有不同打法時，從某條線拉出一條分支；分支只管那一段，結束後併入某條線（可以和分出的線不同），也可以從分支再分出分支。
    點路線圖上的站或灰色的線就能切換路線，逐顆標註、進度與比對都照目前這條路線。
  </p>

  <div class="map" role="list" aria-label="打法路線圖">
    {#each stations as station, index (station.key)}
      {@const row = rows[index]}
      {@const info = stats.get(station.line)}
      <div role="listitem">
        <button
          class="station"
          class:is-active={station.line === activeId && station.kind !== 'head' && station.kind !== 'tail'}
          style={`padding-left:calc(var(--space-4) + ${laneColumnWidth(laneCount) + 8}px)`}
          onclick={() => choose(station)}
          title={`${formatClock(station.time)}　${station.kind === 'head' || station.kind === 'tail' ? '切到主線' : `切到「${annotation.lineName(station.line)}」並跳到這一站`}`}
        >
          {#if row}<LaneGraph {row} lanes={laneCount} />{/if}
          <span class="mono xsmall muted num">第 {stationRef(station)} 顆</span>
          <span class="small what">{stationText(station)}</span>
          {#if station.kind === 'fork' && info}
            <span class="xsmall muted meta">
              第 {info.first}–{info.last} 顆，共 {info.steps} 步・和上一層不同 {info.diff} 步
            </span>
          {/if}
        </button>
      </div>
    {/each}
  </div>
  {#if branches.length === 0}
    <p class="xsmall muted">目前只有主線。在下方選起點和終點建立第一條分支，或在「逐顆標註」清單上按右鍵標記。</p>
  {/if}
</section>

<section class="section stack-sm" aria-label="目前路線">
  <div class="section-title">目前路線</div>
  {#if branches.length > 0}
    <LineChips lines={allLines} current={activeId} label="切換路線" />
  {/if}

  {#if active}
    {@const info = stats.get(active.id)}
    <label class="field">
      <span class="field-label">名稱</span>
      <input class="input" value={active.name} onchange={(event) => annotation.renameBranch(active.id, event.currentTarget.value)} />
      <span class="field-hint">和{annotation.lineName(active.parent)}不同 {info?.diff ?? 0} 步</span>
    </label>
    <div class="links">
      <label class="field">
        <span class="field-label">從哪條線分出</span>
        <select
          class="select"
          value={active.parent}
          onchange={(event) => setLink(active.id, { parent: event.currentTarget.value })}
        >
          {#each annotation.linesAtTime(active.from, active.id) as line (line)}
            <option value={line}>{annotation.lineName(line)}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span class="field-label">併入哪條線</span>
        <select
          class="select"
          value={active.merge}
          onchange={(event) => setLink(active.id, { merge: event.currentTarget.value })}
        >
          {#each annotation.linesAtTime(active.to, active.id) as line (line)}
            <option value={line}>{annotation.lineName(line)}</option>
          {/each}
        </select>
      </label>
    </div>
    <div class="field">
      <span class="field-label">範圍</span>
      <div class="range-edit">
        <span class="small">第</span>
        <input class="input mono num-input" aria-label="分支從第幾顆開始" bind:value={editFrom} />
        <button class="btn" onclick={() => (editFrom = hereRef())} title="目前選取的那一步">取目前</button>
        <span class="small">到</span>
        <input class="input mono num-input" aria-label="分支到第幾顆結束" bind:value={editTo} />
        <button class="btn" onclick={() => (editTo = hereRef())} title="目前選取的那一步">取目前</button>
        <span class="small">顆</span>
      </div>
      <div class="row">
        <button class="linkish small" onclick={() => playback.seek(active.from)}>跳到第 {activeInfo?.first} 顆</button>
        <span class="spacer"></span>
        <button class="btn" disabled={!rangeChanged} onclick={applyRange}>套用範圍</button>
      </div>
    </div>
  {:else}
    <p class="small">{MAIN_NAME}{branches.length > 0 ? '（不走任何分支）' : ''}</p>
  {/if}

  <div class="field">
    <span class="field-label">線的顏色</span>
    <div class="swatches" role="group" aria-label="線的顏色">
      {#each LINE_COLORS as color, index (color)}
        <button
          class="swatch"
          style={`background:${color}`}
          aria-pressed={activeColor.toLowerCase() === color}
          aria-label={`顏色 ${index + 1}`}
          title={color}
          onclick={() => annotation.setLineColor(activeId, color)}
        ></button>
      {/each}
      <label class="custom" title="自訂顏色">
        <input type="color" value={activeColor} aria-label="自訂顏色" oninput={(event) => annotation.setLineColor(activeId, event.currentTarget.value)} />
        <span class="xsmall">自訂</span>
      </label>
    </div>
    <span class="field-hint">路線走到這條線時用這個顏色，沒走到的線一律是灰色。</span>
  </div>

  {#if active}
    <label class="field">
      <span class="field-label">這種打法的說明</span>
      <textarea
        class="input memo"
        rows="2"
        placeholder="例如：交叉打法、不交叉的替代打法…"
        value={active.memo}
        oninput={(event) => annotation.setBranchMemo(active.id, event.currentTarget.value)}
      ></textarea>
    </label>
    <div class="row row-wrap">
      <button class="btn" onclick={swap} title="這段的打法和分出的那條線互換，兩種都會保留">
        <Icon name="flip" size={14} />和{annotation.lineName(active.parent)}交換
      </button>
      <span class="spacer"></span>
      <button class="btn btn--ghost" onclick={remove}>
        <Icon name="trash" size={14} />刪除分支
      </button>
    </div>
  {/if}
</section>

<section class="section stack-sm" aria-label="新增分支">
  <div class="section-title">新增分支</div>
  <div class="range-edit">
    <span class="small">起點 第</span>
    <input
      class="input mono num-input"
      aria-label="新分支起點是第幾顆"
      value={pendingRef ?? ''}
      onchange={(event) => setStart(event.currentTarget.value)}
    />
    <button class="btn" onclick={() => setStart(hereRef())} title="目前選取的那一步">取目前</button>
  </div>
  <div class="range-edit">
    <span class="small">終點 第</span>
    <input class="input mono num-input" aria-label="新分支終點是第幾顆" bind:value={newEnd} />
    <button class="btn" onclick={() => (newEnd = hereRef())} title="目前選取的那一步">取目前</button>
  </div>
  <div class="row">
    <input class="input" aria-label="分支名稱" placeholder={nextBranchName(branches)} bind:value={newName} />
    <button class="btn" disabled={!playback.loopEnabled} onclick={useLoop} title="取循環播放的範圍">取循環範圍</button>
  </div>
  <p class="xsmall muted">
    {createCount > 0
      ? `共 ${createCount} 步，從${annotation.lineName(annotation.pendingFromLine)}分出、併入${annotation.lineName(intoLine)}，並複製這段現在的手順當起點。`
      : '在清單選一步後按「取目前」（「↳64」是第 64 顆 Slide 的滑行）；也可以在「逐顆標註」清單按右鍵標記起點與終點。從標起點時的路線分出、併入目前路線在終點走的線。'}
  </p>
  <div class="row">
    <button class="btn btn--primary" disabled={createCount === 0 || !records.activeId} onclick={create}>
      <Icon name="git-branch" size={14} />建立分支
    </button>
    {#if pendingRef !== null}
      <button class="btn btn--ghost" onclick={() => annotation.cancelMark()}>取消標記</button>
    {/if}
  </div>
</section>

<style>
  .map {
    display: flex;
    flex-direction: column;
    margin: 0 calc(-1 * var(--space-4));
  }

  .station {
    position: relative;
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr);
    grid-template-areas:
      'num what'
      'num meta';
    align-items: center;
    column-gap: var(--space-2);
    width: 100%;
    min-height: 36px;
    padding-top: var(--space-1);
    padding-right: var(--space-4);
    padding-bottom: var(--space-1);
    background: none;
    border: none;
    text-align: left;
    cursor: pointer;
  }

  .station:hover {
    background: var(--c-control);
  }

  .station.is-active {
    background: var(--c-control-hover);
  }

  .station > :global(.lanes) {
    left: var(--space-4);
  }

  .num {
    grid-area: num;
    white-space: nowrap;
  }

  .what {
    grid-area: what;
  }

  .meta {
    grid-area: meta;
  }

  .range-edit {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  .num-input {
    width: 64px;
  }

  .links {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
  }

  .swatch {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 2px solid var(--c-panel);
    border-radius: 50%;
    box-shadow: 0 0 0 1px var(--c-border-strong);
    cursor: pointer;
  }

  /* 外圈用 box-shadow，鍵盤焦點仍用全域的 outline。 */
  .swatch[aria-pressed='true'] {
    box-shadow: 0 0 0 2px var(--c-text-strong);
  }

  .custom {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    margin-left: var(--space-1);
    color: var(--c-text-dim);
    cursor: pointer;
  }

  .custom input {
    width: 24px;
    height: 22px;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
  }

  .memo {
    resize: vertical;
    min-height: 0;
    font-family: var(--font-sans);
  }
</style>
