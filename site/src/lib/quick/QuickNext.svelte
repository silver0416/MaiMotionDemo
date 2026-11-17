<script lang="ts">
  /**
   * 每頁最後的「下一步」：一顆主要按鈕（這一頁唯一的 .btn--primary），文字是 prefix（預設「下一步：」）加上 children。
   * 用法：<QuickNext href="/quick/annotate">開始標記</QuickNext>
   * 最後一頁接到完整說明：<QuickNext href="/annotate/basics" prefix="">想學更多標註的用法</QuickNext>
   */
  import type { Snippet } from 'svelte';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';

  interface Props {
    href: string;
    prefix?: string;
    children: Snippet;
  }

  let { href, prefix = '下一步：', children }: Props = $props();
</script>

<p class="quick-next">
  <a class="btn btn--primary" {href}>{prefix}{@render children()}<ExtraIcon name="arrow-right" /></a>
</p>

<style>
  .quick-next {
    display: flex;
    margin-top: var(--space-5);
  }
</style>
