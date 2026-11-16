<script lang="ts">
  /**
   * 並排的重點（<ul>），裡面放 <Feature>。寬的時候自動排成兩到三欄，窄的時候一欄。
   * 用法同 <Steps>：內文寫在 .md 裡、前後空一行。
   */
  import type { Snippet } from 'svelte';

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

<ul class="features">
  {@render children?.()}
</ul>

<style>
  .features {
    /* Feature 依這個寬度決定圖示放上面（多欄）還是左邊（一欄）。 */
    container-type: inline-size;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: var(--space-3);
    margin: var(--space-4) 0 0;
    padding: 0;
    list-style: none;
  }
</style>
