<script lang="ts" module>
  import type { IconName } from './Icon.svelte';

  export interface MenuItem {
    id: string;
    label: string;
    icon: IconName;
    danger?: boolean;
    disabled?: boolean;
    /** 在這個項目上方畫一條分隔線 */
    divider?: boolean;
  }
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import { popOut } from '../lib/pop';

  interface Props {
    items: MenuItem[];
    /** 視窗座標：右鍵是游標位置，三點按鈕是按鈕右下角（align='end' 時以右緣對齊）。 */
    x: number;
    y: number;
    align?: 'start' | 'end';
    label: string;
    onSelect: (id: string) => void;
    /** focusBack=false 表示因為點到別處而關閉，焦點留在使用者點的地方。 */
    onClose: (focusBack: boolean) => void;
  }

  let { items, x, y, align = 'start', label, onSelect, onClose }: Props = $props();

  let menu: HTMLElement | null = $state(null);
  let left = $state(0);
  let top = $state(0);

  const MARGIN = 8;

  function buttons(): HTMLButtonElement[] {
    return menu ? [...menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')] : [];
  }

  // 開啟時量一次尺寸，貼近視窗邊緣就往內收；聚焦第一個項目。
  $effect(() => {
    if (!menu) return;
    const rect = menu.getBoundingClientRect();
    const wantLeft = align === 'end' ? x - rect.width : x;
    left = Math.max(MARGIN, Math.min(wantLeft, window.innerWidth - rect.width - MARGIN));
    const below = y + rect.height + MARGIN <= window.innerHeight;
    top = below ? y : Math.max(MARGIN, y - rect.height);
    void tick().then(() => buttons()[0]?.focus());
  });

  $effect(() => {
    const onPointer = (event: PointerEvent) => {
      if (menu && !menu.contains(event.target as Node)) onClose(false);
    };
    const dismiss = () => onClose(false);
    // capture：清單本身的捲動也要關閉選單，否則選單會和紀錄列錯開。
    document.addEventListener('pointerdown', onPointer, true);
    window.addEventListener('scroll', dismiss, true);
    window.addEventListener('resize', dismiss);
    window.addEventListener('blur', dismiss);
    return () => {
      document.removeEventListener('pointerdown', onPointer, true);
      window.removeEventListener('scroll', dismiss, true);
      window.removeEventListener('resize', dismiss);
      window.removeEventListener('blur', dismiss);
    };
  });

  function onKeydown(event: KeyboardEvent) {
    const list = buttons();
    const index = list.indexOf(document.activeElement as HTMLButtonElement);
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        list[(index + 1) % list.length]?.focus();
        break;
      case 'ArrowUp':
        event.preventDefault();
        list[(index - 1 + list.length) % list.length]?.focus();
        break;
      case 'Home':
        event.preventDefault();
        list[0]?.focus();
        break;
      case 'End':
        event.preventDefault();
        list[list.length - 1]?.focus();
        break;
      case 'Escape':
        event.preventDefault();
        event.stopPropagation();
        onClose(true);
        break;
      case 'Tab':
        event.preventDefault();
        onClose(true);
        break;
      default:
        break;
    }
  }
</script>

<div
  bind:this={menu}
  class="menu pop"
  role="menu"
  tabindex="-1"
  aria-label={label}
  style={`left:${left}px;top:${top}px`}
  onkeydown={onKeydown}
  oncontextmenu={(event) => event.preventDefault()}
  out:popOut|global
>
  {#each items as item, index (item.id)}
    {#if item.divider}
      <div class="pop-divider" role="separator"></div>
    {/if}
    <button
      class="pop-item"
      class:is-danger={item.danger}
      style={`--i:${index}`}
      role="menuitem"
      disabled={item.disabled}
      onclick={() => onSelect(item.id)}
    >
      <Icon name={item.icon} size={14} />
      {item.label}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 50;
    min-width: 184px;
  }

  .pop-item.is-danger {
    color: var(--c-danger);
  }

  .pop-item.is-danger:hover:not(:disabled),
  .pop-item.is-danger:focus-visible {
    background: var(--c-danger-bg);
  }

  .pop-item :global(.icon) {
    color: var(--c-text-dim);
  }

  .pop-item.is-danger :global(.icon) {
    color: inherit;
  }
</style>
