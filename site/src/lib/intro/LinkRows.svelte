<script lang="ts">
  /**
   * 「想知道什麼 → 看哪一頁」的清單（<ul>），裡面放 <LinkRow>。取代兩欄表格，一列一個可點的連結。
   */
  import type { Snippet } from 'svelte';

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

<ul class="link-rows">
  {@render children?.()}
</ul>

<style>
  .link-rows {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: var(--space-4) 0 0;
    padding: 0;
    list-style: none;
  }
</style>
