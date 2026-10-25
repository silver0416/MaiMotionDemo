<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import Icon from './Icon.svelte';
  import { reducedMotion } from '../lib/press';
  import { carriesFiles, dialogOpen, importDropped } from '../state/dropImport.svelte';

  /** 拖著檔案進到視窗裡；dragenter／dragleave 會在每個子元素觸發，用計數判斷真的離開了。 */
  let depth = $state(0);
  let count = $state(0);
  const active = $derived(depth > 0);
  const motion = (duration: number) => (reducedMotion() ? 0 : duration);

  function onDragEnter(event: DragEvent) {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    // 對話框開著時不接（游標顯示不能放），也不畫提示。
    if (dialogOpen()) return;
    depth += 1;
    count = event.dataTransfer?.items.length ?? 0;
  }

  function onDragOver(event: DragEvent) {
    if (!carriesFiles(event)) return;
    // 一律攔下：沒攔的話 WebView 會直接打開檔案、離開 App 頁面。
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = dialogOpen() ? 'none' : 'copy';
  }

  function onDragLeave(event: DragEvent) {
    if (!carriesFiles(event) || depth === 0) return;
    depth -= 1;
  }

  function onDrop(event: DragEvent) {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    const wasActive = depth > 0;
    depth = 0;
    if (wasActive) void importDropped(event);
  }
</script>

<svelte:window ondragenter={onDragEnter} ondragover={onDragOver} ondragleave={onDragLeave} ondrop={onDrop} />

{#if active}
  <div class="drop-frame" transition:fade={{ duration: motion(120) }} aria-hidden="true"></div>
  <div class="drop-wrap">
    <div class="drop-card" role="status" transition:scale={{ start: 0.97, duration: motion(180) }}>
      <span class="drop-icon"><Icon name="upload" size={24} /></span>
      <div class="drop-text">
        <div class="drop-title">放開就匯入{count > 1 ? `這 ${count} 個檔案` : ''}</div>
        <p class="drop-hint">標記檔（.json）會先列出新紀錄與合併；譜面文字檔（.txt）直接分析後新增。</p>
      </div>
    </div>
  </div>
{/if}

<style>
  /* 放置提示：整個視窗外圈一道主色實線框，中央一張不透明的提示卡；不蓋半透明遮罩，盤面照常看得到。 */
  .drop-frame {
    position: fixed;
    inset: var(--space-1);
    z-index: 30;
    border: 2px solid var(--c-primary);
    border-radius: var(--radius-md);
    pointer-events: none;
  }

  .drop-wrap {
    position: fixed;
    inset: 0;
    z-index: 31;
    display: grid;
    place-items: center;
    padding: var(--space-4);
    pointer-events: none;
  }

  .drop-card {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    max-width: 380px;
    padding: var(--space-4) var(--space-5);
    background: var(--c-panel);
    border: 1px solid var(--c-primary);
    border-radius: var(--radius-pop);
    box-shadow: var(--shadow-pop);
    color: var(--c-text-strong);
  }

  .drop-icon {
    display: inline-flex;
    color: var(--c-primary);
  }

  .drop-title {
    font-size: var(--fs-lg);
    font-weight: 700;
  }

  .drop-hint {
    margin: var(--space-1) 0 0;
    font-size: var(--fs-sm);
    color: var(--c-text-dim);
  }
</style>
