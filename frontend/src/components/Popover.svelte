<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import { popOut } from '../lib/pop';

  interface Props {
    /** 視窗座標；align='end' 時以右緣對齊 x。 */
    x: number;
    y: number;
    align?: 'start' | 'end';
    label: string;
    width?: number;
    /** focusBack=false 表示因為點到別處而關閉，焦點留在使用者點的地方。 */
    onClose: (focusBack: boolean) => void;
    children: Snippet;
  }

  let { x, y, align = 'start', label, width = 240, onClose, children }: Props = $props();

  let panel: HTMLElement | null = $state(null);
  let left = $state(0);
  let top = $state(0);

  const MARGIN = 8;

  // 開啟時量一次尺寸，貼近視窗邊緣就往內收；聚焦第一個輸入框或按鈕。
  $effect(() => {
    if (!panel) return;
    const rect = panel.getBoundingClientRect();
    const wantLeft = align === 'end' ? x - rect.width : x;
    left = Math.max(MARGIN, Math.min(wantLeft, window.innerWidth - rect.width - MARGIN));
    const below = y + rect.height + MARGIN <= window.innerHeight;
    top = below ? y : Math.max(MARGIN, y - rect.height);
    void tick().then(() => panel?.querySelector<HTMLElement>('[data-autofocus], input, button')?.focus());
  });

  $effect(() => {
    const onPointer = (event: PointerEvent) => {
      if (panel && !panel.contains(event.target as Node)) onClose(false);
    };
    // 面板內的清單可以捲動；只有外面捲動時關閉，否則面板會和觸發它的列錯開。
    const onScroll = (event: Event) => {
      if (panel && !panel.contains(event.target as Node)) onClose(false);
    };
    const dismiss = () => onClose(false);
    document.addEventListener('pointerdown', onPointer, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', dismiss);
    window.addEventListener('blur', dismiss);
    return () => {
      document.removeEventListener('pointerdown', onPointer, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', dismiss);
      window.removeEventListener('blur', dismiss);
    };
  });

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onClose(true);
    }
  }
</script>

<div
  bind:this={panel}
  class="popover pop"
  role="dialog"
  aria-label={label}
  tabindex="-1"
  style={`left:${left}px;top:${top}px;width:${width}px`}
  onkeydown={onKeydown}
  oncontextmenu={(event) => event.preventDefault()}
  out:popOut|global
>
  {@render children()}
</div>

<style>
  .popover {
    position: fixed;
    z-index: 50;
    max-height: min(420px, calc(100vh - 16px));
  }
</style>
