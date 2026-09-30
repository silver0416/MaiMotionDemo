<script lang="ts">
  import { saveExport, showSaved } from '../state/exportPrefs.svelte';
  import { tick } from 'svelte';
  import BranchPanel from './BranchPanel.svelte';
  import Deferred from './Deferred.svelte';
  import ContextMenu, { type MenuItem } from './ContextMenu.svelte';
  import Icon from './Icon.svelte';
  import LaneGraph, { laneColumnWidth } from './LaneGraph.svelte';
  import LineChips from './LineChips.svelte';
  import VirtualList from './VirtualList.svelte';
  import ResizeHandle from './ResizeHandle.svelte';
  import {
    assignLanes,
    CONFIDENCE_LABEL,
    inBranch,
    fileName,
    isHumanLabeled,
    isWifi,
    laneRows,
    MAIN_LINE,
    modelMark,
    neededParts,
    parseFile,
    progress,
    spansByTime,
    stepConfidence,
  } from '../lib/annotation';
  import { copyText } from '../lib/debug';
  import { formatClock } from '../lib/format';
  import { noteLabel, stepLabel } from '../lib/notes';
  import { annotation, isStepDone, type ListFilter, type Step, type StepPart } from '../state/annotation.svelte';
  import { playback } from '../state/playback.svelte';
  import { records } from '../state/records.svelte';
  import { session } from '../state/session.svelte';
  import { toasts } from '../state/toasts.svelte';
  import { videoSync } from '../state/videoSync.svelte';
  import type { Confidence, Hand, Note, NoteAnnotation, TrackHand } from '../lib/types';

  type View = 'label' | 'branch' | 'memo' | 'share' | 'compare';
  const VIEWS: { id: View; label: string }[] = [
    { id: 'label', label: '逐顆標註' },
    { id: 'branch', label: '打法' },
    { id: 'memo', label: '備註' },
    { id: 'share', label: '分享' },
    { id: 'compare', label: '比對' },
  ];
  const FILTERS: { id: ListFilter; label: string }[] = [
    { id: 'all', label: '全部' },
    { id: 'todo', label: '還沒確認' },
    { id: 'prefilled', label: '預填待確認' },
    { id: 'diff', label: '與模型不同' },
    { id: 'memo', label: '有備註' },
    { id: 'unsure', label: '不確定／皆可' },
  ];
  const CONFIDENCES: Confidence[] = ['sure', 'unsure', 'either'];

  let view = $state<View>('label');

  // ---- 清單高度：固定高度貼在面板底部，上方內容多寡不會讓清單位置跳動。
  const LIST_KEY = 'maimotion.annotate-list-height.v1';
  const LIST_DEFAULT = 320;
  const LIST_MIN = 160;
  /** 上方編輯區至少留下的高度。 */
  const UPPER_MIN = 160;

  function loadListHeight(): number {
    try {
      const value = Number(localStorage.getItem(LIST_KEY));
      return Number.isFinite(value) && value > 0 ? value : LIST_DEFAULT;
    } catch {
      return LIST_DEFAULT;
    }
  }

  let listHeight = $state(loadListHeight());
  let panelHeight = $state(0);
  let headHeight = $state(0);
  const listMax = $derived(Math.max(LIST_MIN, panelHeight - headHeight - UPPER_MIN));
  const shownList = $derived(Math.round(Math.min(listMax, Math.max(LIST_MIN, listHeight))));

  function setListHeight(value: number): void {
    listHeight = value;
    try {
      localStorage.setItem(LIST_KEY, String(value));
    } catch {
      // 無法保存只影響下次開啟的高度。
    }
  }

  const note = $derived<Note | null>(session.selectedNote);
  const current = $derived<Step | null>(annotation.currentStep);
  /** Slide 的起點與滑行分兩步標；目前這一步在編輯區會標出來。 */
  const split = $derived(!!note && note.kind === 'slide' && note.hasHead);
  /** 信心按鈕分組：有起點的 Slide 起點與滑行各一組，其他音符一組。 */
  const confidenceParts = $derived<(StepPart | null)[]>(split ? ['hand', 'track'] : [null]);
  const mark = $derived<NoteAnnotation | undefined>(annotation.mark(note));
  const need = $derived(note ? neededParts(note) : { hand: false, track: false });
  const noteShape = $derived(note?.pathId ? session.pathById.get(note.pathId)?.shape : undefined);
  const wifi = $derived(note ? isWifi(note, noteShape) : false);

  /** 模型的手，一律取候選方案（不是盤面上的真人打法），用來比對。 */
  const modelSolution = $derived(session.solutions[session.solutionIndex] ?? session.solutions[0] ?? null);
  const modelMarks = $derived.by(() => {
    const map = new Map<string, Omit<NoteAnnotation, 'key'>>();
    if (!modelSolution) return map;
    for (const item of annotation.ordered) map.set(item.id, modelMark(modelSolution, item));
    return map;
  });

  const stats = $derived(progress(annotation.routed, annotation.ordered));
  const percent = $derived(stats.total > 0 ? Math.round((100 * stats.done) / stats.total) : 0);

  /** 這一步和模型不同；human 是這一步自己的標註（annotation.stepMark）。 */
  function stepDiffers(step: Step, human: NoteAnnotation | undefined): boolean {
    if (!isHumanLabeled(human) || !human) return false;
    const model = modelMarks.get(step.note.id);
    if (!model) return false;
    if (step.part === 'hand') return !!human.hand && !!model.hand && human.hand !== model.hand;
    return !!human.track && !!model.track && human.track !== model.track;
  }

  /** 清單一列一步；信心每一步各自有，備註是整顆的，只列在它的第一步。 */
  const rows = $derived.by(() => {
    const filter = annotation.filter;
    const seen = new Set<string>();
    return annotation.steps.filter((step) => {
      // 分支以步驟為單位，起點與滑行可能分屬不同的線：確認狀態看這一步自己的標註。
      const own = annotation.stepMark(step);
      const human = annotation.mark(step.note);
      const first = !seen.has(step.note.id);
      seen.add(step.note.id);
      switch (filter) {
        case 'todo':
          return !isStepDone(step, own);
        case 'prefilled':
          return own?.prefilled === true && stepValue(step, own) !== undefined;
        case 'diff':
          return stepDiffers(step, own);
        case 'memo':
          return first && !!human?.memo;
        case 'unsure': {
          const confidence = stepConfidence(step.note, human, step.part);
          return confidence === 'unsure' || confidence === 'either';
        }
        default:
          return true;
      }
    });
  });

  // ---- 打法分支 ----
  const branches = $derived(annotation.draft.branches);
  const hasBranches = $derived(branches.length > 0);
  const lanes = $derived(assignLanes(branches));
  const laneCount = $derived(Math.max(1, ...lanes.values()) + 1);
  /** 清單每一列的分支圖；範圍以目前看得到的列計算。 */
  const listLanes = $derived(
    hasBranches
      ? laneRows(
          rows.map((step) => step.time),
          spansByTime(
            rows.map((step) => step.time),
            branches,
          ),
          branches,
          lanes,
          rows.map((step) => annotation.lineOfStep(step.note, step.part)),
        )
      : [],
  );
  /** 目前這一步歸哪條線、這一步有哪些打法可選（分支以步驟為單位）。 */
  const stepLine = $derived(current ? annotation.lineOfStep(current.note, current.part) : MAIN_LINE);
  const stepLines = $derived(current ? annotation.linesAt(current) : []);
  /** 起點與滑行分屬不同的線時，說明各自在哪條線。 */
  const splitText = $derived.by(() => {
    if (!note || !split) return null;
    const hand = annotation.lineOfStep(note, 'hand');
    const track = annotation.lineOfStep(note, 'track');
    return hand === track ? null : `起點在${annotation.lineName(hand)}，滑行在${annotation.lineName(track)}`;
  });

  // ---- 右鍵標記新分支：先標起點，再點終點（以步驟為單位，Slide 的起點與滑行可以分開） ----
  const pending = $derived(annotation.pendingStep);
  /** 標記中選到的另一步，當作終點候選。 */
  const endCandidate = $derived(pending && current && current.key !== pending.key ? current : null);

  /** 起點到終點（或只有起點）這段的時間範圍，用來在清單上標出範圍。 */
  function pendingSpan(end: Step | null): { from: number; to: number } {
    if (!pending) return { from: 0, to: -1 };
    const stop = end ? end.time : pending.time;
    return { from: Math.min(pending.time, stop), to: Math.max(pending.time, stop) };
  }

  const pendingRange = $derived(pendingSpan(endCandidate));

  /** 終點候選預設併入目前路線在那一步走的線。 */
  const endInto = $derived(endCandidate ? annotation.lineOfStep(endCandidate.note, endCandidate.part) : MAIN_LINE);

  function finishBranch(end: Step) {
    const branch = annotation.finishMark(end);
    if (!branch) return;
    toasts.show({
      id: 'branch-create',
      tone: 'ok',
      title: `已建立「${branch.name}」`,
      body: `${annotation.rangeText(branch.from, branch.to)}，從${annotation.lineName(branch.parent)}分出、併入${annotation.lineName(branch.merge)}，已複製目前的手順；現在的路線走這條分支。`,
    });
  }

  function deleteBranch(id: string) {
    const name = annotation.lineName(id);
    const before = annotation.removeBranch(id);
    if (!before) return;
    toasts.show({
      id: 'branch-remove',
      tone: 'ok',
      title: `已刪除「${name}」`,
      body: '按「復原」可以還原。',
      action: { label: '復原', run: () => annotation.restore(before) },
    });
  }

  let menu = $state<{ stepKey: string; x: number; y: number } | null>(null);
  const menuStep = $derived(menu ? (annotation.steps.find((step) => step.key === menu?.stepKey) ?? null) : null);

  const menuItems = $derived.by<MenuItem[]>(() => {
    const target = menuStep;
    if (!target) return [];
    // 分出與併入的線都看目前選的路線，選單只留標記本身。
    const items: MenuItem[] = [];
    if (pending) {
      const span = pendingSpan(target);
      items.push({ id: 'end', label: `標記分支終點（${annotation.rangeText(span.from, span.to)}）`, icon: 'flag' });
      if (target.key !== pending.key) items.push({ id: 'start', label: '改從這一步開始', icon: 'git-branch' });
      items.push({ id: 'cancel', label: '取消標記分支', icon: 'x' });
    } else {
      items.push({ id: 'start', label: '標記為新分支起點', icon: 'git-branch' });
    }
    // 刪除這一步所在的分支（主線不能刪）。
    const owner = annotation.lineOfStep(target.note, target.part);
    if (owner !== MAIN_LINE) {
      items.push({ id: `delete:${owner}`, label: `刪除分支「${annotation.lineName(owner)}」`, icon: 'trash', danger: true });
    }
    return items;
  });

  function openMenu(step: Step, event: MouseEvent) {
    event.preventDefault();
    menu = { stepKey: step.key, x: event.clientX, y: event.clientY };
  }

  /** 列上按選單鍵或 Shift+F10，和右鍵一樣打開選單。 */
  function onRowKeydown(step: Step, event: KeyboardEvent) {
    if (event.key !== 'ContextMenu' && !(event.key === 'F10' && event.shiftKey)) return;
    event.preventDefault();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { stepKey: step.key, x: rect.left + 24, y: rect.bottom };
  }

  function selectMenu(id: string) {
    const target = menuStep;
    menu = null;
    if (!target) return;
    const [action, line] = id.split(':');
    if (action === 'start') annotation.markStart(target);
    else if (action === 'end') finishBranch(target);
    else if (action === 'cancel') annotation.cancelMark();
    else if (action === 'delete') deleteBranch(line);
  }

  /** 播放時間所在的步驟（最後一步時間不晚於目前時間的）。 */
  const currentKey = $derived.by(() => {
    const list = annotation.steps;
    let low = 0;
    let high = list.length - 1;
    let found = -1;
    const time = playback.time + 1e-6;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (list[mid].time <= time) {
        found = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return found >= 0 ? list[found].key : null;
  });

  // ---- 清單只畫看得到的列：整首上千步一次全畫會讓切到標註分頁卡一下。
  // 列高固定兩種（有沒有旗標列），先算好每列的位置，捲動時只換可視範圍內的列。
  const ROW_HEIGHT = 28;
  const ROW_FLAGS_HEIGHT = 49;

  interface RowView {
    step: Step;
    index: number;
    human: NoteAnnotation | undefined;
    own: NoteAnnotation | undefined;
    text: string;
    first: boolean;
    number: number;
    differs: boolean;
    /** 不是「確定」時的信心。 */
    doubt: Confidence | null;
    memo: string | null;
    prefilled: boolean;
    pendingStart: boolean;
  }

  const rowViews = $derived.by<RowView[]>(() =>
    rows.map((step, index) => {
      const human = annotation.mark(step.note);
      const own = annotation.stepMark(step);
      const text = stepSummary(step, human);
      const first = step.part === 'hand' || !step.note.hasHead;
      const confidence = human ? stepConfidence(step.note, human, step.part) : 'sure';
      return {
        step,
        index,
        human,
        own,
        text,
        first,
        number: annotation.noteNumber(step.note),
        differs: stepDiffers(step, own),
        doubt: confidence === 'sure' ? null : confidence,
        memo: first && human?.memo ? human.memo : null,
        prefilled: !!own?.prefilled && !!text,
        pendingStart: pending?.key === step.key,
      };
    }),
  );

  const hasFlags = (row: RowView) =>
    row.differs || row.doubt !== null || row.memo !== null || row.prefilled || row.pendingStart;

  const rowHeight = (row: RowView) => (hasFlags(row) ? ROW_FLAGS_HEIGHT : ROW_HEIGHT);

  /** 目前這一步在清單的位置；盤面點選、快捷鍵前進時捲進可視範圍。 */
  const focusIndex = $derived.by(() => {
    const key = current?.key;
    if (!key) return null;
    const index = rows.findIndex((step) => step.key === key);
    return index >= 0 ? index : null;
  });

  function describe(item: Note): string {
    return noteLabel(item, session.pathById);
  }

  function handText(value: TrackHand | undefined): string {
    if (!value) return '—';
    return value === 'LR' ? 'L+R' : value;
  }

  function stepValue(step: Step, value: Omit<NoteAnnotation, 'key'> | undefined): TrackHand | undefined {
    return step.part === 'hand' ? value?.hand : value?.track;
  }

  /** 清單上這一步的手：接觸只有一隻手；滑行另加換手。 */
  function stepSummary(step: Step, value: Omit<NoteAnnotation, 'key'> | undefined): string {
    const hand = stepValue(step, value);
    if (hand === undefined) return '';
    let text = handText(hand);
    if (step.part === 'track') for (const handover of value?.handovers ?? []) text += `→${handover.to}`;
    return text;
  }

  /** 列表上的手：起點／接觸，Slide 另加滑行與換手。 */
  function summary(item: Note, value: Omit<NoteAnnotation, 'key'> | undefined): string {
    if (!value) return '';
    const parts: string[] = [];
    const want = neededParts(item);
    if (want.hand) parts.push(handText(value.hand));
    if (want.track) {
      let track = handText(value.track);
      for (const handover of value.handovers ?? []) track += `→${handover.to}`;
      parts.push(want.hand ? `滑 ${track}` : track);
    }
    return parts.join(' ');
  }

  const inSlide = $derived(
    !!note &&
      note.kind === 'slide' &&
      note.motionStart !== null &&
      note.motionEnd !== null &&
      playback.time > note.motionStart &&
      playback.time < note.motionEnd,
  );

  function onHand(hand: Hand | undefined) {
    if (note) annotation.setHand(note, hand);
  }

  function onTrack(track: TrackHand | undefined) {
    if (note) annotation.setTrack(note, track);
  }

  // ---- 備註 ----
  let rangeFrom = $state('');
  let rangeTo = $state('');
  let rangeMemo = $state('');

  function useLoopRange() {
    const from = playback.loopEnabled ? playback.loopStart : playback.time;
    const to = playback.loopEnabled ? playback.loopEnd : playback.time;
    rangeFrom = from.toFixed(3);
    rangeTo = to.toFixed(3);
  }

  const rangeValid = $derived(
    rangeMemo.trim() !== '' && Number.isFinite(Number.parseFloat(rangeFrom)) && Number.isFinite(Number.parseFloat(rangeTo)),
  );

  function addRange() {
    if (!rangeValid) return;
    annotation.addRange(Number.parseFloat(rangeFrom), Number.parseFloat(rangeTo), rangeMemo);
    rangeMemo = '';
  }

  // ---- 一次清除 ----
  type ClearScope = 'hands' | 'memos' | 'all';
  /** 清除要按兩次：第一次只切成確認狀態。 */
  let confirmingClear = $state<ClearScope | null>(null);
  const counts = $derived.by(() => {
    void annotation.draft.updatedAt;
    return annotation.counts();
  });
  const CLEAR_TEXT: Record<ClearScope, { button: string; ask: (c: { hands: number; memos: number; branches: number }) => string }> = {
    hands: { button: '清除所有手順標註', ask: (c) => `清除 ${c.hands} 顆的手順？備註會保留。` },
    memos: { button: '清除所有備註', ask: (c) => `清除 ${c.memos} 則備註？手順會保留。` },
    all: {
      button: '全部清除',
      ask: (c) =>
        `清除全部手順（${c.hands} 顆）與備註（${c.memos} 則）${c.branches > 0 ? `，連同 ${c.branches} 條打法分支` : ''}？`,
    },
  };

  async function askClear(scope: ClearScope) {
    confirmingClear = scope;
    await tick();
    // 焦點先放在「取消」，誤按 Enter 不會直接清掉。
    document.querySelector<HTMLElement>('.clear-confirm .btn:not(.btn--danger)')?.focus();
  }

  function runClear(scope: ClearScope) {
    confirmingClear = null;
    const before = annotation.clearAll(scope);
    toasts.show({
      id: 'annotation-clear',
      tone: 'ok',
      title: scope === 'hands' ? '已清除手順標註' : scope === 'memos' ? '已清除備註' : '已全部清除',
      body: '按「復原」可以還原。',
      action: { label: '復原', run: () => annotation.restore(before) },
    });
  }

  // ---- 分享 ----
  let importText = $state('');
  let importMode = $state<'fill' | 'replace'>('fill');
  let importMessage = $state<{ tone: 'ok' | 'error'; text: string } | null>(null);
  let importing = $state(false);
  let fileInput = $state<HTMLInputElement | null>(null);

  async function copyAll() {
    const text = await annotation.exportText();
    if (!text) return;
    const ok = await copyText(text);
    toasts.show({
      id: 'annotation-copy',
      tone: ok ? 'ok' : 'error',
      title: ok ? '已複製標註' : '無法寫入剪貼簿',
      body: ok ? `${stats.done} 顆已確認，連同原譜一起，可以直接貼給其他人。` : '請改用匯出檔案。',
    });
  }

  async function download() {
    const text = await annotation.exportText();
    if (!text) return;
    try {
      const path = await saveExport(text, fileName(annotation.draft.title));
      if (path) showSaved('annotation-save', '已匯出標註檔', path);
    } catch (error) {
      toasts.show({ id: 'annotation-save', tone: 'error', title: '匯出失敗', body: String(error) });
    }
  }

  async function pickFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    importText = await file.text();
    input.value = '';
  }

  async function runImport() {
    importMessage = null;
    const parsed = parseFile(importText);
    if (!parsed.ok) {
      importMessage = { tone: 'error', text: parsed.error };
      return;
    }
    importing = true;
    try {
      const result = await annotation.importFile(parsed.file, { mode: importMode, skipped: parsed.skipped });
      importMessage = { tone: result.ok ? 'ok' : 'error', text: result.text };
      if (result.ok) importText = '';
    } finally {
      importing = false;
    }
  }

  // ---- 比對 ----
  const evaluation = $derived(annotation.evaluation);
  const agreement = $derived(
    evaluation && evaluation.compared > 0 ? Math.round((1000 * evaluation.agreed) / evaluation.compared) / 10 : null,
  );
  const gap = $derived(
    evaluation?.model && evaluation.human ? evaluation.human.cost - evaluation.model.cost : null,
  );
  const infeasible = $derived(evaluation?.diagnostics.find((item) => item.code === 'annotation_infeasible') ?? null);
  const showingHuman = $derived(session.humanView !== null && session.humanView === evaluation?.humanSolution);

  function seekTo(time: number, noteId?: string) {
    if (noteId) session.selectNote(noteId);
    playback.seek(time);
  }

  function toggleHumanView() {
    if (showingHuman) {
      session.showHuman(null);
      toasts.dismiss('human-view');
      return;
    }
    if (!evaluation?.humanSolution) return;
    session.showHuman(evaluation.humanSolution);
    toasts.show({
      id: 'human-view',
      tone: 'info',
      title: '盤面顯示：照真人標註的打法',
      body: '這是依標註手順求出的動作，不是模型候選。',
      sticky: true,
      action: {
        label: '回到模型方案',
        run: () => {
          session.showHuman(null);
          toasts.dismiss('human-view');
        },
      },
    });
  }
</script>

<div class="annotate" bind:clientHeight={panelHeight}>
  <header class="head" bind:offsetHeight={headHeight}>
    <div class="row">
      <h2 class="title">真人手順標註</h2>
      <span class="spacer"></span>
      <span class="small muted mono" title="已確認／全部音符">{stats.done}／{stats.total}</span>
      <button
        class="btn"
        onclick={() => void videoSync.open()}
        title={annotation.draft.video ? `對照影片：${annotation.draft.video.title}` : '開啟影片視窗，對照真人手元影片標註'}
      >
        <Icon name="video" size={14} />影片同步
      </button>
    </div>
    <div class="bar-track" role="progressbar" aria-label="標註進度" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
      <div class="bar-fill" style={`width:${percent}%`}></div>
    </div>
    {#if stats.prefilled > 0}
      <p class="xsmall muted">另有 {stats.prefilled} 顆是模型預填，確認後才算真人資料。</p>
    {/if}
    <div class="views" role="group" aria-label="標註面板">
      {#each VIEWS as item (item.id)}
        <button class="view" aria-pressed={view === item.id} onclick={() => (view = item.id)}>{item.label}</button>
      {/each}
    </div>
  </header>

  {#if !session.chart}
    <div class="section">
      <p class="small muted">先在左側開啟或新增譜面並完成分析，才能開始標註。也可以到「分享」匯入別人給的標註檔，會連同原譜一起建立紀錄。</p>
    </div>
  {:else if !annotation.keysReady}
    <div class="section stack-sm">
      <p class="small">這份分析來自舊版核心，缺少音符定位鍵，無法對應標註。</p>
      <button class="btn" onclick={() => void session.analyze()}>
        <Icon name="refresh-cw" size={14} />重新生成
      </button>
    </div>
  {:else if !records.activeId}
    <div class="section">
      <p class="small muted">標註會存在譜面紀錄裡。請先從左側清單開啟一筆紀錄。</p>
    </div>
  {/if}

  {#if view === 'label' && session.chart && annotation.keysReady}
    <div class="upper scroll">
      <section class="section editor" aria-label="目前音符">
        {#if hasBranches && note}
          <div class="route">
            <span class="field-label">這一步的打法</span>
            {#if stepLines.length > 1}
              <LineChips lines={stepLines} current={stepLine} label="這一步的打法，按下切換路線" />
              <span class="xsmall muted"><kbd>B</kbd> 切換</span>
            {:else}
              <span class="small">只有{annotation.lineName(stepLine)}</span>
            {/if}
            {#if splitText}
              <span class="xsmall muted split-note">{splitText}</span>
            {/if}
          </div>
        {/if}
        {#if note}
          <div class="row">
            <button class="btn btn--icon" onclick={() => annotation.step(-1)} aria-label="上一步" title="上一步">
              <Icon name="chevron-left" size={14} />
            </button>
            <div class="note-title">
              <span class="mono small">{formatClock(current?.time ?? note.timeSeconds)}</span>
              <strong>{current ? stepLabel(current, session.pathById) : describe(note)}</strong>
            </div>
            <button class="btn btn--icon" onclick={() => annotation.step(1)} aria-label="下一步" title="下一步">
              <Icon name="chevron-right" size={14} />
            </button>
          </div>

          {#if need.hand}
            <div class="line" class:is-active={split && current?.part === 'hand'}>
              <button class="line-label" onclick={() => annotation.select(note!, 'hand')} disabled={!split} title={split ? '切到起點這一步' : undefined}>
                {note.kind === 'slide' ? '起點' : '手'}
              </button>
              <div class="choices" role="group" aria-label={note.kind === 'slide' ? '起點觸碰的手' : '接觸的手'}>
                <button class="btn choice left" aria-pressed={mark?.hand === 'L'} onclick={() => onHand('L')}>左 L</button>
                <button class="btn choice right" aria-pressed={mark?.hand === 'R'} onclick={() => onHand('R')}>右 R</button>
                <button class="btn btn--icon" onclick={() => onHand(undefined)} disabled={!mark?.hand} aria-label="清除" title="清除">
                  <Icon name="x" size={14} />
                </button>
              </div>
            </div>
          {/if}

          {#if need.track}
            <div class="line" class:is-active={split && current?.part === 'track'}>
              <button class="line-label" onclick={() => annotation.select(note!, 'track')} disabled={!split} title={split ? '切到滑行這一步' : undefined}>
                滑行
              </button>
              <div class="choices" role="group" aria-label="開始滑行的手">
                <button class="btn choice left" aria-pressed={mark?.track === 'L'} onclick={() => onTrack('L')}>左 L</button>
                <button class="btn choice right" aria-pressed={mark?.track === 'R'} onclick={() => onTrack('R')}>右 R</button>
                {#if wifi}
                  <button class="btn choice" aria-pressed={mark?.track === 'LR'} onclick={() => onTrack('LR')} title="兩手一起（2+1）">L+R</button>
                {/if}
                <button class="btn btn--icon" onclick={() => onTrack(undefined)} disabled={!mark?.track} aria-label="清除" title="清除">
                  <Icon name="x" size={14} />
                </button>
              </div>
            </div>
            {#if mark?.track !== 'LR'}
              <div class="line">
                <span class="line-label">換手</span>
                <div class="stack-sm grow">
                  {#each mark?.handovers ?? [] as handover, index (index)}
                    <div class="row small">
                      <button class="linkish mono" onclick={() => seekTo(handover.at)}>{formatClock(handover.at)}</button>
                      <span>換到 {handover.to === 'L' ? '左手' : '右手'}</span>
                      <span class="spacer"></span>
                      <button class="btn btn--icon" onclick={() => annotation.removeHandover(note, index)} aria-label="移除這次換手" title="移除">
                        <Icon name="trash" size={14} />
                      </button>
                    </div>
                  {/each}
                  <button
                    class="btn"
                    disabled={!inSlide}
                    onclick={() => annotation.addHandover(note, Math.round(playback.time * 1000) / 1000)}
                    title="把播放時間拖到換手的那一刻再按"
                  >
                    <Icon name="plus" size={14} />在 {formatClock(playback.time)} 換手
                  </button>
                  {#if !inSlide}
                    <span class="field-hint">播放時間要在這條 Slide 的滑行期間內。</span>
                  {/if}
                </div>
              </div>
            {/if}
          {/if}

          <!-- 有起點的 Slide 起點與滑行各有自己的信心，例如起點確定、滑行兩手皆可。 -->
          {#each confidenceParts as part (part ?? 'one')}
            <div class="line" class:is-active={split && current?.part === part}>
              <span class="line-label">{part === 'hand' ? '起點信心' : part === 'track' ? '滑行信心' : '信心'}</span>
              <div class="choices" role="group" aria-label={part === 'track' ? '滑行的信心' : part === 'hand' ? '起點的信心' : '信心程度'}>
                {#each CONFIDENCES as value (value)}
                  <button
                    class="btn choice"
                    aria-pressed={!!mark && annotation.confidenceOf(note, part ?? undefined) === value}
                    onclick={() => note && annotation.setConfidence(note, value, part ?? undefined)}
                  >
                    {CONFIDENCE_LABEL[value]}
                  </button>
                {/each}
              </div>
            </div>
          {/each}

          <label class="field">
            <span class="field-label">備註</span>
            <textarea
              class="input memo"
              rows="2"
              placeholder="為什麼這樣打、別的打法…"
              value={mark?.memo ?? ''}
              oninput={(event) => note && annotation.setMemo(note, event.currentTarget.value)}
            ></textarea>
          </label>

          <div class="row row-wrap small">
            {#if mark?.prefilled}
              <span class="badge badge--quiet">模型預填，尚未確認</span>
              <button class="btn" onclick={() => note && annotation.confirm(note)}>
                <Icon name="check" size={14} />確認
              </button>
            {/if}
            {#if modelMarks.get(note.id)}
              <span class="muted">模型：{summary(note, modelMarks.get(note.id)) || '—'}</span>
            {/if}
            <span class="spacer"></span>
            <button class="btn btn--ghost" onclick={() => note && annotation.clear(note)} disabled={!mark}>清除這顆</button>
          </div>
        {:else}
          <p class="small muted">在盤面或下方清單點選音符，或按 N 跳到下一顆還沒確認的音符。</p>
        {/if}
        <p class="xsmall muted keys">
          <kbd>A</kbd> 左手　<kbd>D</kbd> 右手（Slide 起點與滑行分兩步）　<kbd>Shift</kbd>+<kbd>A</kbd>/<kbd>D</kbd> 整顆同一手　<kbd>S</kbd> 這一步的信心　<kbd>Enter</kbd> 確認預填　<kbd>Del</kbd> 清除　<kbd>N</kbd> 下一步未確認
        </p>
      </section>

      <section class="section tools" aria-label="批次工具">
        <div class="row row-wrap">
          <button class="btn" disabled={!modelSolution} onclick={() => {
            const count = annotation.prefillFromModel();
            toasts.show({ id: 'annotation-prefill', tone: 'ok', title: `已預填 ${count} 顆`, body: '預填只是草稿，逐顆確認或修改後才算真人資料。' });
          }}>
            <Icon name="zap" size={14} />用模型預填空白
          </button>
          <button class="btn" disabled={stats.prefilled === 0} onclick={() => annotation.confirmAll()}>
            <Icon name="check" size={14} />全部確認
          </button>
          <button class="btn btn--ghost" disabled={stats.prefilled === 0} onclick={() => annotation.clearPrefilled()}>清除預填</button>
        </div>
        <label class="check">
          <input type="checkbox" checked={annotation.autoAdvance} onchange={(event) => annotation.setAutoAdvance(event.currentTarget.checked)} />
          <span>標完自動跳到下一步</span>
        </label>
      </section>
    </div>

    <div class="lower" style={`height:${shownList}px`}>
      <ResizeHandle
        label="調整標註清單高度"
        side="top"
        value={shownList}
        min={LIST_MIN}
        max={listMax}
        defaultValue={LIST_DEFAULT}
        direction={-1}
        onChange={setListHeight}
      />
      <div class="list-head">
        <select class="select filter" aria-label="篩選" value={annotation.filter} onchange={(event) => (annotation.filter = event.currentTarget.value as ListFilter)}>
          {#each FILTERS as item (item.id)}
            <option value={item.id}>{item.label}</option>
          {/each}
        </select>
        <span class="xsmall muted">{rows.length} 步</span>
        {#if pending}
          <span class="badge" role="status">選取分支終點</span>
          <span class="spacer"></span>
          <button class="btn btn--ghost" onclick={() => annotation.cancelMark()} title="取消標記分支（Esc）">取消</button>
        {/if}
      </div>
      {#if pending}
        <div class="mark-bar">
          <span class="small">
            {endCandidate
              ? `${annotation.rangeText(pendingRange.from, pendingRange.to)}，共 ${annotation.stepsIn(pendingRange.from, pendingRange.to).length} 步，從${annotation.lineName(annotation.pendingFromLine)}分出`
              : `起點：第 ${annotation.stepRef(pending)} 顆，從${annotation.lineName(annotation.pendingFromLine)}分出。點選要結束的那一步，或在那一列按右鍵。要併入別條線就先切換路線。`}
          </span>
          {#if endCandidate}
            <span class="spacer"></span>
            <button class="btn btn--primary" onclick={() => endCandidate && finishBranch(endCandidate)}>
              <Icon name="flag" size={14} />標記新分支終點（第 {annotation.stepRef(endCandidate)} 顆，併入{annotation.lineName(endInto)}）
            </button>
          {/if}
        </div>
      {/if}
      <VirtualList
        class="list"
        items={rowViews}
        heightOf={rowHeight}
        keyOf={(row) => row.step.key}
        {focusIndex}
        label="音符標註清單"
      >
        {#snippet row(row: RowView, _index: number, style: string)}
          {@const step = row.step}
          <button
            class="item"
            class:is-pending={!!pending && inBranch(pendingRange, step.time)}
            class:is-selected={current?.key === step.key}
            class:is-current={currentKey === step.key}
            data-step={step.key}
            style={`${style}${hasBranches ? `;padding-left:calc(var(--space-4) + ${laneColumnWidth(laneCount) + 8}px)` : ''}`}
            role="option"
            aria-selected={current?.key === step.key}
            onclick={() => annotation.selectStep(step)}
            oncontextmenu={(event) => openMenu(step, event)}
            onkeydown={(event) => onRowKeydown(step, event)}
          >
            {#if listLanes[row.index]}<LaneGraph row={listLanes[row.index]} lanes={laneCount} />{/if}
            <!-- 有起點的 Slide 分兩步標：滑行那一列是同一顆的第二步，編號前加箭頭區分。 -->
            <span class="mono xsmall muted num" title={row.first ? `第 ${row.number} 顆` : `第 ${row.number} 顆的滑行（同一顆的第二步）`}>
              {row.first ? row.number : `↳${row.number}`}
            </span>
            <span class="mono xsmall muted time">{formatClock(step.time)}</span>
            <span class="what small">{stepLabel(step, session.pathById)}</span>
            <span class="hands small mono" class:is-prefilled={row.prefilled} class:is-empty={!row.text}>
              {row.text || '未標'}
            </span>
            <span class="flags xsmall">
              {#if row.differs}<span class="badge" title="與模型方案不同">≠模型</span>{/if}
              {#if row.doubt}<span class="badge badge--quiet">{CONFIDENCE_LABEL[row.doubt]}</span>{/if}
              {#if row.memo}<span class="badge badge--quiet" title={row.memo}>備註</span>{/if}
              {#if row.prefilled}<span class="badge badge--quiet">預填</span>{/if}
              {#if row.pendingStart}<span class="badge">分支起點</span>{/if}
            </span>
          </button>
        {/snippet}
        {#snippet empty()}
          <p class="small muted empty">沒有符合條件的步驟。</p>
        {/snippet}
      </VirtualList>
    </div>
    {#if menu && menuStep}
      <ContextMenu
        items={menuItems}
        x={menu.x}
        y={menu.y}
        label={`第 ${annotation.stepRef(menuStep)} 顆的動作`}
        onSelect={selectMenu}
        onClose={() => (menu = null)}
      />
    {/if}
  {/if}

  {#if view === 'branch' && session.chart && annotation.keysReady}
    <div class="upper scroll">
      <Deferred label="載入打法分支">
        <BranchPanel />
      </Deferred>
    </div>
  {/if}

  {#if view === 'memo'}
    <section class="section stack">
      <label class="field">
        <span class="field-label">你的名字</span>
        <input class="input" value={annotation.annotator} placeholder="協作時用來區分是誰標的" onchange={(event) => annotation.setAnnotator(event.currentTarget.value)} />
        <span class="field-hint">
          記在每一顆的標註上。{annotation.draft.annotators.length > 0 ? `這份標註的參與者：${annotation.draft.annotators.join('、')}` : ''}
        </span>
      </label>
      <label class="field">
        <span class="field-label">整體備註</span>
        <textarea
          class="input memo"
          rows="4"
          placeholder="例如：這份是 SSS 玩家的打法、手順的整體習慣、不確定的段落…"
          value={annotation.draft.memo}
          disabled={!records.activeId}
          oninput={(event) => annotation.setOverallMemo(event.currentTarget.value)}
        ></textarea>
      </label>
    </section>
    <section class="section stack-sm">
      <div class="section-title">區段備註</div>
      {#each annotation.draft.ranges as range, index (index)}
        <div class="range">
          <button class="linkish mono small" onclick={() => seekTo(range.from)}>{formatClock(range.from)}–{formatClock(range.to)}</button>
          <span class="small grow">{range.memo}{range.by ? `（${range.by}）` : ''}</span>
          <button class="btn btn--icon" onclick={() => annotation.removeRange(index)} aria-label="刪除這段備註" title="刪除">
            <Icon name="trash" size={14} />
          </button>
        </div>
      {:else}
        <p class="small muted">還沒有區段備註。可以記「這段真人會交叉」「這段兩種打法都常見」之類的說明。</p>
      {/each}
      <div class="row">
        <input class="input mono" aria-label="開始秒數" placeholder="開始秒" bind:value={rangeFrom} />
        <input class="input mono" aria-label="結束秒數" placeholder="結束秒" bind:value={rangeTo} />
        <button class="btn" onclick={useLoopRange} title="循環開啟時取循環範圍，否則取目前時間">取目前範圍</button>
      </div>
      <input class="input" placeholder="這段的說明" bind:value={rangeMemo} onkeydown={(event) => event.key === 'Enter' && addRange()} />
      <button class="btn" disabled={!rangeValid || !records.activeId} onclick={addRange}>
        <Icon name="plus" size={14} />加入區段備註
      </button>
    </section>
  {/if}

  {#if view === 'memo'}
    <section class="section stack-sm" aria-label="一次清除">
      <div class="section-title">一次清除</div>
      <p class="xsmall muted">只影響目前這份譜面的標註。清除後可在提示上按「復原」。</p>
      {#if confirmingClear}
        <div class="clear-confirm alert alert--warn" role="alertdialog" aria-label="確認清除">
          <p class="small">{CLEAR_TEXT[confirmingClear].ask(counts)}</p>
          <div class="row">
            <button class="btn btn--danger" onclick={() => confirmingClear && runClear(confirmingClear)}>
              <Icon name="trash" size={14} />清除
            </button>
            <button class="btn" onclick={() => (confirmingClear = null)}>
              <Icon name="x" size={14} />取消
            </button>
          </div>
        </div>
      {:else}
        <div class="row row-wrap">
          <button class="btn" disabled={!records.activeId || counts.hands === 0} onclick={() => askClear('hands')}>
            {CLEAR_TEXT.hands.button}
          </button>
          <button class="btn" disabled={!records.activeId || counts.memos === 0} onclick={() => askClear('memos')}>
            {CLEAR_TEXT.memos.button}
          </button>
          <button class="btn" disabled={!records.activeId || counts.hands + counts.memos === 0} onclick={() => askClear('all')}>
            {CLEAR_TEXT.all.button}
          </button>
        </div>
      {/if}
    </section>
  {/if}

  {#if view === 'share'}
    <section class="section stack-sm">
      <div class="section-title">分享這份標註</div>
      <p class="small muted">標註連同原譜一起轉成標註檔（maimotion-hand-annotation），每顆音符一行。複製後可以直接貼給其他人，對方匯入就能合併。</p>
      <div class="row row-wrap">
        <button class="btn btn--primary" disabled={!annotation.keysReady || !session.chart} onclick={copyAll}>
          <Icon name="copy" size={14} />複製標註
        </button>
        <button class="btn" disabled={!annotation.keysReady || !session.chart} onclick={download}>
          <Icon name="download" size={14} />匯出檔案
        </button>
      </div>
    </section>
    <section class="section stack-sm">
      <div class="section-title">匯入別人的標註</div>
      <textarea class="textarea" rows="5" placeholder="貼上標註檔內容" bind:value={importText}></textarea>
      <div class="row row-wrap">
        <button class="btn" onclick={() => fileInput?.click()}>
          <Icon name="upload" size={14} />開啟檔案
        </button>
        <input class="sr-only" type="file" accept=".json,application/json" bind:this={fileInput} onchange={pickFile} tabindex="-1" />
      </div>
      <fieldset class="modes">
        <legend class="field-label">同一顆音符手順不同時</legend>
        <label class="check">
          <input type="radio" name="import-mode" value="fill" bind:group={importMode} />
          <span>保留我的，只補我還沒標的</span>
        </label>
        <label class="check">
          <input type="radio" name="import-mode" value="replace" bind:group={importMode} />
          <span>改用匯入的</span>
        </label>
      </fieldset>
      <button class="btn" disabled={importText.trim() === '' || importing} onclick={runImport}>
        <Icon name="upload" size={14} />匯入並合併
      </button>
      {#if importMessage}
        <div class="alert" class:alert--error={importMessage.tone === 'error'} role="status">{importMessage.text}</div>
      {/if}
      <p class="xsmall muted">匯入時以原譜比對：已有同一份譜面的紀錄就合併進去，沒有就連同原譜新增一筆紀錄。備註與區段備註兩邊都會保留。</p>
    </section>
  {/if}

  {#if view === 'compare'}
    <section class="section stack-sm">
      <div class="section-title">和模型比對</div>
      <p class="small muted">用「參數」分頁目前的設定，分別求出模型最佳解與「照你的標註打」的最佳解。只用已確認且信心為「確定」的步驟（Slide 的起點與滑行分開看）。</p>
      {#if hasBranches}
        <p class="small">比對的路線：<strong>{annotation.lineName(annotation.draft.active)}</strong>（到「打法」分頁切換）</p>
      {/if}
      <button class="btn btn--primary" disabled={!session.desktop || annotation.evaluating || session.phase === 'analyzing' || stats.done === 0} onclick={() => void annotation.evaluate()}>
        <Icon name={annotation.evaluating ? 'loader' : 'compare'} size={14} spin={annotation.evaluating} />
        {annotation.evaluating ? '比對中…' : '開始比對'}
      </button>
      {#if !session.desktop}
        <p class="xsmall muted">瀏覽器預覽沒有 Rust 核心，比對只能在桌面版執行。</p>
      {:else if stats.done === 0}
        <p class="xsmall muted">還沒有確認過的標註。</p>
      {/if}
      {#if annotation.evaluationError}
        <div class="alert alert--error" role="alert">{annotation.evaluationError}</div>
      {/if}
    </section>

    {#if evaluation}
      <section class="section stack-sm" aria-label="比對結果">
        <dl class="kv">
          <dt>參與比對</dt>
          <dd>{evaluation.matched} 顆{evaluation.unmatchedKeys.length > 0 ? `（${evaluation.unmatchedKeys.length} 顆對不上音符）` : ''}</dd>
          <dt>吻合</dt>
          <dd>{agreement === null ? '—' : `${evaluation.agreed}／${evaluation.compared}（${agreement}%）`}</dd>
          <dt>模型分數</dt>
          <dd>{evaluation.model ? evaluation.model.cost.toFixed(3) : '—'}</dd>
          <dt>照標註</dt>
          <dd>{evaluation.human ? evaluation.human.cost.toFixed(3) : '做不到'}</dd>
          {#if gap !== null}
            <dt>差距</dt>
            <dd>{gap >= 0 ? '+' : ''}{gap.toFixed(3)}</dd>
          {/if}
        </dl>
        <p class="xsmall muted">分數越低越好。差距越小，代表目前參數越認同真人的打法；若照標註反而更低，是搜尋沒找到真人打法。</p>
        {#if infeasible}
          <div class="alert alert--warn" role="status">
            <div class="alert-title">模型照標註走不下去</div>
            <p>{infeasible.message}</p>
            {#if infeasible.timeSeconds !== null}
              <button class="linkish mono" onclick={() => seekTo(infeasible.timeSeconds ?? 0, infeasible.noteIds[0])}>
                {formatClock(infeasible.timeSeconds)}（{infeasible.noteIds.join('、')}）
              </button>
            {/if}
          </div>
        {/if}
        {#if evaluation.humanSolution}
          <button class="btn" onclick={toggleHumanView} aria-pressed={showingHuman}>
            <Icon name="eye" size={14} />{showingHuman ? '回到模型方案' : '在盤面看照標註的打法'}
          </button>
        {/if}
      </section>
      {#if evaluation.divergences.length > 0}
        <section class="section stack-sm">
          <div class="section-title">與模型不同（{evaluation.divergences.length}）</div>
          <div class="divergences">
            {#each evaluation.divergences as item (item.noteId + item.part)}
              <button class="item" onclick={() => seekTo(item.timeSeconds, item.noteId)}>
                <span class="mono xsmall muted time">{formatClock(item.timeSeconds)}</span>
                <span class="what small">{session.noteById.get(item.noteId) ? describe(session.noteById.get(item.noteId)!) : item.key}{item.part === 'track' ? '（滑行）' : ''}</span>
                <span class="hands small mono">真人 {handText(item.human)}・模型 {handText(item.model)}</span>
              </button>
            {/each}
          </div>
        </section>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .annotate {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .head {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .title {
    font-size: var(--fs-md);
  }

  /* 第二層分頁：底線式，貼在標頭的底邊上，和上面第一層的分段控制區分。 */
  .views {
    display: flex;
    margin: var(--space-1) calc(-1 * var(--space-4)) calc(-1 * var(--space-3) - 1px);
    padding: 0 var(--space-2);
  }

  .view {
    flex: 1 1 0;
    min-width: 0;
    min-height: 34px;
    padding: 0 var(--space-1);
    color: var(--c-text-dim);
    font: inherit;
    font-size: var(--fs-sm);
    white-space: nowrap;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    transition:
      color 120ms ease,
      border-color 120ms ease;
  }

  .view:hover {
    color: var(--c-text);
    border-bottom-color: var(--c-border-strong);
  }

  .view[aria-pressed='true'] {
    color: var(--c-text-strong);
    font-weight: 600;
    border-bottom-color: var(--c-accent);
  }

  .view:focus-visible {
    outline: 2px solid var(--c-focus);
    outline-offset: -2px;
  }

  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .note-title {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    align-items: center;
    min-width: 0;
    line-height: var(--lh-tight);
  }

  .line {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr);
    align-items: start;
    gap: var(--space-2);
  }

  .line-label {
    padding: 6px 0 0;
    background: none;
    border: none;
    font-size: var(--fs-sm);
    color: var(--c-text-dim);
    text-align: left;
    cursor: pointer;
  }

  .line-label:disabled {
    cursor: default;
  }

  /* 目前這一步：A／D 會標到這一行。用淺底色標出，不用左側色條。 */
  .line.is-active {
    margin: 0 calc(-1 * var(--space-2));
    padding: var(--space-1) var(--space-2);
    background: var(--c-control);
    border-radius: var(--radius-md);
  }

  .line.is-active .line-label {
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .choice {
    min-width: 56px;
  }

  /* 左右手選中時沿用左右手的顏色，另有文字 L／R 區分，不只靠顏色。 */
  .choice.left[aria-pressed='true'],
  .choice.left[aria-pressed='true']:hover:not(:disabled) {
    background: var(--c-left);
    border-color: var(--c-left);
  }

  .choice.right[aria-pressed='true'],
  .choice.right[aria-pressed='true']:hover:not(:disabled) {
    background: var(--c-right);
    border-color: var(--c-right);
  }

  .grow {
    flex: 1 1 auto;
    min-width: 0;
  }

  .memo {
    resize: vertical;
    min-height: 0;
    font-family: var(--font-sans);
  }

  .keys {
    line-height: 1.9;
  }

  kbd {
    padding: 0 var(--space-1);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-sm);
    background: var(--c-control);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }

  .tools {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .list-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .filter {
    width: auto;
    padding: var(--space-1) var(--space-2);
  }

  /* 上方編輯區吃剩下的高度，內容多時自己捲動。 */
  .upper {
    flex: 1 1 auto;
    min-height: 0;
  }

  /* 清單固定高度貼在底部：選到哪顆音符都不會讓清單上緣移動。 */
  .lower {
    position: relative;
    display: flex;
    flex: 0 0 auto;
    flex-direction: column;
    min-height: 0;
    border-top: 1px solid var(--c-border);
  }

  .lower :global(.list) {
    flex: 1 1 auto;
    min-height: 0;
  }

  .route {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }

  .split-note {
    flex-basis: 100%;
  }

  .mark-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--c-border);
    background: var(--c-control);
  }

  /* 標記中的新分支範圍：淺底色標出，不用色條。 */
  .item.is-pending {
    background: var(--c-control);
  }

  .item :global(.lanes) {
    left: var(--space-4);
  }

  /* 位置與固定列高由 VirtualList 給（見 ROW_HEIGHT）。 */
  .item {
    overflow: hidden;
    display: grid;
    grid-template-columns: minmax(20px, auto) 60px minmax(0, 1fr) auto;
    grid-template-areas:
      'num time what hands'
      'num time flags flags';
    align-items: center;
    column-gap: var(--space-2);
    width: 100%;
    padding: var(--space-1) var(--space-4);
    background: none;
    border: none;
    border-bottom: 1px solid var(--c-border);
    text-align: left;
    cursor: pointer;
  }

  .item:hover {
    background: var(--c-control);
  }

  .item.is-current .time {
    color: var(--c-accent);
  }

  .item.is-selected {
    background: var(--c-control-hover);
  }

  .num {
    grid-area: num;
    text-align: right;
  }

  .time {
    grid-area: time;
  }

  .what {
    grid-area: what;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hands {
    grid-area: hands;
    white-space: nowrap;
  }

  /* 預填以虛線外框表示，與確認過的實心文字區分。 */
  .hands.is-prefilled {
    padding: 0 var(--space-1);
    border: 1px dashed var(--c-border-strong);
    border-radius: var(--radius-sm);
    color: var(--c-text-dim);
  }

  .hands.is-empty {
    color: var(--c-text-dim);
  }

  .flags {
    grid-area: flags;
    display: flex;
    flex-wrap: nowrap;
    overflow: hidden;
    gap: var(--space-1);
  }

  .flags:empty {
    display: none;
  }

  .empty {
    padding: var(--space-4);
  }

  .range {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .range .linkish {
    flex: none;
    white-space: nowrap;
  }

  .modes {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    border: none;
  }

  .divergences {
    margin: 0 calc(-1 * var(--space-4));
  }
</style>
