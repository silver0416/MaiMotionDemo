<script lang="ts">
  import Icon from '$app/components/Icon.svelte';
  import { formatClock } from '$app/lib/format';
  import { bounce } from '$app/lib/press';
  import Keycap from './Keycap.svelte';
  import MiniDisc, { type DiscMark } from './MiniDisc.svelte';
  import { DemoPlayer, userAction } from './player.svelte';
  import Stage from './Stage.svelte';

  /**
   * 示意：逐顆標註。左邊是編輯區與一小段清單（樣子照 AnnotatePanel），右邊是極簡盤面與鍵帽提示。
   * 自動示範：選第一顆 → A／D 標手、自動跳下一步 → Slide 起點與滑行各標一次 → 回到滑行按兩次 S 切成「皆可」→ N → D。
   * 之後可以點清單、點鍵帽，或在演示裡按 A D S N（Shift+A／D 整顆同一手、↑↓ 換步、Delete 清除）。
   * 清單與標註都是示意，不是任何譜面的資料。文字對照 annotate/basics.md。
   */

  type Hand = 'L' | 'R';
  type Confidence = 'sure' | 'unsure' | 'either';
  type Part = 'one' | 'hand' | 'track';

  interface Row {
    key: string;
    num: number;
    part: Part;
    time: number;
    what: string;
    button: number;
    slide?: { from: number; to: number };
  }

  const ROWS: Row[] = [
    { key: '1', num: 1, part: 'one', time: 0, what: 'Tap 1', button: 1 },
    { key: '2', num: 2, part: 'one', time: 0.5, what: 'Tap 8', button: 8 },
    { key: '3', num: 3, part: 'one', time: 1, what: 'Tap 2', button: 2 },
    { key: '4h', num: 4, part: 'hand', time: 1.5, what: 'Slide 7-3 起點', button: 7, slide: { from: 7, to: 3 } },
    { key: '4t', num: 4, part: 'track', time: 2, what: 'Slide 7-3 滑行', button: 7, slide: { from: 7, to: 3 } },
    { key: '5', num: 5, part: 'one', time: 2.5, what: 'Tap 6', button: 6 },
    { key: '6', num: 6, part: 'one', time: 3, what: 'Tap 4', button: 4 },
  ];

  const CONFIDENCE_LABEL: Record<Confidence, string> = { sure: '確定', unsure: '不確定', either: '皆可' };
  const CONFIDENCES: Confidence[] = ['sure', 'unsure', 'either'];
  const PARTS = ['hand', 'track'] as const;
  const HANDS: Hand[] = ['L', 'R'];

  let current = $state<number | null>(null);
  let marks = $state<Record<string, { hand?: Hand; conf: Confidence }>>({});
  let autoAdvance = $state(true);
  let listEl = $state<HTMLElement | null>(null);
  let advanceTimer: ReturnType<typeof setTimeout> | undefined;

  const row = $derived(current === null ? null : ROWS[current]);
  const split = $derived(row !== null && row.part !== 'one');
  const mark = $derived(row ? marks[row.key] : undefined);

  function reset() {
    clearTimeout(advanceTimer);
    current = null;
    marks = {};
    autoAdvance = true;
  }

  function select(index: number) {
    clearTimeout(advanceTimer);
    current = Math.max(0, Math.min(ROWS.length - 1, index));
  }

  /** 標完自動跳到下一步：等一下下再跳，看得到這一列先被標上。 */
  function advanceLater() {
    if (!autoAdvance || current === null || current >= ROWS.length - 1) return;
    const from = current;
    const go = () => {
      if (current === from) current = from + 1;
    };
    clearTimeout(advanceTimer);
    if (player.skipping) go();
    else advanceTimer = setTimeout(go, 450);
  }

  function setHand(hand: Hand, whole = false) {
    if (current === null) return;
    const target = ROWS[current];
    const keys = whole && target.part !== 'one' ? ROWS.filter((item) => item.num === target.num).map((item) => item.key) : [target.key];
    for (const key of keys) marks[key] = { conf: marks[key]?.conf ?? 'sure', hand };
    if (whole && target.part === 'hand') current += 1;
    advanceLater();
  }

  function cycleConfidence() {
    if (!row) return;
    const now = marks[row.key]?.conf ?? 'sure';
    const next = CONFIDENCES[(CONFIDENCES.indexOf(now) + 1) % CONFIDENCES.length];
    marks[row.key] = { ...marks[row.key], conf: next };
  }

  function setConfidence(value: Confidence) {
    if (!row) return;
    marks[row.key] = { ...marks[row.key], conf: value };
  }

  function nextUnconfirmed(): boolean {
    const start = current ?? -1;
    for (let offset = 1; offset <= ROWS.length; offset += 1) {
      const index = (start + offset) % ROWS.length;
      if (!marks[ROWS[index].key]?.hand) {
        select(index);
        return true;
      }
    }
    return false;
  }

  function clear() {
    if (!row) return;
    delete marks[row.key];
  }

  const handName = (hand: Hand) => (hand === 'L' ? '左手' : '右手');

  // ---- 使用者操作（player 在下面才建立，按下時才取用） ----

  const act =
    <A extends unknown[]>(fn: (...args: A) => void) =>
    (...args: A) =>
      userAction(player, fn)(...args);

  const userSelect = act((index: number) => {
    select(index);
    player.say = `選到第 ${ROWS[index].part === 'track' ? `↳${ROWS[index].num}` : ROWS[index].num} 顆：${ROWS[index].what}。`;
  });
  const userHand = act((hand: Hand, whole?: boolean) => {
    if (current === null) {
      player.say = '先在清單點一顆音符。';
      return;
    }
    player.flash(hand === 'L' ? 'A' : 'D');
    const what = ROWS[current].what;
    setHand(hand, whole);
    player.say = `${what}：${whole ? '整顆' : ''}${handName(hand)}。`;
  });
  const userConfidence = act((value?: Confidence) => {
    if (!row) {
      player.say = '先在清單點一顆音符。';
      return;
    }
    if (value) setConfidence(value);
    else {
      player.flash('S');
      cycleConfidence();
    }
    player.say = `${row.what}：信心「${CONFIDENCE_LABEL[marks[row.key].conf]}」。`;
  });
  const userNext = act(() => {
    player.flash('N');
    player.say = nextUnconfirmed() && row ? `跳到下一步還沒確認的：${row.what}。` : '全部都標好了。';
  });
  const userClear = act(() => {
    if (!row) return;
    clear();
    player.say = `清除 ${row.what} 的標註。`;
  });
  const userAuto = act((on: boolean) => {
    autoAdvance = on;
    player.say = on ? '標完會自動跳到下一步。' : '標完停在這一步。';
  });

  const rowEl = (key: string) => listEl?.querySelector(`[data-row="${key}"]`);

  const player = new DemoPlayer({
    intro: '逐顆標註：選一顆，按 A 或 D。',
    outro: '示範結束。換你試試：點清單或鍵帽，或在這裡按 A、D、S、N。',
    reset,
    steps: [
      { say: '在清單點第一顆：Tap 1。', target: () => rowEl('1'), run: () => select(0), hold: 1100 },
      { say: '按 A：標成左手，這一列出現 L。', key: 'A', run: () => setHand('L'), hold: 1400 },
      { say: '標完自動跳到下一步。按 D：右手。', key: 'D', run: () => setHand('R'), hold: 1400 },
      { say: 'Tap 2：按 A，左手。', key: 'A', run: () => setHand('L'), hold: 1300 },
      { say: '有起點的 Slide 分成兩步：先標「起點」，也就是拍下星星的手。', hold: 1700 },
      { say: '按 A：起點用左手。', key: 'A', run: () => setHand('L'), hold: 1300 },
      { say: '接著是「滑行」這一步：按 D，開始滑的是右手。', key: 'D', run: () => setHand('R'), hold: 1600 },
      { say: 'Tap 6：按 A，左手。', key: 'A', run: () => setHand('L'), hold: 1300 },
      { say: '點回滑行那一列（↳4）。', target: () => rowEl('4t'), run: () => select(4), hold: 1100 },
      { say: '按 S 切換這一步的信心：確定 → 不確定。', key: 'S', run: cycleConfidence, hold: 1200 },
      { say: '再按一次 S：切成「皆可」，表示兩隻手都常見。', key: 'S', run: cycleConfidence, hold: 1700 },
      { say: '按 N：跳到下一步還沒確認的 Tap 4。', key: 'N', run: () => nextUnconfirmed(), hold: 1300 },
      { say: '按 D：右手。這一段都標完了。', key: 'D', run: () => setHand('R'), hold: 1500 },
    ],
  });

  function onkey(key: string, event: KeyboardEvent): boolean {
    switch (key.toLowerCase()) {
      case 'a':
        userHand('L', event.shiftKey);
        return true;
      case 'd':
        userHand('R', event.shiftKey);
        return true;
      case 's':
        userConfidence();
        return true;
      case 'n':
        userNext();
        return true;
      case 'delete':
      case 'backspace':
        userClear();
        return true;
      case 'arrowdown':
        userSelect(current === null ? 0 : Math.min(ROWS.length - 1, current + 1));
        return true;
      case 'arrowup':
        userSelect(current === null ? 0 : Math.max(0, current - 1));
        return true;
      default:
        return false;
    }
  }

  const discMarks = $derived.by<DiscMark[]>(() => {
    if (!row) return [];
    const list: DiscMark[] = [];
    if (mark?.hand) list.push({ button: row.button, tone: mark.hand === 'L' ? 'left' : 'right' });
    list.push({ button: row.button, tone: 'select' });
    return list;
  });

  const confParts = $derived(row?.part === 'hand' ? '起點信心' : row?.part === 'track' ? '滑行信心' : '信心');
  const stepMark = (part: Part) => (row ? marks[ROWS.find((item) => item.num === row.num && item.part === part)?.key ?? '']?.hand : undefined);
</script>

<Stage label="逐顆標註的按鍵" {player} {onkey}>
  <div class="annotate">
    <div class="col-main">
      <div class="editor">
        <div class="row">
          <button
            class="btn btn--icon"
            use:bounce={-1}
            onclick={() => userSelect(current === null ? 0 : current - 1)}
            aria-label="上一步"
          >
            <Icon name="chevron-left" size={14} />
          </button>
          <div class="note-title">
            {#if row}
              <span class="mono small">{formatClock(row.time)}</span>
              <span class="note-what">{row.what}</span>
            {:else}
              <span class="small muted">在清單點選音符</span>
            {/if}
          </div>
          <button
            class="btn btn--icon"
            use:bounce={1}
            onclick={() => userSelect(current === null ? 0 : current + 1)}
            aria-label="下一步"
          >
            <Icon name="chevron-right" size={14} />
          </button>
        </div>

        <!-- 固定三行高：一般音符兩行、Slide 三行，切換時下面的清單不跳動。 -->
        <div class="lines">
        {#if row && split}
          {#each PARTS as part (part)}
            {@const value = stepMark(part)}
            <div class="line" class:is-active={row.part === part}>
              <button
                class="line-label"
                onclick={() => userSelect(ROWS.findIndex((item) => item.num === row.num && item.part === part))}
              >
                {part === 'hand' ? '起點' : '滑行'}
              </button>
              <div class="choices" role="group" aria-label={part === 'hand' ? '起點觸碰的手' : '開始滑行的手'}>
                {#each HANDS as hand (hand)}
                  <button
                    class={['btn', 'choice', hand === 'L' ? 'left' : 'right']}
                    aria-pressed={value === hand}
                    onclick={() => {
                      userSelect(ROWS.findIndex((item) => item.num === row.num && item.part === part));
                      userHand(hand);
                    }}
                  >
                    {hand === 'L' ? '左 L' : '右 R'}
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        {:else}
          <div class="line">
            <span class="line-label">手</span>
            <div class="choices" role="group" aria-label="接觸的手">
              {#each HANDS as hand (hand)}
                <button
                  class={['btn', 'choice', hand === 'L' ? 'left' : 'right']}
                  aria-pressed={mark?.hand === hand}
                  disabled={!row}
                  onclick={() => userHand(hand)}
                >
                  {hand === 'L' ? '左 L' : '右 R'}
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="line" class:is-active={split}>
          <span class="line-label">{confParts}</span>
          <div class="choices" role="group" aria-label="信心程度">
            {#each CONFIDENCES as value (value)}
              <button
                class="btn choice"
                aria-pressed={!!row && (mark?.conf ?? (mark ? 'sure' : null)) === value}
                disabled={!row}
                onclick={() => userConfidence(value)}
              >
                {CONFIDENCE_LABEL[value]}
              </button>
            {/each}
          </div>
        </div>
        </div>

        <label class="check auto">
          <input type="checkbox" checked={autoAdvance} onchange={(event) => userAuto(event.currentTarget.checked)} />
          <span>標完自動跳到下一步</span>
        </label>
      </div>

      <div class="list" role="listbox" aria-label="音符標註清單" bind:this={listEl}>
        {#each ROWS as item, index (item.key)}
          {@const value = marks[item.key]}
          <button
            class="item"
            class:is-selected={current === index}
            role="option"
            aria-selected={current === index}
            data-row={item.key}
            onclick={() => userSelect(index)}
          >
            <span class="mono xsmall muted num">{item.part === 'track' ? `↳${item.num}` : item.num}</span>
            <span class="mono xsmall muted time">{formatClock(item.time)}</span>
            <span class="what small">{item.what}</span>
            <span class="hands small">
              {#if value?.conf && value.conf !== 'sure'}
                <span class="badge badge--quiet">{CONFIDENCE_LABEL[value.conf]}</span>
              {/if}
              {#if value?.hand}
                <span class={['badge', value.hand === 'L' ? 'badge--left' : 'badge--right']} aria-label={handName(value.hand)}>{value.hand}</span>
              {:else}
                <span class="muted">未標</span>
              {/if}
            </span>
          </button>
        {/each}
      </div>
    </div>

    <div class="col-side">
      <div class="disc">
        <MiniDisc
          label={row ? `盤面：目前是 ${row.what}` : '盤面'}
          marks={discMarks}
          slide={row?.slide ? { ...row.slide, on: row.part === 'track' } : null}
        />
      </div>
      <div class="keys" role="group" aria-label="快捷鍵">
        <Keycap cap="A" label="左手" lit={player.lit === 'A'} onclick={() => userHand('L')} />
        <Keycap cap="D" label="右手" lit={player.lit === 'D'} onclick={() => userHand('R')} />
        <Keycap cap="S" label="信心" lit={player.lit === 'S'} onclick={() => userConfidence()} />
        <Keycap cap="N" label="下一個未確認" lit={player.lit === 'N'} onclick={userNext} />
      </div>
    </div>
  </div>
</Stage>

<style>
  .annotate {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 11rem;
    gap: var(--space-4);
    align-items: start;
  }

  .col-main {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-width: 0;
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

  .note-what {
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .lines {
    --line-h: 40px;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-height: calc(var(--line-h) * 3 + var(--space-1) * 2);
  }

  @media (pointer: coarse) {
    .lines {
      --line-h: 52px;
    }
  }

  /* 編輯區的一行：同 AnnotatePanel（左邊 56px 標籤，目前這一步以淺底標出，不用左側色條）。 */
  .line {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr);
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-item);
  }

  .line.is-active {
    background: var(--c-control);
  }

  .line-label {
    padding: 0;
    background: none;
    border: none;
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
  }

  span.line-label {
    cursor: default;
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

  .choice.left[aria-pressed='true'],
  .choice.left[aria-pressed='true']:hover:not(:disabled) {
    background: var(--c-left);
    color: var(--c-on-hand);
  }

  .choice.right[aria-pressed='true'],
  .choice.right[aria-pressed='true']:hover:not(:disabled) {
    background: var(--c-right);
    color: var(--c-on-hand);
  }

  .auto {
    font-size: var(--fs-sm);
    padding-left: var(--space-2);
  }

  /* 清單：同 AnnotatePanel 的列（編號、時間、音符、手），列間 1px 分隔線。 */
  .list {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--c-border);
  }

  .item {
    display: grid;
    grid-template-columns: 2.25rem 4.5rem minmax(0, 1fr) auto;
    align-items: center;
    column-gap: var(--space-2);
    min-height: 32px;
    padding: 2px var(--space-2);
    background: none;
    border: none;
    border-bottom: 1px solid var(--c-border);
    color: var(--c-text);
    text-align: left;
    cursor: pointer;
    transition: background-color 120ms ease;
  }

  @media (hover: hover) {
    .item:hover {
      background: var(--c-control);
    }
  }

  .item.is-selected {
    background: var(--c-control-hover);
  }

  .item.is-selected .time {
    color: var(--c-accent);
  }

  .item:focus-visible {
    outline-offset: -2px;
  }

  .num {
    text-align: right;
  }

  .what {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hands {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    white-space: nowrap;
  }

  .col-side {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .disc {
    width: 100%;
  }

  .keys {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-1);
  }

  .keys :global(.keycap) {
    justify-content: flex-start;
  }

  @container demo (width < 560px) {
    .annotate {
      grid-template-columns: minmax(0, 1fr);
    }

    .col-side {
      order: -1;
      display: grid;
      grid-template-columns: 8.5rem minmax(0, 1fr);
      align-items: center;
    }
  }

  @media (pointer: coarse) {
    .item {
      min-height: 44px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .item {
      transition: none;
    }
  }
</style>
