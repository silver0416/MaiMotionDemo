<script lang="ts">
  /**
   * 面板用的載入骨架：一個標題列加上幾張卡片，寬度錯落，看起來像還在排版的內容。
   * 只負責畫面；何時顯示由外層決定（通常放在 .skeleton-veil 裡）。
   */
  let { cards = 3, lines = 2, label = '載入中' }: { cards?: number; lines?: number; label?: string } = $props();

  const WIDTHS = [92, 68, 80, 56, 74];
</script>

<div class="skeleton-list" role="status" aria-label={label}>
  <span class="skeleton skeleton--line head"></span>
  {#each { length: cards } as _, card (card)}
    <div class="card">
      <span class="skeleton skeleton--line title" style={`width:${WIDTHS[card % WIDTHS.length] - 30}%`}></span>
      {#each { length: lines } as _, line (line)}
        <span class="skeleton skeleton--line" style={`width:${WIDTHS[(card + line + 1) % WIDTHS.length]}%`}></span>
      {/each}
    </div>
  {/each}
</div>

<style>
  .skeleton-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }

  .head {
    width: 38%;
    height: 12px;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
  }

  .title {
    height: 12px;
  }
</style>
