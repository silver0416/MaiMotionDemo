<script lang="ts">
  import { tick as nextTick } from 'svelte';
  import Icon from './Icon.svelte';
  import ContextMenu, { type MenuItem } from './ContextMenu.svelte';
  import { MODE_LABEL } from '../lib/contract';
  import { formatClock } from '../lib/format';
  import { sampleWithStarts } from '../lib/motion';
  import { markers } from '../state/markers.svelte';
  import { playback, RATES } from '../state/playback.svelte';
  import type { TimelineMarker } from '../state/records.svelte';
  import { session } from '../state/session.svelte';

  const bounds = $derived(session.bounds);
  const span = $derived(Math.max(bounds.end - bounds.start, 1e-6));
  const time = $derived(playback.time);
  const ready = $derived(session.result !== null);

  function ratio(value: number): number {
    return ((value - bounds.start) / span) * 100;
  }

  function clampTime(value: number): number {
    return Math.min(Math.max(value, bounds.start), bounds.end);
  }

  const leftState = $derived.by(() => {
    const solution = session.solution;
    return solution ? sampleWithStarts(solution.leftSegments, session.leftStarts, time) : null;
  });
  const rightState = $derived.by(() => {
    const solution = session.solution;
    return solution ? sampleWithStarts(solution.rightSegments, session.rightStarts, time) : null;
  });

  function handText(state: typeof leftState): string {
    if (!state) return '—';
    const mode = MODE_LABEL[state.mode] ?? state.mode;
    return state.noteId ? `${mode} ${state.noteId}` : mode;
  }

  // ---- 片段範圍 ----

  /** 循環開啟且不是整段時，範圍外的時間軸改成暗底、音符標記改灰。 */
  const dimOutside = $derived(playback.loopEnabled && !playback.rangeIsFull);

  function outside(value: number): boolean {
    return dimOutside && (value < playback.loopStart - 1e-6 || value > playback.loopEnd + 1e-6);
  }

  interface Tick {
    id: string;
    left: number;
    hand: string;
    kind: string;
    muted: boolean;
  }

  const ticks = $derived.by<Tick[]>(() =>
    session.notes.map((note) => {
      const assignments = session.assignmentsByNote.get(note.id) ?? [];
      const hands = [...new Set(assignments.map((item) => item.hand))];
      return {
        id: note.id,
        left: ratio(note.timeSeconds),
        hand: hands.length === 1 ? hands[0] : hands.length > 1 ? 'B' : 'N',
        kind: note.kind,
        muted: outside(note.timeSeconds),
      };
    }),
  );

  /** 手掌覆蓋區間：時間軸上緣一條該手顏色的細條，與音符標記分層不重疊。 */
  const palmBands = $derived.by(() =>
    session.palmPlacements.map((palm, index) => ({
      id: `${palm.hand}-${index}`,
      hand: palm.hand,
      left: ratio(palm.startSeconds),
      width: Math.max(0.6, ratio(palm.endSeconds) - ratio(palm.startSeconds)),
      muted: outside(palm.startSeconds) && outside(palm.endSeconds),
    })),
  );

  const handoverTicks = $derived.by(() =>
    (session.solution?.handovers ?? []).map((handover, index) => ({
      id: `${handover.noteId}-${index}`,
      left: ratio(handover.startSeconds),
      width: Math.max(0.6, ratio(handover.endSeconds) - ratio(handover.startSeconds)),
      muted: outside(handover.startSeconds) && outside(handover.endSeconds),
    })),
  );

  /** 用按鈕或快捷鍵設起點／終點時一併開啟循環，設了馬上看得到效果。 */
  function setStart(value = time) {
    playback.setLoopStart(value);
    if (!playback.loopEnabled) playback.setLoop(true);
  }

  function setEnd(value = time) {
    playback.setLoopEnd(value);
    if (!playback.loopEnabled) playback.setLoop(true);
  }

  let rangeLane: HTMLElement | null = $state(null);
  let rangeDrag = $state<{ which: 'start' | 'end'; pointerId: number } | null>(null);

  function timeAt(element: HTMLElement | null, clientX: number): number {
    if (!element) return time;
    const rect = element.getBoundingClientRect();
    const x = rect.width > 0 ? (clientX - rect.left) / rect.width : 0;
    return clampTime(bounds.start + Math.min(Math.max(x, 0), 1) * span);
  }

  function onHandleDown(which: 'start' | 'end', event: PointerEvent) {
    if (event.button !== 0 || !ready) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    (event.currentTarget as HTMLElement).focus();
    rangeDrag = { which, pointerId: event.pointerId };
    if (!playback.loopEnabled) playback.setLoop(true);
  }

  function onHandleMove(event: PointerEvent) {
    if (!rangeDrag || event.pointerId !== rangeDrag.pointerId) return;
    const value = Math.round(timeAt(rangeLane, event.clientX) * 1000) / 1000;
    if (rangeDrag.which === 'start') playback.setLoopStart(value);
    else playback.setLoopEnd(value);
  }

  function onHandleUp(event: PointerEvent) {
    if (rangeDrag && event.pointerId === rangeDrag.pointerId) rangeDrag = null;
  }

  function onHandleKey(which: 'start' | 'end', event: KeyboardEvent) {
    const step = event.shiftKey ? 1 : 0.1;
    const current = which === 'start' ? playback.loopStart : playback.loopEnd;
    let next: number | null = null;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') next = current - step;
    else if (event.key === 'ArrowRight' || event.key === 'ArrowUp') next = current + step;
    else if (event.key === 'Home') next = bounds.start;
    else if (event.key === 'End') next = bounds.end;
    if (next === null) return;
    event.preventDefault();
    event.stopPropagation();
    if (which === 'start') setStart(clampTime(next));
    else setEnd(clampTime(next));
  }

  // ---- 標籤 ----

  let lane: HTMLElement | null = $state(null);
  let laneWidth = $state(0);
  let editor: HTMLInputElement | null = $state(null);
  let draftLabel = $state('');

  /** 拖曳中的標籤：moved 之前都當成點擊。 */
  let markerDrag = $state<{
    id: string;
    pointerId: number;
    startX: number;
    origin: number;
    time: number;
    moved: boolean;
  } | null>(null);

  const CHIP_MAX = 132;
  /** 放得下至少兩三個字的最小寬度；再窄就只畫旗桿。 */
  const CHIP_MIN = 30;
  const CHIP_GAP = 3;

  function estimateWidth(text: string): number {
    let width = 0;
    for (const char of text) width += (char.codePointAt(0) ?? 0) > 0x2e80 ? 11 : 6.3;
    return Math.min(CHIP_MAX, Math.ceil(width) + 16);
  }

  interface Chip {
    marker: TimelineMarker;
    time: number;
    left: number;
    /** 右側放不下時標籤改往左長。 */
    alignEnd: boolean;
    /** 兩側都放不下時只畫旗桿，名稱放到提示。 */
    compact: boolean;
    /** 不可蓋到下一個標籤的位置，名稱依可用寬度截斷。 */
    maxWidth: number;
    muted: boolean;
  }

  const chips = $derived.by<Chip[]>(() => {
    const width = Math.max(laneWidth, 1);
    const placed = markers.items
      .map((marker) => {
        const at = markerDrag?.id === marker.id && markerDrag.moved ? markerDrag.time : marker.time;
        return { marker, time: at, x: (Math.min(Math.max(ratio(at), 0), 100) / 100) * width };
      })
      .sort((a, b) => a.time - b.time);
    let occupied = -Infinity;
    return placed.map((item, index) => {
      const floating =
        markers.editingId === item.marker.id || (markerDrag?.id === item.marker.id && markerDrag.moved);
      const nextX = placed[index + 1]?.x ?? width;
      const wanted = estimateWidth(chipLabel({ marker: item.marker, time: item.time }));
      const rightRoom = nextX - item.x - CHIP_GAP;
      const leftRoom = item.x - Math.max(occupied, 0) - CHIP_GAP;
      let alignEnd = false;
      let compact = false;
      let maxWidth = CHIP_MAX;
      if (floating) {
        // 改名或拖曳中的標籤浮在最上層，完整顯示。
        alignEnd = item.x + wanted > width;
      } else if (rightRoom >= Math.min(wanted, CHIP_MIN) || rightRoom >= leftRoom) {
        if (rightRoom >= CHIP_MIN || rightRoom >= wanted) maxWidth = Math.min(CHIP_MAX, rightRoom);
        else compact = true;
      } else if (leftRoom >= CHIP_MIN || leftRoom >= wanted) {
        alignEnd = true;
        maxWidth = Math.min(CHIP_MAX, leftRoom);
      } else {
        compact = true;
      }
      if (compact) occupied = item.x + CHIP_GAP;
      else if (alignEnd) occupied = item.x;
      else occupied = item.x + Math.min(wanted, maxWidth);
      return {
        marker: item.marker,
        time: item.time,
        left: Math.min(Math.max(ratio(item.time), 0), 100),
        alignEnd,
        compact,
        maxWidth,
        muted: outside(item.time),
      };
    });
  });

  function chipLabel(chip: Pick<Chip, 'marker' | 'time'>): string {
    return markerDrag?.id === chip.marker.id && markerDrag.moved ? formatClock(chip.time) : chip.marker.label;
  }

  export async function addMarkerAt(value: number) {
    if (!markers.available) return;
    markers.add(Math.round(clampTime(value) * 1000) / 1000);
  }

  function onLaneDblClick(event: MouseEvent) {
    if (event.target !== lane) return;
    void addMarkerAt(timeAt(lane, event.clientX));
  }

  function onChipDown(marker: TimelineMarker, event: PointerEvent) {
    if (event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    markerDrag = {
      id: marker.id,
      pointerId: event.pointerId,
      startX: event.clientX,
      origin: marker.time,
      time: marker.time,
      moved: false,
    };
  }

  function onChipMove(event: PointerEvent) {
    const drag = markerDrag;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const dx = event.clientX - drag.startX;
    if (!drag.moved && Math.abs(dx) < 4) return;
    const width = Math.max(laneWidth, 1);
    markerDrag = {
      ...drag,
      moved: true,
      time: Math.round(clampTime(drag.origin + (dx / width) * span) * 1000) / 1000,
    };
  }

  function onChipUp(marker: TimelineMarker, event: PointerEvent) {
    const drag = markerDrag;
    if (!drag || drag.pointerId !== event.pointerId) return;
    markerDrag = null;
    if (drag.moved) markers.move(marker.id, drag.time);
    else playback.seek(marker.time);
  }

  /** 鍵盤觸發的 click（detail 為 0）才在這裡跳轉；滑鼠點擊由 pointerup 處理。 */
  function onChipClick(marker: TimelineMarker, event: MouseEvent) {
    if (event.detail === 0) playback.seek(marker.time);
  }

  function onChipKey(marker: TimelineMarker, event: KeyboardEvent) {
    if (event.key === 'F2') {
      event.preventDefault();
      markers.editingId = marker.id;
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      focusNeighbour(marker);
      markers.remove(marker.id);
    } else if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
      event.preventDefault();
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
      menu = { id: marker.id, x: rect.left, y: rect.bottom + 4 };
    }
  }

  function focusNeighbour(marker: TimelineMarker) {
    const list = markers.items;
    const index = list.findIndex((item) => item.id === marker.id);
    const neighbour = list[index + 1] ?? list[index - 1];
    void nextTick().then(() => {
      if (neighbour) focusChip(neighbour.id);
    });
  }

  function focusChip(id: string) {
    lane?.querySelector<HTMLElement>(`[data-marker-id="${CSS.escape(id)}"]`)?.focus();
  }

  // 進入改名：帶入目前名稱並全選。
  let editingMarker = $derived(markers.items.find((item) => item.id === markers.editingId) ?? null);
  $effect(() => {
    const id = markers.editingId;
    if (!id) return;
    const target = markers.items.find((item) => item.id === id);
    if (!target) {
      markers.editingId = null;
      return;
    }
    draftLabel = target.label;
    void nextTick().then(() => {
      editor?.focus();
      editor?.select();
    });
  });

  function finishEdit(save: boolean) {
    const id = markers.editingId;
    if (!id) return;
    if (save) markers.rename(id, draftLabel);
    markers.editingId = null;
    void nextTick().then(() => focusChip(id));
  }

  function onEditorKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      finishEdit(true);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      finishEdit(false);
    }
  }

  function onEditorFocusOut(event: FocusEvent) {
    const form = event.currentTarget as HTMLElement;
    if (event.relatedTarget instanceof Node && form.contains(event.relatedTarget)) return;
    if (markers.editingId) finishEdit(true);
  }

  const editorLeft = $derived.by(() => {
    if (!editingMarker) return 0;
    const width = Math.max(laneWidth, 1);
    const x = (Math.min(Math.max(ratio(editingMarker.time), 0), 100) / 100) * width;
    return Math.min(Math.max(x - 120, -8), width - 240 + 8);
  });

  // ---- 右鍵選單 ----

  let menu = $state<{ id: string; x: number; y: number } | null>(null);
  const menuMarker = $derived(menu ? (markers.items.find((item) => item.id === menu?.id) ?? null) : null);

  const MENU_ITEMS: MenuItem[] = [
    { id: 'seek', label: '跳到這裡', icon: 'play' },
    { id: 'rename', label: '重新命名', icon: 'pencil' },
    { id: 'start', label: '設為片段起點', icon: 'to-start' },
    { id: 'end', label: '設為片段終點', icon: 'to-end' },
    { id: 'delete', label: '刪除標籤', icon: 'trash', danger: true },
  ];

  function onChipContext(marker: TimelineMarker, event: MouseEvent) {
    event.preventDefault();
    menu = { id: marker.id, x: event.clientX, y: event.clientY };
  }

  function selectMenu(action: string) {
    const marker = menuMarker;
    menu = null;
    if (!marker) return;
    if (action === 'seek') playback.seek(marker.time);
    else if (action === 'rename') markers.editingId = marker.id;
    else if (action === 'start') setStart(marker.time);
    else if (action === 'end') setEnd(marker.time);
    else if (action === 'delete') markers.remove(marker.id);
  }

  function closeMenu(focusBack: boolean) {
    const id = menu?.id;
    menu = null;
    if (focusBack && id) void nextTick().then(() => focusChip(id));
  }
</script>

<div class="transport">
  <div class="row row-wrap">
    <button class="btn btn--primary" onclick={() => playback.toggle()} disabled={!ready}>
      <Icon name={playback.playing ? 'pause' : 'play'} />
      {playback.playing ? '暫停' : '播放'}
    </button>
    <button class="btn" onclick={() => playback.reset()} disabled={!ready}><Icon name="skip-back" />重置</button>
    <button
      class="btn btn--icon"
      onclick={() => playback.nudge(-0.1)}
      disabled={!ready}
      aria-label="後退 0.1 秒"><Icon name="chevron-left" />0.1s</button
    >
    <button
      class="btn btn--icon"
      onclick={() => playback.nudge(0.1)}
      disabled={!ready}
      aria-label="前進 0.1 秒">0.1s<Icon name="chevron-right" /></button
    >

    <span class="clock mono">{formatClock(time)}</span>
    <span class="muted small mono total">／ {formatClock(bounds.end)}</span>

    <button
      class="btn"
      onclick={() => addMarkerAt(time)}
      disabled={!markers.available}
      title={markers.available
        ? '在目前時間加標籤（M）；也可以在時間軸上方的空白處雙擊'
        : '開啟譜面紀錄並生成結果後才能加標籤'}
    >
      <Icon name="bookmark-plus" />標籤
    </button>

    <span class="spacer"></span>

    <div class="row" role="group" aria-label="播放倍率">
      {#each RATES as rate (rate)}
        <button
          class="btn btn--icon"
          class:is-active={playback.rate === rate}
          aria-pressed={playback.rate === rate}
          onclick={() => playback.setRate(rate)}>{rate}×</button
        >
      {/each}
    </div>
  </div>

  <!-- 三條橫向軌道共用同一條座標軸：標籤列、時間軸標記層、片段把手列都左右內縮半個滑鈕寬度，
       正好等於滑鈕中心的移動範圍，因此標籤、大頭針與把手永遠對齊。 -->
  <div class="timeline" class:is-dimmed={dimOutside}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="marker-lane"
      class:is-available={markers.available}
      bind:this={lane}
      bind:clientWidth={laneWidth}
      ondblclick={onLaneDblClick}
      title={markers.available && markers.items.length === 0 ? '雙擊加標籤' : undefined}
    >
      {#if markers.available && markers.items.length === 0}
        <span class="lane-hint" aria-hidden="true">雙擊這裡或按 M 加標籤</span>
      {/if}
      {#each chips as chip (chip.marker.id)}
        <button
          class="marker"
          class:is-end={chip.alignEnd}
          class:is-compact={chip.compact}
          class:is-muted={chip.muted}
          class:is-editing={markers.editingId === chip.marker.id}
          class:is-dragging={markerDrag?.id === chip.marker.id && markerDrag.moved}
          style={`left:${chip.left}%;max-width:${chip.maxWidth}px`}
          data-marker-id={chip.marker.id}
          aria-label={`標籤 ${chip.marker.label}，${formatClock(chip.marker.time)}。Enter 跳轉，F2 改名，Delete 刪除`}
          title={`${chip.marker.label}（${formatClock(chip.marker.time)}）\n點擊跳轉・拖曳移動・雙擊改名・右鍵更多`}
          onpointerdown={(event) => onChipDown(chip.marker, event)}
          onpointermove={onChipMove}
          onpointerup={(event) => onChipUp(chip.marker, event)}
          onpointercancel={() => (markerDrag = null)}
          onclick={(event) => onChipClick(chip.marker, event)}
          ondblclick={(event) => {
            event.stopPropagation();
            markers.editingId = chip.marker.id;
          }}
          oncontextmenu={(event) => onChipContext(chip.marker, event)}
          onkeydown={(event) => onChipKey(chip.marker, event)}
        >
          {#if !chip.compact}<span class="marker-text">{chipLabel(chip)}</span>{/if}
        </button>
      {/each}

      {#if editingMarker}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="marker-editor" style={`left:${editorLeft}px`} onfocusout={onEditorFocusOut}>
          <span class="mono xsmall muted editor-time">{formatClock(editingMarker.time)}</span>
          <input
            bind:this={editor}
            class="input editor-input"
            type="text"
            maxlength="40"
            aria-label="標籤名稱"
            bind:value={draftLabel}
            onkeydown={onEditorKey}
          />
          <button class="btn btn--icon" onclick={() => finishEdit(true)} aria-label="完成" title="完成（Enter）">
            <Icon name="check" />
          </button>
          <button
            class="btn btn--icon editor-delete"
            onclick={() => {
              const id = markers.editingId;
              markers.editingId = null;
              if (id) markers.remove(id);
            }}
            aria-label="刪除標籤"
            title="刪除標籤"
          >
            <Icon name="trash" />
          </button>
        </div>
      {/if}
    </div>

    <div class="track">
      <div class="track-bg" aria-hidden="true"></div>
      <div class="marks" aria-hidden="true">
        {#if dimOutside}
          <div class="outside outside--before" style={`width:calc(${ratio(playback.loopStart)}% + var(--thumb) / 2)`}></div>
          <div class="outside outside--after" style={`width:calc(${100 - ratio(playback.loopEnd)}% + var(--thumb) / 2)`}></div>
        {/if}
        {#if bounds.start < 0}
          <div class="zero-mark" style={`left:${ratio(0)}%`}></div>
        {/if}
        {#each chips as chip (chip.marker.id)}
          <div class="marker-line" class:is-muted={chip.muted} style={`left:${chip.left}%`}></div>
        {/each}
        {#each ticks as item (item.id)}
          <div
            class="tick"
            class:tick--left={item.hand === 'L'}
            class:tick--right={item.hand === 'R'}
            class:tick--both={item.hand === 'B'}
            class:tick--slide={item.kind === 'slide'}
            class:tick--hold={item.kind === 'hold' || item.kind === 'touchHold'}
            class:tick--touch={item.kind === 'touch' || item.kind === 'touchHold'}
            class:tick--muted={item.muted}
            style={`left:${item.left}%`}
          ></div>
        {/each}
        {#each palmBands as band (band.id)}
          <div
            class="palm-band"
            class:palm-band--left={band.hand === 'L'}
            class:is-muted={band.muted}
            style={`left:${band.left}%;width:${band.width}%`}
          ></div>
        {/each}
        {#each handoverTicks as handover (handover.id)}
          <div
            class="handover-band"
            class:is-muted={handover.muted}
            style={`left:${handover.left}%;width:${handover.width}%`}
          ></div>
        {/each}
        {#if playback.loopEnabled && !playback.rangeIsFull}
          <div class="range-edge" style={`left:${ratio(playback.loopStart)}%`}></div>
          <div class="range-edge" style={`left:${ratio(playback.loopEnd)}%`}></div>
        {/if}
        <div class="playhead" style={`left:${ratio(time)}%`}></div>
      </div>

      <input
        class="seek"
        type="range"
        min={bounds.start}
        max={bounds.end}
        step="0.001"
        value={time}
        disabled={!ready}
        aria-label="播放時間（秒）"
        oninput={(event) => playback.seek(Number(event.currentTarget.value))}
      />
    </div>

    <div class="range-lane" class:is-active={playback.loopEnabled} bind:this={rangeLane}>
      {#if ready}
        <div
          class="range-bar"
          style={`left:${ratio(playback.loopStart)}%;width:${Math.max(0, ratio(playback.loopEnd) - ratio(playback.loopStart))}%`}
        ></div>
        {#each [{ which: 'start', value: playback.loopStart, label: '片段起點' }, { which: 'end', value: playback.loopEnd, label: '片段終點' }] as handle (handle.which)}
          {@const which = handle.which as 'start' | 'end'}
          <div
            class="range-handle"
            class:range-handle--end={which === 'end'}
            class:is-dragging={rangeDrag?.which === which}
            style={`left:${ratio(handle.value)}%`}
            role="slider"
            tabindex="0"
            aria-label={handle.label}
            aria-valuemin={bounds.start}
            aria-valuemax={bounds.end}
            aria-valuenow={handle.value}
            aria-valuetext={formatClock(handle.value)}
            title={`${handle.label} ${formatClock(handle.value)}：拖曳調整，方向鍵微調（Shift 一秒）`}
            onpointerdown={(event) => onHandleDown(which, event)}
            onpointermove={onHandleMove}
            onpointerup={onHandleUp}
            onpointercancel={() => (rangeDrag = null)}
            onkeydown={(event) => onHandleKey(which, event)}
          ></div>
        {/each}
      {/if}
    </div>
  </div>

  <div class="loop-row small">
    <div class="row row-wrap loop-controls">
      <label class="check">
        <input
          type="checkbox"
          checked={playback.loopEnabled}
          disabled={!ready}
          onchange={(event) => playback.setLoop(event.currentTarget.checked)}
        />
        <span>循環片段</span>
      </label>
      <button class="btn btn--icon" onclick={() => setStart()} disabled={!ready} title="目前時間設為片段起點（I）">
        <Icon name="to-start" />設為起點
      </button>
      <button class="btn btn--icon" onclick={() => setEnd()} disabled={!ready} title="目前時間設為片段終點（O）">
        <Icon name="to-end" />設為終點
      </button>
      <span class="mono xsmall range-text" class:muted={!playback.loopEnabled}>
        {formatClock(playback.loopStart)} – {formatClock(playback.loopEnd)}
      </span>
      <button
        class="btn btn--icon"
        onclick={() => playback.clearLoopRange()}
        disabled={!ready || (playback.rangeIsFull && !playback.loopEnabled)}
        title="片段回到整段並關閉循環"
      >
        <Icon name="move-horizontal" />整段
      </button>
    </div>

    <!-- 寬度固定：播放時狀態文字一直變，版面不能跟著換行跳動。 -->
    <div class="hands" aria-live="off">
      {#if session.hasHands}
        <span class="hand-state">
          <span class="badge badge--left">L</span>
          <span class="state-text" title={handText(leftState)}>{handText(leftState)}</span>
        </span>
        <span class="hand-state">
          <span class="badge badge--right">R</span>
          <span class="state-text" title={handText(rightState)}>{handText(rightState)}</span>
        </span>
      {:else}
        <span class="muted xsmall hands-empty">沒有可播放的方案</span>
      {/if}
    </div>
  </div>

  <div class="row row-wrap xsmall muted legend">
    <span><span class="swatch swatch--left"></span>左手 L</span>
    <span><span class="swatch swatch--right"></span>右手 R</span>
    <span><span class="swatch swatch--accent"></span>換手</span>
    {#if session.hasPalms}
      <span>時間軸上緣細條：該手的手掌覆蓋區間</span>
    {/if}
    {#if markers.items.length > 0}
      <span>標籤：點擊跳轉・拖曳移動・雙擊改名・右鍵更多・[ ] 切換</span>
    {/if}
  </div>
</div>

{#if menu && menuMarker}
  <ContextMenu
    items={MENU_ITEMS}
    x={menu.x}
    y={menu.y}
    label={`標籤 ${menuMarker.label} 的動作`}
    onSelect={selectMenu}
    onClose={closeMenu}
  />
{/if}

<style>
  .transport {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
  }

  /* 時間與手部狀態都保留固定寬度：內容變動時播放列的換行數不變，
     盤面高度因此穩定，不會在切換範例或播放時被重新縮放。 */
  .clock {
    min-width: 9ch;
    font-size: var(--fs-lg);
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .total {
    min-width: 9ch;
  }

  .timeline {
    --thumb: 14px;
    --inset: calc(var(--thumb) / 2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* ---- 標籤列 ---- */

  .marker-lane {
    position: relative;
    height: 20px;
    margin: 0 var(--inset);
  }

  .marker-lane.is-available {
    cursor: copy;
  }

  .lane-hint {
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    font-size: var(--fs-xs);
    color: var(--c-text-dim);
    pointer-events: none;
    opacity: 0;
    transition: opacity 120ms ease;
  }

  .marker-lane.is-available:hover .lane-hint {
    opacity: 1;
  }

  /* 標籤貼在時間點的右側（靠近右端改貼左側），左下角的直角正好對準下方的標記線。 */
  .marker {
    position: absolute;
    bottom: 0;
    display: flex;
    align-items: center;
    max-width: 132px;
    height: 18px;
    padding: 0 6px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--c-text);
    background: var(--c-control-hover);
    border: 1px solid var(--c-border-strong);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    cursor: grab;
    touch-action: none;
    user-select: none;
  }

  .marker.is-end {
    transform: translateX(-100%);
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }

  .marker-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .marker:hover,
  .marker:focus-visible,
  .marker.is-editing,
  .marker.is-dragging {
    z-index: 2;
    color: var(--c-on-invert);
    background: var(--c-text);
    border-color: var(--c-text);
  }

  .marker:focus-visible {
    outline-offset: 1px;
  }

  .marker.is-dragging {
    cursor: grabbing;
    font-family: var(--font-mono);
  }

  /* 太擠時只留一根短旗桿，滑過或聚焦時才展開名稱（title 提示）。 */
  .marker.is-compact {
    width: 6px;
    min-width: 6px;
    padding: 0;
    margin-left: -3px;
    background: var(--c-text-dim);
    border-color: var(--c-text-dim);
    border-radius: var(--radius-sm);
  }

  .marker.is-compact.is-end {
    transform: none;
  }

  .marker.is-muted:not(:hover):not(:focus-visible):not(.is-editing) {
    color: var(--c-text-dim);
    background: var(--c-control);
    border-color: var(--c-border);
  }

  .marker-editor {
    position: absolute;
    bottom: calc(100% + 6px);
    z-index: 10;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 240px;
    padding: var(--space-1);
    background: var(--c-control);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
    box-shadow: 0 12px 32px #000000;
    cursor: default;
  }

  .editor-time {
    padding: 0 var(--space-1);
  }

  .editor-input {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 28px;
  }

  .editor-delete:hover:not(:disabled) {
    color: var(--c-danger);
  }

  /* ---- 時間軸 ---- */

  .track {
    position: relative;
    height: 26px;
  }

  .track-bg {
    position: absolute;
    inset: 0;
    background: var(--c-control);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    pointer-events: none;
  }

  .marks {
    position: absolute;
    top: 1px;
    bottom: 1px;
    left: var(--inset);
    right: var(--inset);
    pointer-events: none;
  }

  /* 片段外的時間軸：不用半透明遮罩，直接換成最深的底色，標記同時改灰。 */
  .outside {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--c-bg);
  }

  .outside--before {
    left: calc(-1 * var(--inset));
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
  }

  .outside--after {
    right: calc(-1 * var(--inset));
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }

  .range-edge {
    position: absolute;
    top: -1px;
    bottom: -1px;
    width: 2px;
    margin-left: -1px;
    background: var(--c-text-dim);
  }

  .marker-line {
    position: absolute;
    top: -1px;
    bottom: -1px;
    width: 1px;
    background: var(--c-border-strong);
  }

  .marker-line.is-muted {
    background: var(--c-border);
  }

  .tick {
    position: absolute;
    top: 5px;
    width: 3px;
    height: 8px;
    margin-left: -1px;
    background: var(--c-text-dim);
  }

  .tick--left {
    background: var(--c-left);
  }

  .tick--right {
    background: var(--c-right);
  }

  .tick--both {
    background: var(--c-accent);
  }

  .tick--slide {
    height: 14px;
  }

  .tick--hold {
    height: 12px;
    width: 5px;
  }

  .tick--touch {
    top: 9px;
  }

  .tick.tick--muted {
    background: var(--c-border-strong);
  }

  /* 手掌覆蓋區間貼在時間軸上緣；音符標記從 top:5px 起，兩者不會疊在一起。 */
  .palm-band {
    position: absolute;
    top: 1px;
    height: 3px;
    min-width: 3px;
    background: var(--c-right);
  }

  .palm-band--left {
    background: var(--c-left);
  }

  .handover-band {
    position: absolute;
    bottom: 3px;
    height: 5px;
    min-width: 3px;
    background: var(--c-accent);
  }

  .palm-band.is-muted,
  .handover-band.is-muted {
    background: var(--c-border-strong);
  }

  .zero-mark {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--c-border-strong);
  }

  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--c-text);
  }

  /* 滑桿疊在標記層上，軌道透明，只留下大頭針。 */
  .seek {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    appearance: none;
    -webkit-appearance: none;
    background: transparent;
  }

  .seek::-webkit-slider-runnable-track {
    height: 100%;
    background: transparent;
  }

  .seek::-webkit-slider-thumb {
    appearance: none;
    -webkit-appearance: none;
    width: var(--thumb);
    height: var(--thumb);
    border: 2px solid var(--c-bg);
    border-radius: 50%;
    background: var(--c-text);
  }

  .seek:disabled::-webkit-slider-thumb {
    background: var(--c-border-strong);
  }

  .seek::-moz-range-track {
    height: 100%;
    background: transparent;
  }

  .seek::-moz-range-thumb {
    width: var(--thumb);
    height: var(--thumb);
    border: 2px solid var(--c-bg);
    border-radius: 50%;
    background: var(--c-text);
  }

  /* ---- 片段把手列 ---- */

  .range-lane {
    position: relative;
    height: 14px;
    margin: 0 var(--inset);
  }

  .range-bar {
    position: absolute;
    top: 2px;
    height: 3px;
    background: var(--c-border-strong);
    pointer-events: none;
  }

  .range-lane.is-active .range-bar {
    background: var(--c-text-dim);
  }

  /* 把手做成朝時間軸的直角括號：起點開口向右、終點開口向左，不靠顏色也分得出方向。 */
  .range-handle {
    position: absolute;
    top: 0;
    width: 10px;
    height: 14px;
    border: 3px solid var(--c-border-strong);
    border-right: none;
    border-top-width: 3px;
    border-bottom: none;
    border-radius: 2px 0 0 0;
    cursor: ew-resize;
    touch-action: none;
  }

  .range-handle--end {
    margin-left: -10px;
    border-left: none;
    border-right: 3px solid var(--c-border-strong);
    border-radius: 0 2px 0 0;
  }

  .range-handle:not(.range-handle--end) {
    margin-left: -1px;
  }

  .range-lane.is-active .range-handle {
    border-color: var(--c-text);
  }

  .range-handle:hover,
  .range-handle:focus-visible,
  .range-handle.is-dragging {
    border-color: var(--c-focus);
  }

  .range-handle:focus-visible {
    outline-offset: 1px;
  }

  /* ---- 下方列 ---- */

  .loop-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-3);
  }

  .loop-controls {
    min-width: 0;
  }

  .range-text {
    min-width: 20ch;
  }

  .hands {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-3);
    flex: none;
    width: 30ch;
    margin-left: auto;
  }

  .hand-state {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
    flex: 1 1 0;
  }

  .state-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
  }

  .hands-empty {
    white-space: nowrap;
  }

  .legend {
    gap: var(--space-3);
  }

  .swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    margin-right: 4px;
    vertical-align: middle;
    border-radius: 999px;
  }

  .swatch--left {
    background: var(--c-left);
  }

  .swatch--right {
    background: var(--c-right);
  }

  .swatch--accent {
    background: var(--c-accent);
  }

  @media (prefers-reduced-motion: reduce) {
    .lane-hint {
      transition: none;
    }
  }
</style>
