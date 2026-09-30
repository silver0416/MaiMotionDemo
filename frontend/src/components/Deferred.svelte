<script lang="ts">
  import type { Snippet } from 'svelte';
  import { afterPaint } from '../lib/frame';
  import SkeletonList from './SkeletonList.svelte';

  /**
   * 晚一個畫面才掛上內容，先畫骨架：需要繪製時間的區塊（例如打法分支圖）
   * 不會讓切換的那一下卡住。
   */
  let { children, cards = 2, label = '載入中' }: { children: Snippet; cards?: number; label?: string } = $props();

  let ready = $state(false);

  $effect(() => afterPaint(() => (ready = true)));
</script>

{#if ready}
  {@render children()}
{:else}
  <SkeletonList {cards} {label} />
{/if}
