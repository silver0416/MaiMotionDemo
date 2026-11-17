<script lang="ts">
  /**
   * 鍵帽提示：一顆膠囊鈕，裡面是鍵（kbd）與它的作用。
   * lit 時鍵反白一下，表示自動示範「現在按了這個鍵」；點下去等於按那個鍵。
   * still：只是提示、不能按（實際畫面導覽的鍵帽列）。不是按鈕，所以不畫成膠囊：一顆方形的鍵（和內文的 kbd 同一種外形，大一點）加旁邊的說明字。
   */
  interface Props {
    /** 鍵上的字 */
    cap: string;
    /** 這個鍵做什麼 */
    label: string;
    lit?: boolean;
    disabled?: boolean;
    still?: boolean;
    onclick?: () => void;
  }

  let { cap, label, lit = false, disabled = false, still = false, onclick }: Props = $props();
</script>

{#if still}
  <span class="keycap-still" class:is-lit={lit}>
    <kbd class="cap">{cap}</kbd>
    <span class="cap-label">{label}</span>
  </span>
{:else}
  <button class="btn keycap" class:is-lit={lit} {disabled} {onclick} aria-label={`${label}（按鍵 ${cap}）`}>
    <kbd class="cap">{cap}</kbd>
    <span class="cap-label">{label}</span>
  </button>
{/if}

<style>
  .keycap {
    gap: var(--space-2);
    padding: 0 12px 0 4px;
    background: var(--c-control);
    color: var(--c-text);
    font-weight: 500;
  }

  /* 只是提示：一顆鍵加說明字，沒有外框。 */
  .keycap-still {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--c-text);
    font-weight: 500;
  }

  .keycap-still .cap {
    min-width: 32px;
    height: 32px;
    padding: 0 var(--space-2);
    border-bottom-width: 3px;
    border-radius: var(--radius-md);
    background: var(--c-control);
    font-size: var(--fs-md);
  }

  .keycap-still .cap-label {
    font-size: var(--fs-md);
  }

  .keycap-still.is-lit .cap {
    background: var(--c-invert-bg);
    border-color: var(--c-invert-bg);
    color: var(--c-on-invert);
    transform: translateY(2px);
  }

  .cap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--c-border-strong);
    border-bottom-width: 2px;
    border-radius: var(--radius-pill);
    background: var(--c-panel);
    color: var(--c-text-strong);
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    font-weight: 600;
    line-height: 1;
    transition:
      background-color 120ms ease,
      color 120ms ease,
      border-color 120ms ease,
      transform 90ms ease-out;
  }

  .keycap.is-lit .cap {
    background: var(--c-invert-bg);
    border-color: var(--c-invert-bg);
    color: var(--c-on-invert);
    transform: translateY(1px);
  }

  .cap-label {
    font-size: var(--fs-sm);
  }

  @media (pointer: coarse) {
    .cap {
      min-width: 32px;
      height: 32px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .cap {
      transition: none;
    }
  }
</style>
