<script lang="ts">
  /**
   * 鍵帽提示：一顆膠囊鈕，裡面是鍵（kbd）與它的作用。
   * lit 時鍵反白一下，表示自動示範「現在按了這個鍵」；點下去等於按那個鍵。
   */
  interface Props {
    /** 鍵上的字 */
    cap: string;
    /** 這個鍵做什麼 */
    label: string;
    lit?: boolean;
    disabled?: boolean;
    onclick?: () => void;
  }

  let { cap, label, lit = false, disabled = false, onclick }: Props = $props();
</script>

<button class="btn keycap" class:is-lit={lit} {disabled} {onclick} aria-label={`${label}（按鍵 ${cap}）`}>
  <kbd class="cap">{cap}</kbd>
  <span class="cap-label">{label}</span>
</button>

<style>
  .keycap {
    gap: var(--space-2);
    padding: 0 12px 0 4px;
    background: var(--c-control);
    color: var(--c-text);
    font-weight: 500;
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
