<script lang="ts">
  import { flip } from 'svelte/animate';
  import { V3_SCORE_GROUPS } from '$app/lib/contract';
  import { formatNumber } from '$app/lib/format';
  import { reducedMotion } from '$app/lib/press';
  import { segmented } from '$app/lib/segmented';
  import { DemoPlayer, userAction } from './player.svelte';
  import Stage from './Stage.svelte';

  /**
   * 示意：三群分數（動作效率、姿態與分工、短期負荷）疊成總分，候選方案依總分由低到高排名。
   * 數字全是示意用的（畫面上標「示意數字」），不是任何譜面的分析結果。
   * 群組與分項的名稱取自 App 的 V3_SCORE_GROUPS；候選的樣子照 SolutionPanel 的清單列。
   * 文字對照 model/overview.md 與 play/solutions.md。
   */

  type GroupId = 'movement' | 'posture' | 'fatigue';

  interface Candidate {
    id: string;
    name: string;
    /** 各分項的示意值，key 同 V3_SCORE_GROUPS 的 items */
    parts: Record<string, number>;
  }

  const CANDIDATES: Candidate[] = [
    {
      id: 'a',
      name: '候選 A',
      parts: {
        travel: 2.4, speedStrain: 0.6, compressionStrain: 0.2,
        excursion: 0.7, crossExposure: 0.3, handover: 0.4, ownershipSwitch: 0.2,
        jackFatigue: 0.4, reversal: 0.3, workload: 0,
      },
    },
    {
      id: 'b',
      name: '候選 B',
      parts: {
        travel: 2.0, speedStrain: 0.4, compressionStrain: 0.2,
        excursion: 1.2, crossExposure: 1.1, handover: 0.5, ownershipSwitch: 0.3,
        jackFatigue: 0.3, reversal: 0.3, workload: 0,
      },
    },
    {
      id: 'c',
      name: '候選 C',
      parts: {
        travel: 2.6, speedStrain: 0.6, compressionStrain: 0.2,
        excursion: 0.4, crossExposure: 0.1, handover: 0.2, ownershipSwitch: 0.2,
        jackFatigue: 0.4, reversal: 0.2, workload: 0,
      },
    },
  ];

  const GROUPS = V3_SCORE_GROUPS.map((group) => ({ ...group, id: group.id as GroupId }));
  /** 長條的滿格：比最大的總分再多一點。 */
  const SCALE = 7;

  const groupValue = (item: Candidate, group: (typeof GROUPS)[number]) =>
    group.items.reduce((sum, entry) => sum + (item.parts[entry.key] ?? 0), 0);
  const total = (item: Candidate) => GROUPS.reduce((sum, group) => sum + groupValue(item, group), 0);
  const peak = (item: Candidate) => Math.max(...Object.values(item.parts), 1e-6);

  let filled = $state(0);
  let sorted = $state(false);
  let selected = $state<string | null>(null);
  let open = $state<string | null>(null);
  let segEl = $state<HTMLElement | null>(null);
  let listEl = $state<HTMLElement | null>(null);

  const ordered = $derived(sorted ? [...CANDIDATES].sort((a, b) => total(a) - total(b)) : CANDIDATES);
  const best = $derived(Math.min(...CANDIDATES.map(total)));

  function reset() {
    filled = 0;
    sorted = false;
    selected = null;
    open = null;
  }

  const fmt = (value: number) => formatNumber(value, 1);

  function pick(id: string) {
    if (selected === id) {
      open = open === id ? null : id;
    } else {
      selected = id;
      open = id;
    }
    const item = CANDIDATES.find((entry) => entry.id === id)!;
    player.say =
      open === id
        ? `${item.name}：總分 ${fmt(total(item))}，展開分項。`
        : `${item.name}：收起分項。`;
  }

  const userPick = (id: string) => userAction(player, () => pick(id))();
  const userSort = (next: boolean) =>
    userAction(player, () => {
      filled = 3;
      sorted = next;
      player.say = next ? '依總分由低到高排：越前面，模型越偏好。' : '回到原本的順序。';
    })();

  const player = new DemoPlayer({
    intro: '三個候選方案，各有三群分數。',
    outro: '示範結束。點任一個候選看分項；這裡的分數都是示意數字。',
    reset,
    steps: [
      { say: '三個候選方案，各有三群分數（這裡是示意數字）。', hold: 1400 },
      { say: '第一群「動作效率」：手移動得省不省力。', run: () => (filled = 1), hold: 1500 },
      { say: '第二群「姿態與分工」：姿勢自不自然、分工有沒有亂掉。', run: () => (filled = 2), hold: 1500 },
      { say: '第三群「短期負荷」：一隻手有沒有被操太兇。三群直接相加就是總分。', run: () => (filled = 3), hold: 2000 },
      {
        say: '總分越低，模型越偏好：依總分由低到高排名。',
        target: () => segEl?.querySelector('[data-sort="on"]'),
        run: () => (sorted = true),
        hold: 1800,
      },
      {
        say: '點第 1 名展開：每一群底下還有幾個分項。',
        target: () => listEl?.querySelector('[data-candidate="c"]'),
        run: () => {
          selected = 'c';
          open = 'c';
        },
        hold: 2200,
      },
      {
        say: '第 1 名只比第 2 名少 0.6：差距不是機率，只表示在目前的設定下模型比較偏好它。',
        hold: 2800,
      },
    ],
  });

  function onkey(key: string): boolean {
    if (key !== 'ArrowDown' && key !== 'ArrowUp') return false;
    const index = ordered.findIndex((item) => item.id === selected);
    const next = ordered[(index + (key === 'ArrowDown' ? 1 : -1) + ordered.length) % ordered.length] ?? ordered[0];
    userAction(player, () => {
      selected = next.id;
      open = next.id;
      player.say = `${next.name}：總分 ${fmt(total(next))}。`;
    })();
    requestAnimationFrame(() => listEl?.querySelector<HTMLElement>(`[data-candidate="${next.id}"]`)?.focus());
    return true;
  }

  const motion = (ms: number) => (reducedMotion() ? 0 : ms);
</script>

<Stage label="三群分數怎麼排出候選方案" {player} {onkey}>
  <div class="cost">
    <div class="head">
      <span class="title">候選方案</span>
      <span class="badge badge--sample">示意數字</span>
      <span class="spacer"></span>
      <div class="seg seg--fit seg--sm" role="group" aria-label="排序" use:segmented bind:this={segEl}>
        <button class="seg-btn" data-sort="off" aria-pressed={!sorted} onclick={() => userSort(false)}>原本順序</button>
        <button class="seg-btn" data-sort="on" aria-pressed={sorted} onclick={() => userSort(true)}>由低到高</button>
      </div>
    </div>

    <div class="legend xsmall" aria-hidden="true">
      {#each GROUPS as group, index (group.id)}
        <span class="legend-item"><span class={`swatch swatch--${index}`}></span>{group.label}</span>
      {/each}
    </div>

    <div class="candidates" role="radiogroup" aria-label="候選方案" bind:this={listEl}>
      {#each ordered as item, rank (item.id)}
        {@const sum = total(item)}
        <div class="slot" animate:flip={{ duration: motion(360) }}>
          <button
            class="candidate"
            class:is-selected={selected === item.id}
            role="radio"
            aria-checked={selected === item.id}
            data-candidate={item.id}
            tabindex={selected === item.id || (selected === null && rank === 0) ? 0 : -1}
            onclick={() => userPick(item.id)}
          >
            <span class="candidate-head">
              <span class="mono">{item.name}</span>
              {#if sorted}<span class="xsmall muted">第 {rank + 1} 名</span>{/if}
              <span class="spacer"></span>
              <span class="mono total" class:is-shown={filled === 3}>總分 {fmt(sum)}</span>
            </span>
            <span class="stack-bar" aria-hidden="true">
              {#each GROUPS as group, index (group.id)}
                {@const before = GROUPS.slice(0, index).reduce((acc, prev) => acc + groupValue(item, prev), 0)}
                <span
                  class={`piece swatch--${index}`}
                  class:is-on={filled > index}
                  style:left={`${(before / SCALE) * 100}%`}
                  style:width={`${(groupValue(item, group) / SCALE) * 100}%`}
                ></span>
              {/each}
            </span>
            <span class="xsmall mono line">
              {GROUPS.slice(0, Math.max(filled, 0))
                .map((group) => `${group.short} ${fmt(groupValue(item, group))}`)
                .join('・') || '　'}
            </span>
            {#if sorted && rank > 0 && filled === 3}
              <span class="xsmall gap">比第 1 名多 {fmt(sum - best)}</span>
            {/if}
          </button>
          {#if open === item.id}
            {@const top = peak(item)}
            <div class="parts">
              {#each GROUPS as group (group.id)}
                <div class="part-group">
                  <div class="part-head">
                    <span class="part-label">{group.label}</span>
                    <span class="mono">{fmt(groupValue(item, group))}</span>
                  </div>
                  {#each group.items as entry (entry.key)}
                    <div class="part-row">
                      <span class="xsmall">{entry.label}</span>
                      <span class="xsmall mono">{fmt(item.parts[entry.key] ?? 0)}</span>
                      <div class="bar-track"><div class="bar-fill" style:width={`${((item.parts[entry.key] ?? 0) / top) * 100}%`}></div></div>
                    </div>
                  {/each}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</Stage>

<style>
  .cost {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .title {
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    color: var(--c-text-dim);
  }

  .legend-item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .swatch {
    display: inline-block;
    width: 14px;
    height: 8px;
    border-radius: var(--radius-pill);
  }

  /* 三群用三階中性灰，不另加色相；中間以面板色細縫分開，色弱也分得出段落。 */
  .swatch--0 {
    background: var(--c-text);
  }

  .swatch--1 {
    background: var(--c-text-dim);
  }

  .swatch--2 {
    background: var(--c-border-strong);
  }

  .candidates {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .slot {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  /* 候選列：同 SolutionPanel（圓角 16、透明底，hover 出底、選中凸起底加框、按下縮 .99）。 */
  .candidate {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    width: 100%;
    padding: 10px 14px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    color: var(--c-text);
    cursor: pointer;
    transition:
      background-color 120ms ease,
      border-color 120ms ease,
      transform 90ms ease-out;
  }

  @media (hover: hover) {
    .candidate:hover {
      background: var(--c-control);
    }
  }

  .candidate:active {
    transform: scale(0.99);
  }

  .candidate.is-selected {
    border-color: var(--c-border-strong);
    background: var(--c-control-hover);
  }

  .candidate-head {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .total {
    opacity: 0;
    transition: opacity 200ms ease;
  }

  .total.is-shown {
    opacity: 1;
  }

  .stack-bar {
    position: relative;
    display: block;
    height: 12px;
    border-radius: var(--radius-pill);
    background: var(--c-control);
    overflow: hidden;
  }

  .piece {
    position: absolute;
    top: 0;
    bottom: 0;
    border-right: 2px solid var(--c-panel);
    transform: scaleX(0);
    transform-origin: left center;
    transition: transform 520ms var(--ease-out);
  }

  .piece.is-on {
    transform: none;
  }

  .line {
    min-height: 1.6em;
    color: var(--c-text);
  }

  .gap {
    color: var(--c-text-dim);
  }

  /* 分項：沒有外框，靠縮排與間距分組（不包第二層卡片）。 */
  .parts {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-1) 14px var(--space-2);
  }

  .part-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .part-head {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .part-label {
    color: var(--c-text-strong);
  }

  .part-row {
    display: grid;
    grid-template-columns: minmax(0, 7rem) 2.5rem minmax(0, 1fr);
    align-items: center;
    gap: var(--space-2);
  }

  @media (pointer: coarse) {
    .candidate {
      min-height: 44px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .piece,
    .total,
    .candidate {
      transition: none;
    }
  }
</style>
