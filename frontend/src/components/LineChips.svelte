<script lang="ts">
  import { annotation } from '../state/annotation.svelte';

  /**
   * 一排打法按鈕：色點＋名稱，按下去切換路線。
   * current 是目前走的那條（按下狀態）；色點是那條線在分支圖上的顏色，另有名稱，不只靠顏色辨識。
   */
  let { lines, current, label }: { lines: string[]; current: string; label: string } = $props();
</script>

<div class="chips" role="group" aria-label={label}>
  {#each lines as line (line)}
    <button
      class="btn chip"
      aria-pressed={line === current}
      onclick={() => annotation.setActive(line)}
      onmouseenter={() => (annotation.hoverLine = line)}
      onmouseleave={() => (annotation.hoverLine = null)}
    >
      <span class="dot" style={`background:${annotation.lineColor(line)}`}></span>
      <span class="name">{annotation.lineName(line)}</span>
    </button>
  {/each}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    min-width: 0;
  }

  .chip {
    gap: var(--space-1);
    max-width: 100%;
    padding: 0 var(--space-2);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
