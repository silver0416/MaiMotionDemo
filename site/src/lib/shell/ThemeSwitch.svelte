<script lang="ts">
  import { segmented } from '$app/lib/segmented';
  import ExtraIcon, { type ExtraIconName } from './ExtraIcon.svelte';
  import { theme, type ThemeMode } from './theme.svelte';

  /**
   * 外觀切換：跟隨系統／深色／淺色，三選一用 App 的 .seg（滑動反白膠囊）。
   * compact：只顯示圖示（頂列）；否則圖示加文字（手機的選單抽屜）。
   */
  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  const OPTIONS: { id: ThemeMode; label: string; icon: ExtraIconName }[] = [
    { id: 'system', label: '跟隨系統', icon: 'monitor' },
    { id: 'dark', label: '深色', icon: 'moon' },
    { id: 'light', label: '淺色', icon: 'sun' },
  ];
</script>

<div class={['seg', 'theme-switch', compact && 'seg--fit']} role="group" aria-label="外觀" use:segmented>
  {#each OPTIONS as option (option.id)}
    <button
      type="button"
      class="seg-btn"
      aria-pressed={theme.mode === option.id}
      aria-label={compact ? option.label : undefined}
      title={compact ? option.label : undefined}
      onclick={() => theme.set(option.id)}
    >
      <ExtraIcon name={option.icon} />
      {#if !compact}<span>{option.label}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .theme-switch.seg--fit :global(.seg-btn) {
    width: 32px;
    padding: 0;
  }

  @media (pointer: coarse) {
    .theme-switch.seg--fit :global(.seg-btn) {
      width: 40px;
    }
  }
</style>
