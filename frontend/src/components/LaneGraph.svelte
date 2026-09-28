<script lang="ts" module>
  /** 每條軌道的寬度（px）。 */
  export const LANE_WIDTH = 12;

  export function laneColumnWidth(lanes: number): number {
    return lanes * LANE_WIDTH + 4;
  }
</script>

<script lang="ts">
  import type { LaneCell, LaneRow } from '../lib/annotation';
  import { annotation } from '../state/annotation.svelte';

  /**
   * 一列的打法分支圖（像 git 分支圖／電車路線）：主線在最左，分支在右側並排。
   * 路線經過的線段用那條線自己的顏色，停靠點畫圓圈；路線沒走到的線是灰色。
   * 灰線可以直接點選切換到那條線，滑鼠停著時整條線以它的顏色預覽。
   * 由外層決定列高，這裡撐滿整列並往下多畫 1px 蓋過列與列之間的分隔線。
   */
  let { row, lanes }: { row: LaneRow; lanes: number } = $props();

  const width = $derived(laneColumnWidth(lanes));
  const x = (lane: number) => 2 + lane * LANE_WIDTH + LANE_WIDTH / 2;

  // 座標：寬度用像素，高度 0～100 拉伸到列高（線寬不隨拉伸變粗）。
  function path(cell: LaneCell): string {
    const lx = x(cell.lane);
    // 從分出的線彎出來、最後彎進併入的線；兩條可以不同。
    const fx = x(cell.from);
    const tx = x(cell.to);
    switch (cell.kind) {
      case 'start':
        return `M ${fx} 0 C ${fx} 30, ${lx} 20, ${lx} 50 V 100`;
      case 'end':
        return `M ${lx} 0 V 50 C ${lx} 80, ${tx} 70, ${tx} 100`;
      case 'single':
        return `M ${fx} 0 C ${fx} 25, ${lx} 15, ${lx} 40 V 60 C ${lx} 85, ${tx} 75, ${tx} 100`;
      default:
        return `M ${lx} 0 V 100`;
    }
  }

  function lit(cell: LaneCell): boolean {
    return cell.on || annotation.hoverLine === cell.line;
  }

  // 先畫灰線，再把有顏色的疊在上面。
  const ordered = $derived([...row.cells].sort((a, b) => Number(lit(a)) - Number(lit(b))));

  // 在清單列（本身是按鈕）裡點線只切換路線，不選取那一列。
  function pick(event: MouseEvent, line: string) {
    event.preventDefault();
    event.stopPropagation();
    annotation.setActive(line);
  }
</script>

<span class="lanes" style={`width:${width}px`}>
  <svg viewBox={`0 0 ${width} 100`} preserveAspectRatio="none" width={width} height="100%" aria-hidden="true">
    <!-- Slide 的滑行步驟排在較晚的列，錯開的兩條分支偶爾會在同一軌道相接，所以不以軌道當鍵。 -->
    {#each ordered as cell, index (index)}
      <path d={path(cell)} class="line" class:on={cell.on} style={lit(cell) ? `stroke:${annotation.lineColor(cell.line)}` : undefined} />
    {/each}
    {#each row.cells as cell, index (index)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <path
        d={path(cell)}
        class="hit"
        class:is-on={cell.on}
        onclick={(event) => pick(event, cell.line)}
        onmouseenter={() => (annotation.hoverLine = cell.line)}
        onmouseleave={() => (annotation.hoverLine = null)}
      >
        <title>{cell.on ? annotation.lineName(cell.line) : `切換到「${annotation.lineName(cell.line)}」`}</title>
      </path>
    {/each}
  </svg>
  <span class="stop" style={`left:${x(row.stop) - 4}px;border-color:${annotation.lineColor(row.stopLine)}`}></span>
</span>

<style>
  .lanes {
    position: absolute;
    top: 0;
    bottom: -1px;
    left: 0;
    pointer-events: none;
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
  }

  .line.on {
    stroke-width: 3;
  }

  /* 看不見的粗線當點擊範圍，比 2px 的線好點。 */
  .hit {
    fill: none;
    stroke: transparent;
    stroke-width: 10;
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
  }
</style>
