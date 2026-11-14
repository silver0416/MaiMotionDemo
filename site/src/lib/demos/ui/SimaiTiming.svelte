<script lang="ts">
  import { scale } from 'svelte/transition';
  import Icon from '$app/components/Icon.svelte';
  import { bounce, reducedMotion } from '$app/lib/press';
  import { segmented } from '$app/lib/segmented';
  import { DemoPlayer, userAction, type Step } from './player.svelte';
  import Stage from './Stage.svelte';

  /**
   * 示意：simai 的 (BPM){分割} 與逗號怎麼換成秒數，以及 Slide 起點被拍下後先等一拍才出發。
   * 三個固定範例（不解析自由輸入）；每一格的時間是照 60 ÷ BPM × 4 ÷ 分割 手算好的。
   * 自動示範先走 {4} 的例子再走 Slide；之後可以切換範例、用「上一步／下一步」或 ←／→ 自己逐格看。
   * 文字對照 charts/simai.md。
   */

  type ExampleId = 'quarter' | 'eighth' | 'slide';

  interface Item {
    id: string;
    lane: 'tap' | 'slide';
    kind: 'tap' | 'star' | 'wait' | 'move';
    t: number;
    to?: number;
    label: string;
  }

  interface Frame {
    /** 正在讀的記號 */
    tok: number;
    /** 讀完這一格後的時間（秒） */
    t: number;
    add?: string[];
    /** 逗號推進的那一格 */
    cell?: [number, number];
    /** 算式要強調哪一段 */
    focus?: 'bpm' | 'div';
    say: string;
  }

  interface Example {
    id: ExampleId;
    label: string;
    tokens: string[];
    bpm: number;
    div: number;
    cell: number;
    items: Item[];
    frames: Frame[];
  }

  const EXAMPLES: Example[] = [
    {
      id: 'quarter',
      label: '{4}',
      tokens: ['(120)', '{4}', '1', ',', '2', ',', ',', '3', ',', 'E'],
      bpm: 120,
      div: 4,
      cell: 0.5,
      items: [
        { id: 'a1', lane: 'tap', kind: 'tap', t: 0, label: '1' },
        { id: 'a2', lane: 'tap', kind: 'tap', t: 0.5, label: '2' },
        { id: 'a3', lane: 'tap', kind: 'tap', t: 1.5, label: '3' },
      ],
      frames: [
        { tok: 0, t: 0, focus: 'bpm', say: '(120)：BPM 120，一拍是 60 ÷ 120 = 0.5 秒。' },
        { tok: 1, t: 0, focus: 'div', say: '{4}：每個逗號推進一個 4 分音符，一格 = 60 ÷ 120 × 4 ÷ 4 = 0.5 秒。' },
        { tok: 2, t: 0, add: ['a1'], say: '0 秒：按 1。' },
        { tok: 3, t: 0.5, cell: [0, 0.5], say: '逗號：時間往前推一格（+0.5 秒），到 0.5 秒。' },
        { tok: 4, t: 0.5, add: ['a2'], say: '0.5 秒：按 2。' },
        { tok: 5, t: 1, cell: [0.5, 1], say: '逗號：再推一格，到 1 秒。' },
        { tok: 6, t: 1.5, cell: [1, 1.5], say: '又一個逗號：中間沒寫音符，這一格是空的，到 1.5 秒。' },
        { tok: 7, t: 1.5, add: ['a3'], say: '1.5 秒：按 3。' },
        { tok: 8, t: 2, cell: [1.5, 2], say: '逗號：到 2 秒。' },
        { tok: 9, t: 2, say: 'E：譜面結束。三顆分別在 0、0.5、1.5 秒。' },
      ],
    },
    {
      id: 'eighth',
      label: '{8}',
      tokens: ['(120)', '{8}', '1', ',', '2', ',', '3', ',', '4', ',', 'E'],
      bpm: 120,
      div: 8,
      cell: 0.25,
      items: [
        { id: 'b1', lane: 'tap', kind: 'tap', t: 0, label: '1' },
        { id: 'b2', lane: 'tap', kind: 'tap', t: 0.25, label: '2' },
        { id: 'b3', lane: 'tap', kind: 'tap', t: 0.5, label: '3' },
        { id: 'b4', lane: 'tap', kind: 'tap', t: 0.75, label: '4' },
      ],
      frames: [
        { tok: 0, t: 0, focus: 'bpm', say: '(120)：一拍一樣是 0.5 秒。' },
        { tok: 1, t: 0, focus: 'div', say: '{8}：每個逗號推進一個 8 分音符，一格 = 60 ÷ 120 × 4 ÷ 8 = 0.25 秒，比 {4} 密一倍。' },
        { tok: 2, t: 0, add: ['b1'], say: '0 秒：按 1。' },
        { tok: 3, t: 0.25, cell: [0, 0.25], say: '逗號：+0.25 秒，到 0.25 秒。' },
        { tok: 4, t: 0.25, add: ['b2'], say: '0.25 秒：按 2。' },
        { tok: 5, t: 0.5, cell: [0.25, 0.5], say: '逗號：到 0.5 秒。' },
        { tok: 6, t: 0.5, add: ['b3'], say: '0.5 秒：按 3。' },
        { tok: 7, t: 0.75, cell: [0.5, 0.75], say: '逗號：到 0.75 秒。' },
        { tok: 8, t: 0.75, add: ['b4'], say: '0.75 秒：按 4。' },
        { tok: 9, t: 1, cell: [0.75, 1], say: '逗號：到 1 秒。' },
        { tok: 10, t: 1, say: 'E：譜面結束。四顆在 1 秒內打完。' },
      ],
    },
    {
      id: 'slide',
      label: 'Slide',
      tokens: ['(120)', '{4}', '1-5[4:3]', ',', '8', ',', '7', ',', '6', ',', 'E'],
      bpm: 120,
      div: 4,
      cell: 0.5,
      items: [
        { id: 'c-star', lane: 'slide', kind: 'star', t: 0, label: '1' },
        { id: 'c-wait', lane: 'slide', kind: 'wait', t: 0, to: 0.5, label: '等一拍' },
        { id: 'c-move', lane: 'slide', kind: 'move', t: 0.5, to: 2, label: '滑 1.5 秒到 5' },
        { id: 'c8', lane: 'tap', kind: 'tap', t: 0.5, label: '8' },
        { id: 'c7', lane: 'tap', kind: 'tap', t: 1, label: '7' },
        { id: 'c6', lane: 'tap', kind: 'tap', t: 1.5, label: '6' },
      ],
      frames: [
        { tok: 0, t: 0, focus: 'bpm', say: '(120)：一拍 0.5 秒。' },
        { tok: 1, t: 0, focus: 'div', say: '{4}：一格 = 60 ÷ 120 × 4 ÷ 4 = 0.5 秒。' },
        { tok: 2, t: 0, add: ['c-star'], say: '1-5[4:3]：0 秒先在 1 拍下起點的星星。' },
        { tok: 2, t: 0, add: ['c-wait'], say: '先等一拍：0 到 0.5 秒，星星還停在 1。' },
        { tok: 2, t: 0, add: ['c-move'], say: '0.5 秒星星出發；[4:3] 是 3 個 4 分音符 = 1.5 秒，2 秒滑到 5。' },
        { tok: 3, t: 0.5, cell: [0, 0.5], say: '逗號：到 0.5 秒。' },
        { tok: 4, t: 0.5, add: ['c8'], say: '0.5 秒：按 8，這時星星剛出發。' },
        { tok: 5, t: 1, cell: [0.5, 1], say: '逗號：到 1 秒。' },
        { tok: 6, t: 1, add: ['c7'], say: '1 秒：按 7。' },
        { tok: 7, t: 1.5, cell: [1, 1.5], say: '逗號：到 1.5 秒。' },
        { tok: 8, t: 1.5, add: ['c6'], say: '1.5 秒：按 6，星星還在滑。' },
        { tok: 9, t: 2, cell: [1.5, 2], say: '逗號：到 2 秒，星星剛好滑到 5。' },
        { tok: 10, t: 2, say: 'E：譜面結束。滑行期間另外有 8、7、6 三顆 Tap。' },
      ],
    },
  ];

  /** 時間軸畫到 2.5 秒，三個範例共用同一把尺，{8} 比 {4} 密就看得出來。 */
  const AXIS = 2.5;
  const TICKS = [0, 0.5, 1, 1.5, 2, 2.5];

  let exampleId = $state<ExampleId>('quarter');
  let pos = $state(-1);
  let segEl = $state<HTMLElement | null>(null);

  const example = $derived(EXAMPLES.find((item) => item.id === exampleId)!);
  const frame = $derived(pos >= 0 ? example.frames[pos] : null);
  const time = $derived(frame?.t ?? 0);
  const shown = $derived(new Set(example.frames.slice(0, pos + 1).flatMap((item) => item.add ?? [])));
  const cells = $derived(example.frames.slice(0, pos + 1).flatMap((item) => (item.cell ? [item.cell] : [])));
  const pct = (t: number) => (t / AXIS) * 100;
  const fmt = (t: number) => (Math.round(t * 1000) / 1000).toString();

  function choose(id: ExampleId) {
    exampleId = id;
    pos = -1;
  }

  function step(delta: number) {
    pos = Math.max(-1, Math.min(example.frames.length - 1, pos + delta));
    player.say = frame ? frame.say : `回到開頭：${example.tokens.join('')}`;
  }

  const userChoose = (id: ExampleId) =>
    userAction(player, () => {
      choose(id);
      player.say = `${example.tokens.join('')}：按「下一步」或 → 一格一格讀。`;
    })();
  const userStep = (delta: number) => userAction(player, () => step(delta))();

  function framesOf(id: ExampleId): Step[] {
    const target = EXAMPLES.find((item) => item.id === id)!;
    return target.frames.map((item, index) => ({
      say: item.say,
      hold: item.focus ? 1700 : item.add?.includes('c-move') ? 2200 : 1100,
      run: () => {
        exampleId = id;
        pos = index;
      },
    }));
  }

  const player = new DemoPlayer({
    intro: '一段 simai 怎麼換成時間：逐格讀過去。',
    outro: '示範結束。可以切換範例，或按「下一步」（→）自己一格一格讀。',
    reset: () => choose('quarter'),
    steps: [
      ...framesOf('quarter'),
      {
        say: '換成 Slide 的例子：(120){4}1-5[4:3],8,7,6,E。',
        target: () => segEl?.querySelector('[data-example="slide"]'),
        run: () => choose('slide'),
        hold: 1400,
      },
      ...framesOf('slide'),
    ],
  });

  function onkey(key: string): boolean {
    if (key === 'ArrowRight') {
      userStep(1);
      return true;
    }
    if (key === 'ArrowLeft') {
      userStep(-1);
      return true;
    }
    return false;
  }

  const motion = (ms: number) => (reducedMotion() ? 0 : ms);
  function grow(_node: Element, { duration }: { duration: number }) {
    return { duration, css: (t: number) => `transform: scaleX(${t}); transform-origin: left center;` };
  }
</script>

<Stage label="simai 的時間怎麼算" {player} {onkey}>
  <div class="timing">
    <div class="top">
      <div class="seg seg--fit" role="group" aria-label="範例" use:segmented bind:this={segEl}>
        {#each EXAMPLES as item (item.id)}
          <button class="seg-btn mono" data-example={item.id} aria-pressed={exampleId === item.id} onclick={() => userChoose(item.id)}>
            {item.label}
          </button>
        {/each}
      </div>
      <span class="spacer"></span>
      <span class="clock mono" aria-label={`現在 ${fmt(time)} 秒`}>{fmt(time)}s</span>
      <div class="btn-group" role="group" aria-label="逐格讀">
        <button class="btn-group-item" use:bounce={-1} onclick={() => userStep(-1)} disabled={pos < 0} aria-label="上一步（←）">
          <Icon name="chevron-left" size={14} />上一步
        </button>
        <button
          class="btn-group-item"
          use:bounce={1}
          onclick={() => userStep(1)}
          disabled={pos >= example.frames.length - 1}
          aria-label="下一步（→）"
        >
          下一步<Icon name="chevron-right" size={14} />
        </button>
      </div>
    </div>

    <div class="code mono" aria-label={`simai：${example.tokens.join('')}`}>
      {#each example.tokens as token, index (index)}
        <span class="tok" class:is-read={frame !== null && index < frame.tok} class:is-now={frame?.tok === index} aria-hidden="true">{token}</span>
      {/each}
    </div>

    <p class="formula">
      <span class="muted">一格 =</span>
      <span class="mono">60 ÷ <span class="part" class:is-focus={frame?.focus === 'bpm'}>{example.bpm}</span> × 4 ÷
        <span class="part" class:is-focus={frame?.focus === 'div'}>{example.div}</span> = {example.cell} 秒</span>
      <span class="muted xsmall">（60 ÷ BPM × 4 ÷ 分割）</span>
    </p>

    <div class="chart" aria-hidden="true">
      <span class="lane-name">時間</span>
      <div class="axis">
        {#each TICKS as tick (tick)}
          <span class="tick-label mono" style:left={`${pct(tick)}%`}>{tick}s</span>
        {/each}
      </div>

      <span class="lane-name">Tap</span>
      <div class="lane lane--tap">
        {#each cells as cell (cell.join('-'))}
          <span class="cell" class:is-now={frame?.cell === cell} style:left={`${pct(cell[0])}%`} style:width={`${pct(cell[1] - cell[0])}%`}></span>
        {/each}
        {#each example.items.filter((item) => item.lane === 'tap' && shown.has(item.id)) as item (item.id)}
          <span class="note" style:left={`${pct(item.t)}%`} in:scale={{ duration: motion(200), start: 0.5 }}>
            <span class="note-dot mono">{item.label}</span>
            <span class="note-time mono">{fmt(item.t)}</span>
          </span>
        {/each}
      </div>

      <span class="lane-name">Slide</span>
      <div class="lane lane--slide">
        {#if exampleId !== 'slide'}
          <span class="empty xsmall muted">這個例子沒有 Slide</span>
        {/if}
        {#each example.items.filter((item) => item.lane === 'slide' && shown.has(item.id)) as item (item.id)}
          {#if item.kind === 'star'}
            <span class="note" style:left={`${pct(item.t)}%`} in:scale={{ duration: motion(200), start: 0.5 }}>
              <span class="note-dot note-dot--star mono">{item.label}</span>
            </span>
          {:else}
            <span
              class={['span', item.kind === 'wait' ? 'span--wait' : 'span--move']}
              style:left={`${pct(item.t)}%`}
              style:width={`${pct((item.to ?? item.t) - item.t)}%`}
              in:grow={{ duration: motion(item.kind === 'move' ? 600 : 300) }}
            >
              <span class="span-label">{item.label}</span>
            </span>
          {/if}
        {/each}
      </div>

      <span></span>
      <div class="now-row">
        {#if frame?.cell}
          <span class="step-chip mono" style:left={`${pct((frame.cell[0] + frame.cell[1]) / 2)}%`}>+{fmt(frame.cell[1] - frame.cell[0])}s</span>
        {/if}
      </div>

      <!-- 播放頭：整條和軌道一樣寬，用 translateX 的百分比移動。 -->
      <div class="head-layer">
        <div class="head" style:transform={`translateX(${pct(time)}%)`}>
          <span class="head-line"></span>
        </div>
      </div>
    </div>
  </div>
</Stage>

<style>
  .timing {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .btn-group-item {
    padding: 0 10px;
  }

  @media (pointer: coarse) {
    .btn-group-item {
      height: 38px;
    }
  }

  .code {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-item);
    background: var(--c-control);
    font-size: var(--fs-lg);
    line-height: 1.4;
  }

  .tok {
    padding: 0 3px;
    border-radius: var(--radius-pill);
    color: var(--c-text-dim);
    transition:
      background-color 160ms ease,
      color 160ms ease;
  }

  .tok.is-read {
    color: var(--c-text-strong);
  }

  .tok.is-now {
    background: var(--c-invert-bg);
    color: var(--c-on-invert);
  }

  .formula {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 var(--space-2);
    font-size: var(--fs-md);
  }

  .part {
    padding: 0 3px;
    border-radius: var(--radius-pill);
    transition:
      background-color 160ms ease,
      color 160ms ease;
  }

  .part.is-focus {
    background: var(--c-accent);
    color: var(--c-on-hand);
  }

  /* ---- 時間軸：左邊一欄名稱，右邊是 0–2.5 秒 ---- */

  .chart {
    --name-w: 2.75rem;
    position: relative;
    display: grid;
    grid-template-columns: var(--name-w) minmax(0, 1fr);
    row-gap: var(--space-1);
    align-items: center;
    padding-right: var(--space-3);
  }

  .lane-name {
    color: var(--c-text-dim);
    font-size: var(--fs-xs);
    font-weight: 600;
  }

  .axis {
    position: relative;
    height: 18px;
  }

  .tick-label {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    color: var(--c-text-dim);
    font-size: var(--fs-xs);
  }

  .tick-label:first-child {
    transform: none;
  }

  .lane {
    position: relative;
    height: 52px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg);
  }

  .cell {
    position: absolute;
    top: 0;
    bottom: 0;
    border-right: 1px solid var(--c-border-strong);
    background: var(--c-control-hover);
  }

  .cell.is-now {
    border-right-color: var(--c-accent);
    border-right-width: 2px;
  }

  .note {
    position: absolute;
    top: 6px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    width: 0;
  }

  /* 音符：空心圓環（盤面語彙），裡面寫鍵位。 */
  .note-dot {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: 2px solid var(--c-text-strong);
    border-radius: 50%;
    background: var(--c-panel);
    color: var(--c-text-strong);
    font-size: var(--fs-xs);
    font-weight: 700;
  }

  .note-dot--star {
    border-style: dashed;
  }

  .note-time {
    color: var(--c-text-dim);
    font-size: var(--fs-xs);
    line-height: 1.2;
  }

  .lane--slide .note {
    top: 15px;
    z-index: 1;
  }

  .span {
    position: absolute;
    top: 17px;
    height: 18px;
    display: flex;
    align-items: center;
    border-radius: var(--radius-pill);
  }

  .span--wait {
    border: 1px dashed var(--c-text-dim);
  }

  .span--move {
    background: var(--c-text-dim);
  }

  .span-label {
    position: absolute;
    top: 100%;
    left: 50%;
    transform: translateX(-50%);
    color: var(--c-text);
    font-size: var(--fs-xs);
    line-height: 1.4;
    white-space: nowrap;
  }

  .span--wait .span-label {
    left: 0;
    transform: none;
  }

  .empty {
    position: absolute;
    top: 50%;
    left: var(--space-2);
    transform: translateY(-50%);
  }

  .now-row {
    position: relative;
    height: 20px;
  }

  .step-chip {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    padding: 0 6px;
    border-radius: var(--radius-pill);
    background: var(--c-accent);
    color: var(--c-on-hand);
    font-size: var(--fs-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .head-layer {
    position: absolute;
    top: 18px;
    bottom: 22px;
    left: var(--name-w);
    right: var(--space-3);
    pointer-events: none;
  }

  .head {
    position: absolute;
    inset: 0;
    transition: transform 500ms var(--ease-out);
  }

  .head-line {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -1px;
    width: 2px;
    background: var(--c-accent);
  }

  .clock {
    min-width: 6ch;
    color: var(--c-text-strong);
    font-size: var(--fs-lg);
    font-weight: 700;
    text-align: right;
  }

  @media (prefers-reduced-motion: reduce) {
    .head,
    .tok,
    .part {
      transition: none;
    }
  }
</style>
