<script lang="ts">
  import { fade } from 'svelte/transition';
  import { formatClock } from '$app/lib/format';
  import { reducedMotion } from '$app/lib/press';
  import Keycap from './Keycap.svelte';
  import { DemoPlayer, userAction } from './player.svelte';
  import Stage from './Stage.svelte';

  /**
   * 示意：打法分支。清單左側是分支圖（畫法同 App 的 LaneGraph：主線在最左、分支在右側並排，
   * 路線走到的線段用那條線的顏色，沒走到的是灰色），第 4 步分出「打法 B」、第 8 步之後併入主線。
   * 自動示範：看主線 → 分支出現 → 點灰線改走打法 B（第 4–8 步的手順跟著換）→ 按 B 來回切換。
   * 之後可以點灰線、點「路線」按鈕，或選一列後按 B／Shift+B。手順是示意，不是任何譜面的資料。
   * 文字對照 annotate/branches.md。
   */

  type Hand = 'L' | 'R';
  type Line = 'main' | 'b';

  interface Row {
    num: number;
    time: number;
    what: string;
    main: Hand;
    branch?: Hand;
  }

  /** 和 App 的 lib/annotation.ts LINE_COLORS 前兩個相同：主線、第一條分支。 */
  const LINE_COLOR: Record<Line, string> = { main: '#f0b429', b: '#5fc98a' };
  const LINE_NAME: Record<Line, string> = { main: '主線', b: '打法 B' };
  const FROM = 4;
  const TO = 8;

  const KEYS = [1, 8, 2, 7, 3, 6, 4, 5, 1, 8, 2, 7];
  const MAIN: Hand[] = ['L', 'R', 'L', 'R', 'L', 'R', 'L', 'R', 'L', 'R', 'L', 'R'];
  const BRANCH: Partial<Record<number, Hand>> = { 4: 'L', 5: 'R', 6: 'R', 7: 'L', 8: 'L' };

  const ROWS: Row[] = KEYS.map((button, index) => ({
    num: index + 1,
    time: index * 0.25,
    what: `Tap ${button}`,
    main: MAIN[index],
    branch: BRANCH[index + 1],
  }));

  /** 每條軌道 14px，兩條軌道。 */
  const LANE = 14;
  const WIDTH = LANE * 2 + 4;
  const x = (lane: number) => 2 + lane * LANE + LANE / 2;

  let revealed = $state(false);
  let route = $state<Line>('main');
  let selected = $state(5);
  let hover = $state<Line | null>(null);
  let listEl = $state<HTMLElement | null>(null);

  const inBranch = (num: number) => num >= FROM && num <= TO;
  const handOf = (item: Row): Hand => (route === 'b' && item.branch ? item.branch : item.main);
  const walked = (num: number): Line => (route === 'b' && inBranch(num) && revealed ? 'b' : 'main');

  interface Cell {
    line: Line;
    lane: number;
    kind: 'mid' | 'start' | 'end';
    on: boolean;
  }

  function cellsOf(num: number): Cell[] {
    const cells: Cell[] = [{ line: 'main', lane: 0, kind: 'mid', on: walked(num) === 'main' }];
    if (revealed && inBranch(num)) {
      cells.push({ line: 'b', lane: 1, kind: num === FROM ? 'start' : num === TO ? 'end' : 'mid', on: walked(num) === 'b' });
    }
    return cells;
  }

  // 座標：寬度用像素，高度 0～100 拉伸到列高（線寬不隨拉伸變粗）。分支從主線彎出、最後彎回主線。
  function path(cell: Cell): string {
    const lx = x(cell.lane);
    const mx = x(0);
    if (cell.kind === 'start') return `M ${mx} 0 C ${mx} 30, ${lx} 20, ${lx} 50 V 100`;
    if (cell.kind === 'end') return `M ${lx} 0 V 50 C ${lx} 80, ${mx} 70, ${mx} 100`;
    return `M ${lx} 0 V 100`;
  }

  const lit = (cell: Cell) => cell.on || hover === cell.line;

  function reset() {
    revealed = false;
    route = 'main';
    selected = 5;
    hover = null;
  }

  function pickLine(line: Line) {
    route = line;
  }

  const act =
    <A extends unknown[]>(fn: (...args: A) => void) =>
    (...args: A) =>
      userAction(player, fn)(...args);

  const userPickLine = act((line: Line) => {
    revealed = true;
    pickLine(line);
    player.say =
      line === 'b'
        ? `第 ${FROM}–${TO} 步改走打法 B，清單上的手順跟著換；範圍外照舊走主線。`
        : `第 ${FROM}–${TO} 步改回主線。`;
  });

  const userSelect = act((index: number) => {
    selected = index;
    const item = ROWS[index];
    player.say = inBranch(item.num) && revealed
      ? `第 ${item.num} 步：這一步有主線和打法 B 兩種，現在走${LINE_NAME[walked(item.num)]}。`
      : `第 ${item.num} 步：這一步只有主線。`;
  });

  const userCycle = act(() => {
    player.flash('B');
    const item = ROWS[selected];
    if (!revealed || !inBranch(item.num)) {
      player.say = `第 ${item.num} 步只有主線，沒有別的打法可以切換。`;
      return;
    }
    pickLine(route === 'b' ? 'main' : 'b');
    player.say = `第 ${FROM}–${TO} 步改走${LINE_NAME[route]}。`;
  });

  const player = new DemoPlayer({
    intro: '打法分支：同一段記兩種打法。',
    outro: '示範結束。換你試試：點灰色的線、點上方的路線按鈕，或選第 4–8 步的一列後按 B。',
    reset,
    steps: [
      { say: '這是主線：12 步都走主線，路線走到的線用主線的顏色。', hold: 1800 },
      {
        say: `第 ${FROM}–${TO} 步有人用另一種打法：從第 ${FROM} 步「分出」打法 B，第 ${TO} 步之後「併入」主線。`,
        run: () => (revealed = true),
        hold: 2600,
      },
      { say: '目前的路線走主線，所以打法 B 那一段是灰色的。', hold: 1800 },
      {
        say: '點灰色的線：這一段改走打法 B。',
        target: () => listEl?.querySelector('[data-row="6"] .hit--b'),
        run: () => pickLine('b'),
        hold: 1400,
      },
      { say: `第 ${FROM}–${TO} 步的手順換成打法 B 的內容；主線那一段變灰，範圍外照舊走主線。`, hold: 2400 },
      { say: '按 B：在這一步的打法之間切換，回到主線。', key: 'B', run: () => pickLine('main'), hold: 1700 },
      { say: '再按一次 B：又走打法 B。', key: 'B', run: () => pickLine('b'), hold: 1700 },
    ],
  });

  function onkey(key: string): boolean {
    if (key === 'b' || key === 'B') {
      userCycle();
      return true;
    }
    if (key === 'ArrowDown' || key === 'ArrowUp') {
      const next = Math.max(0, Math.min(ROWS.length - 1, selected + (key === 'ArrowDown' ? 1 : -1)));
      userSelect(next);
      requestAnimationFrame(() => listEl?.querySelector<HTMLElement>(`[data-row="${ROWS[next].num}"]`)?.focus());
      return true;
    }
    return false;
  }

  const motion = (ms: number) => (reducedMotion() ? 0 : ms);
  const lines: Line[] = ['main', 'b'];
</script>

<Stage label="打法分支與路線" {player} {onkey}>
  <div class="branches">
    <div class="route">
      <span class="route-label">路線</span>
      <div class="chips" role="group" aria-label="這一段走哪條線">
        {#each lines as line (line)}
          <button
            class="btn chip"
            aria-pressed={walked(FROM) === line && (revealed || line === 'main')}
            disabled={line === 'b' && !revealed}
            onclick={() => userPickLine(line)}
            onmouseenter={() => (hover = line)}
            onmouseleave={() => (hover = null)}
          >
            <span class="dot" style:background={LINE_COLOR[line]}></span>{LINE_NAME[line]}
          </button>
        {/each}
      </div>
      <span class="spacer"></span>
      <Keycap cap="B" label="切換打法" lit={player.lit === 'B'} onclick={userCycle} />
    </div>

    <div class="list" role="listbox" aria-label="逐顆標註清單（示意）" bind:this={listEl}>
      {#each ROWS as item, index (item.num)}
        {@const cells = cellsOf(item.num)}
        {@const hand = handOf(item)}
        {@const stop = walked(item.num)}
        <div
          class="item"
          class:is-selected={selected === index}
          class:is-branch={revealed && route === 'b' && inBranch(item.num)}
          role="option"
          tabindex={selected === index ? 0 : -1}
          aria-selected={selected === index}
          data-row={item.num}
          onclick={() => userSelect(index)}
          onkeydown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault();
              userSelect(index);
            }
          }}
        >
          <span class="lanes" aria-hidden="true">
            <svg viewBox={`0 0 ${WIDTH} 100`} preserveAspectRatio="none" width={WIDTH} height="100%">
              {#each [...cells].sort((a, b) => Number(lit(a)) - Number(lit(b))) as cell (cell.line)}
                <path
                  d={path(cell)}
                  class="line"
                  class:on={cell.on}
                  style:stroke={lit(cell) ? LINE_COLOR[cell.line] : undefined}
                  in:fade={{ duration: motion(cell.line === 'b' ? 400 : 0) }}
                />
              {/each}
              {#each cells as cell (cell.line)}
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <path
                  d={path(cell)}
                  class={`hit hit--${cell.line}`}
                  onclick={(event) => {
                    event.stopPropagation();
                    userPickLine(cell.line);
                  }}
                  onmouseenter={() => (hover = cell.line)}
                  onmouseleave={() => (hover = null)}
                >
                  <title>{cell.on ? LINE_NAME[cell.line] : `這一段改走「${LINE_NAME[cell.line]}」`}</title>
                </path>
              {/each}
            </svg>
            <span class="stop" style:left={`${x(stop === 'b' ? 1 : 0) - 4}px`} style:border-color={LINE_COLOR[stop]}></span>
          </span>
          <span class="tag xsmall">
            {#if revealed && item.num === FROM}分出{:else if revealed && item.num === TO}併入{/if}
          </span>
          <span class="mono xsmall muted num">{item.num}</span>
          <span class="mono xsmall muted time">{formatClock(item.time)}</span>
          <span class="what small">{item.what}</span>
          <span class="hands">
            {#key hand}
              <span
                class={['badge', hand === 'L' ? 'badge--left' : 'badge--right']}
                aria-label={hand === 'L' ? '左手' : '右手'}
                in:fade={{ duration: motion(item.branch && revealed ? 240 : 0) }}>{hand}</span
              >
            {/key}
          </span>
        </div>
      {/each}
    </div>
  </div>
</Stage>

<style>
  .branches {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .route {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .route-label {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .chip {
    gap: var(--space-1);
    padding: 0 var(--space-2);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .list {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--c-border);
  }

  /* 清單列：分支圖撐滿整列高度（往下多畫 1px 蓋過分隔線），右邊是編號、時間、音符、手。 */
  .item {
    position: relative;
    display: grid;
    grid-template-columns: 2.25rem 1.5rem 4.5rem minmax(0, 1fr) auto;
    align-items: center;
    column-gap: var(--space-2);
    height: 30px;
    padding: 0 var(--space-2) 0 calc(var(--space-1) + 40px);
    border-bottom: 1px solid var(--c-border);
    color: var(--c-text);
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

  .item:focus-visible {
    outline-offset: -2px;
  }

  /* 分支圖疊在列的左側，撐滿整列並往下多畫 1px 蓋過分隔線（同 LaneGraph）。 */
  .lanes {
    position: absolute;
    top: 0;
    bottom: -1px;
    left: var(--space-1);
    width: 32px;
  }

  svg {
    display: block;
    overflow: visible;
  }

  .line {
    fill: none;
    stroke: var(--c-border-strong);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
    transition: stroke 160ms ease;
  }

  .line.on {
    stroke-width: 3;
  }

  .hit {
    fill: none;
    stroke: transparent;
    stroke-width: 12;
    vector-effect: non-scaling-stroke;
    pointer-events: stroke;
    cursor: pointer;
  }

  .stop {
    position: absolute;
    top: 50%;
    width: 8px;
    height: 8px;
    margin-top: -4px;
    border: 2px solid;
    border-radius: 50%;
    background: var(--c-panel);
    pointer-events: none;
  }

  .tag {
    color: var(--c-text-strong);
    font-weight: 600;
    white-space: nowrap;
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
    justify-content: flex-end;
    min-width: 1.75rem;
  }

  @container demo (width < 420px) {
    .item {
      grid-template-columns: 2.25rem 1.5rem minmax(0, 1fr) auto;
    }

    .time {
      display: none;
    }
  }

  @media (pointer: coarse) {
    .item {
      height: 44px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .item,
    .line {
      transition: none;
    }
  }
</style>
