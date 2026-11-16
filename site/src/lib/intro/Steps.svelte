<script lang="ts">
  /**
   * 有編號的步驟清單（<ol>），裡面放 <Step>。編號由 CSS counter 產生（ol 本身就有順序語意）。
   * - 一般（原則、五步上手）：每步一列，列與列之間 1px 分隔線。
   * - flow：流程圖（如何推算手順）。圖示之間有連接線，每步的內文是一塊面板；
   *   進場時由上往下依序淡入（純 CSS，沒有 JS 也照常顯示；減少動態時不播）。
   *
   * 用法（內文寫在 .md 裡、前後空一行，才會被當成 Markdown，也才會進搜尋索引）：
   *   <Steps flow>
   *   <Step icon="file-text">
   *
   *   **讀入譜面**：……
   *
   *   </Step>
   *   </Steps>
   */
  import type { Snippet } from 'svelte';

  interface Props {
    flow?: boolean;
    children?: Snippet;
  }

  let { flow = false, children }: Props = $props();
</script>

<ol class={['steps', flow && 'steps--flow']}>
  {@render children?.()}
</ol>

<style>
  .steps {
    margin: var(--space-4) 0 0;
    padding: 0;
    list-style: none;
    counter-reset: intro-step;
  }

  .steps--flow {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
