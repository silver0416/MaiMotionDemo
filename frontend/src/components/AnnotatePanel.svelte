<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import { KIND_LABEL } from '../lib/contract';
  import {
    CONFIDENCE_LABEL,
    fileName,
    isHumanLabeled,
    isWifi,
    modelMark,
    neededParts,
    parseFile,
    progress,
  } from '../lib/annotation';
  import { copyText } from '../lib/debug';
  import { hashText } from '../lib/db';
  import { formatClock } from '../lib/format';
  import { noteTarget } from '../lib/notes';
  import { annotation, type ListFilter } from '../state/annotation.svelte';
  import { playback } from '../state/playback.svelte';
  import { records } from '../state/records.svelte';
  import { session } from '../state/session.svelte';
  import { toasts } from '../state/toasts.svelte';
  import type { Confidence, Hand, Note, NoteAnnotation, TrackHand } from '../lib/types';

  type View = 'label' | 'memo' | 'share' | 'compare';
  const VIEWS: { id: View; label: string }[] = [
    { id: 'label', label: '逐顆標註' },
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

  const note = $derived<Note | null>(session.selectedNote);
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

  const stats = $derived(progress(annotation.draft, annotation.ordered));
  const percent = $derived(stats.total > 0 ? Math.round((100 * stats.done) / stats.total) : 0);

  function differs(item: Note, human: NoteAnnotation | undefined): boolean {
    if (!isHumanLabeled(human)) return false;
    const model = modelMarks.get(item.id);
    if (!model || !human) return false;
    if (human.hand && model.hand && human.hand !== model.hand) return true;
    if (human.track && model.track && human.track !== model.track) return true;
    return false;
  }

  const rows = $derived.by(() => {
    const filter = annotation.filter;
    return annotation.ordered.filter((item) => {
      const human = annotation.mark(item);
      switch (filter) {
        case 'todo':
          return !isHumanLabeled(human);
        case 'prefilled':
          return human?.prefilled === true;
        case 'diff':
          return differs(item, human);
        case 'memo':
          return !!human?.memo;
        case 'unsure':
          return human?.confidence === 'unsure' || human?.confidence === 'either';
        default:
          return true;
      }
    });
  });

  /** 播放時間所在的音符（最後一顆判定時間不晚於目前時間的）。 */
  const currentId = $derived.by(() => {
    const list = annotation.ordered;
    let low = 0;
    let high = list.length - 1;
    let found = -1;
    const time = playback.time + 1e-6;
    while (low <= high) {
      const mid = (low + high) >> 1;
      if (list[mid].timeSeconds <= time) {
        found = mid;
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return found >= 0 ? list[found].id : null;
  });

  let listElement = $state<HTMLDivElement | null>(null);

  // 選取的音符捲進可視範圍（盤面點選、快捷鍵前進時）。
  $effect(() => {
    const id = session.selectedNoteId;
    if (!id || !listElement) return;
    queueMicrotask(() => {
      listElement?.querySelector(`[data-note-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
    });
  });

  function describe(item: Note): string {
    const kind = KIND_LABEL[item.kind] ?? item.kind;
    if (item.kind === 'slide' && item.pathId) {
      const path = session.pathById.get(item.pathId);
      if (path) return `${kind} ${path.startButton}${path.shape}${path.endButton}`;
    }
    return `${kind} ${noteTarget(item)}`;
  }

  function handText(value: TrackHand | undefined): string {
    if (!value) return '—';
    return value === 'LR' ? 'L+R' : value;
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
  const CLEAR_TEXT: Record<ClearScope, { button: string; ask: (c: { hands: number; memos: number }) => string }> = {
    hands: { button: '清除所有手順標註', ask: (c) => `清除 ${c.hands} 顆的手順？備註會保留。` },
    memos: { button: '清除所有備註', ask: (c) => `清除 ${c.memos} 則備註？手順會保留。` },
    all: { button: '全部清除', ask: (c) => `清除全部手順（${c.hands} 顆）與備註（${c.memos} 則）？` },
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
      body: ok ? `${stats.done} 顆已確認，連同原譜一起，可以直接貼給其他人。` : '請改用下載檔案。',
    });
  }

  async function download() {
    const text = await annotation.exportText();
    if (!text) return;
    const blob = new Blob([text], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = fileName(annotation.draft.title);
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
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
      const file = parsed.file;
      const hash = await hashText(file.chart.source);
      if (file.chart.sha256 && file.chart.sha256 !== hash) {
        importMessage = { tone: 'error', text: '標註檔裡的原譜被改過（雜湊不符），為避免對錯音符已停止匯入。' };
        return;
      }
      // 找同一份原譜的紀錄；沒有就連同原譜新增一筆。
      let record = records.findByHash(hash);
      let created = false;
      if (!record) {
        record = await records.add(file.chart.source, { name: file.title || undefined });
        created = true;
      }
      const switching = records.activeId !== record.id;
      records.activeId = record.id;
      annotation.bind(record);
      const result = annotation.applyMerge(file, importMode);
      if (switching || !session.result || session.result.source !== record.source) {
        void session.load(record.source);
      }
      const parts = [
        created ? '已連同原譜新增譜面紀錄' : switching ? '已切換到同一份譜面的紀錄' : '已合併到目前的譜面',
        `新增 ${result.added} 顆`,
      ];
      if (result.conflicts.length > 0) {
        parts.push(
          importMode === 'fill'
            ? `${result.conflicts.length} 顆手順不同，保留你的`
            : `${result.conflicts.length} 顆手順不同，已改用匯入的`,
        );
      }
      if (parsed.skipped > 0) parts.push(`略過 ${parsed.skipped} 筆格式不對的項目`);
      importMessage = { tone: 'ok', text: `${parts.join('，')}。` };
      importText = '';
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

<div class="annotate">
  <header class="head">
    <div class="row">
      <h2 class="title">真人手順標註</h2>
      <span class="spacer"></span>
      <span class="small muted mono" title="已確認／全部音符">{stats.done}／{stats.total}</span>
    </div>
    <div class="bar-track" role="progressbar" aria-label="標註進度" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
      <div class="bar-fill" style={`width:${percent}%`}></div>
    </div>
    {#if stats.prefilled > 0}
      <p class="xsmall muted">另有 {stats.prefilled} 顆是模型預填，確認後才算真人資料。</p>
    {/if}
    <div class="views" role="group" aria-label="標註面板">
      {#each VIEWS as item (item.id)}
        <button class="btn view" aria-pressed={view === item.id} onclick={() => (view = item.id)}>{item.label}</button>
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
    <section class="section editor" aria-label="目前音符">
      {#if note}
        <div class="row">
          <button class="btn btn--icon" onclick={() => annotation.step(-1)} aria-label="上一顆" title="上一顆">
            <Icon name="chevron-left" size={14} />
          </button>
          <div class="note-title">
            <span class="mono small">{formatClock(note.timeSeconds)}</span>
            <strong>{describe(note)}</strong>
          </div>
          <button class="btn btn--icon" onclick={() => annotation.step(1)} aria-label="下一顆" title="下一顆">
            <Icon name="chevron-right" size={14} />
          </button>
        </div>

        {#if need.hand}
          <div class="line">
            <span class="line-label">{note.kind === 'slide' ? '起點' : '手'}</span>
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
          <div class="line">
            <span class="line-label">滑行</span>
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

        <div class="line">
          <span class="line-label">信心</span>
          <div class="choices" role="group" aria-label="信心程度">
            {#each CONFIDENCES as value (value)}
              <button
                class="btn choice"
                aria-pressed={(mark?.confidence ?? 'sure') === value && !!mark}
                onclick={() => note && annotation.setConfidence(note, value)}
              >
                {CONFIDENCE_LABEL[value]}
              </button>
            {/each}
          </div>
        </div>

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
        <kbd>A</kbd> 左手　<kbd>D</kbd> 右手　<kbd>Shift</kbd>+<kbd>A</kbd>/<kbd>D</kbd> 只標滑行　<kbd>S</kbd> 信心　<kbd>Enter</kbd> 確認預填　<kbd>Del</kbd> 清除　<kbd>N</kbd> 下一顆未確認
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
        <span>標完自動跳到下一顆</span>
      </label>
    </section>

    <div class="list-head">
      <select class="select filter" aria-label="篩選" value={annotation.filter} onchange={(event) => (annotation.filter = event.currentTarget.value as ListFilter)}>
        {#each FILTERS as item (item.id)}
          <option value={item.id}>{item.label}</option>
        {/each}
      </select>
      <span class="xsmall muted">{rows.length} 顆</span>
    </div>
    <div class="list scroll" bind:this={listElement} role="listbox" aria-label="音符標註清單">
      {#each rows as item (item.id)}
        {@const human = annotation.mark(item)}
        {@const labeled = isHumanLabeled(human)}
        <button
          class="item"
          class:is-selected={session.selectedNoteId === item.id}
          class:is-current={currentId === item.id}
          data-note-id={item.id}
          role="option"
          aria-selected={session.selectedNoteId === item.id}
          onclick={() => annotation.select(item)}
        >
          <span class="mono xsmall muted time">{formatClock(item.timeSeconds)}</span>
          <span class="what small">{describe(item)}</span>
          <span class="hands small mono" class:is-prefilled={human?.prefilled} class:is-empty={!human}>
            {summary(item, human) || '未標'}
          </span>
          <span class="flags xsmall">
            {#if differs(item, human)}<span class="badge" title="與模型方案不同">≠模型</span>{/if}
            {#if human?.confidence && human.confidence !== 'sure'}<span class="badge badge--quiet">{CONFIDENCE_LABEL[human.confidence]}</span>{/if}
            {#if human?.memo}<span class="badge badge--quiet" title={human.memo}>備註</span>{/if}
            {#if !labeled && human?.prefilled}<span class="badge badge--quiet">預填</span>{/if}
          </span>
        </button>
      {:else}
        <p class="small muted empty">沒有符合條件的音符。</p>
      {/each}
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
          <Icon name="download" size={14} />下載檔案
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
      <p class="small muted">用「參數」分頁目前的設定，分別求出模型最佳解與「照你的標註打」的最佳解。只用已確認且信心為「確定」的音符。</p>
      <button class="btn btn--primary" disabled={!session.desktop || annotation.evaluating || stats.done === 0} onclick={() => void annotation.evaluate()}>
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

  .views {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--space-1);
  }

  .view {
    padding: 0 var(--space-1);
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
    grid-template-columns: 40px minmax(0, 1fr);
    align-items: start;
    gap: var(--space-2);
  }

  .line-label {
    padding-top: 6px;
    font-size: var(--fs-sm);
    color: var(--c-text-dim);
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

  .list {
    flex: 1 1 auto;
    min-height: 160px;
  }

  .item {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr) auto;
    grid-template-areas:
      'time what hands'
      'time flags flags';
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
    flex-wrap: wrap;
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
