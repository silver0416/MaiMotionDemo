<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';

  /**
   * 只畫看得到的列的清單。每列高度由 heightOf 決定（必須與樣式一致），
   * 先算好每列的位置，捲動時只換可視範圍內的列；整首上千顆也不會讓切換分頁卡住。
   * 列由外層用 row 片段畫出，外層要把 style 套在列的根元素上（絕對定位與高度）。
   */
  let {
    items,
    heightOf,
    keyOf,
    row,
    empty,
    focusIndex = null,
    focusToken = undefined,
    overscan = 8,
    label,
    class: className = '',
  }: {
    items: T[];
    heightOf: (item: T) => number;
    keyOf: (item: T) => string;
    row: Snippet<[item: T, index: number, style: string]>;
    empty?: Snippet;
    /**
     * 這一列要捲進可視範圍。只在它換成別列（選取改變）時捲；清單內容或列高改變
     * （例如切換打法路線、換模型方案）不捲，免得畫面從正在看的地方跳回選取那列。
     */
    focusIndex?: number | null;
    /** 這個值換了，選取沒變也再對準一次（例如換篩選，清單整個換過）。 */
    focusToken?: unknown;
    overscan?: number;
    label: string;
    class?: string;
  } = $props();

  let element = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(0);

  /** tops[i] 是第 i 列的上緣，最後一格是總高度。 */
  const tops = $derived.by(() => {
    const result = new Float64Array(items.length + 1);
    for (let i = 0; i < items.length; i++) result[i + 1] = result[i] + heightOf(items[i]);
    return result;
  });

  /** 第一個下緣超過 y 的列。 */
  function rowAt(y: number): number {
    let low = 0;
    let high = items.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (tops[mid + 1] <= y) low = mid + 1;
      else high = mid;
    }
    return low;
  }

  const visible = $derived.by(() => {
    if (items.length === 0) return [];
    const start = Math.max(0, rowAt(scrollTop) - overscan);
    const end = Math.min(items.length, rowAt(scrollTop + Math.max(viewport, 1)) + 1 + overscan);
    const out: { item: T; index: number }[] = [];
    for (let index = start; index < end; index++) out.push({ item: items[index], index });
    return out;
  });

  /** 上一次對準的列與 focusToken；一樣就不再捲（只用來比較，不需要是響應式狀態）。 */
  let aimed: { key: string; token: unknown } | null = null;

  $effect(() => {
    const index = focusIndex;
    const token = focusToken;
    const list = element;
    if (!list) return;
    if (index === null || index < 0 || index >= items.length) {
      aimed = null;
      return;
    }
    const key = keyOf(items[index]);
    if (aimed?.key === key && aimed.token === token) return;
    aimed = { key, token };
    const top = tops[index];
    const bottom = tops[index + 1];
    if (top < list.scrollTop) list.scrollTop = top;
    else if (bottom > list.scrollTop + list.clientHeight) list.scrollTop = bottom - list.clientHeight;
  });
</script>

<div
  class={`virtual scroll ${className}`}
  bind:this={element}
  bind:clientHeight={viewport}
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  role="listbox"
  aria-label={label}
>
  {#if items.length === 0}
    {@render empty?.()}
  {:else}
    <div class="canvas" style={`height:${tops[items.length]}px`}>
      {#each visible as entry (keyOf(entry.item))}
        {@render row(
          entry.item,
          entry.index,
          `position:absolute;left:0;right:0;top:${tops[entry.index]}px;height:${tops[entry.index + 1] - tops[entry.index]}px`,
        )}
      {/each}
    </div>
  {/if}
</div>

<style>
  .canvas {
    position: relative;
  }
</style>
