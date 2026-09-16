<script lang="ts">
  interface Props {
    label: string;
    value: number;
    min: number;
    max: number;
    defaultValue: number;
    /** 往右拖時寬度變大（左欄）為 1，變小（右欄）為 -1。 */
    direction: 1 | -1;
    /** 掛在容器的哪一側邊緣；感應區以該邊緣為中心，跨在面板外框上。 */
    side: 'left' | 'right';
    onChange: (value: number) => void;
  }

  let { label, value, min, max, defaultValue, direction, side, onChange }: Props = $props();

  let dragging = $state(false);
  let originX = 0;
  let originValue = 0;

  function clamp(next: number): number {
    return Math.round(Math.min(max, Math.max(min, next)));
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    const target = event.currentTarget as HTMLElement;
    target.setPointerCapture(event.pointerId);
    dragging = true;
    originX = event.clientX;
    originValue = value;
  }

  function onPointerMove(event: PointerEvent) {
    if (!dragging) return;
    onChange(clamp(originValue + (event.clientX - originX) * direction));
  }

  function onPointerUp(event: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  }

  function onKeydown(event: KeyboardEvent) {
    const step = event.shiftKey ? 64 : 16;
    let next: number | null = null;
    if (event.key === 'ArrowLeft') next = value - step * direction;
    else if (event.key === 'ArrowRight') next = value + step * direction;
    else if (event.key === 'Home') next = min;
    else if (event.key === 'End') next = max;
    else if (event.key === 'Enter') next = defaultValue;
    if (next === null) return;
    // 不讓方向鍵冒泡到全域的播放快捷鍵。
    event.preventDefault();
    event.stopPropagation();
    onChange(clamp(next));
  }
</script>

<!-- role="separator" 搭配 aria-valuenow 時是可聚焦、可操作的視窗分隔線（ARIA window splitter）。 -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="handle handle--{side}"
  class:is-dragging={dragging}
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={value}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  title="拖曳調整寬度，雙擊還原"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onkeydown={onKeydown}
  ondblclick={() => onChange(clamp(defaultValue))}
></div>

<style>
  /* 10px 感應區以面板外框為中心，左右各跨 5px；亮起的線正好蓋在外框上。 */
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 5;
    width: 10px;
    cursor: col-resize;
    touch-action: none;
  }

  .handle--right {
    right: -5px;
  }

  .handle--left {
    left: -5px;
  }

  .handle::after {
    content: '';
    position: absolute;
    top: var(--radius-md);
    bottom: var(--radius-md);
    left: 4px;
    width: 2px;
    border-radius: 1px;
    background: transparent;
    transition: background-color 120ms;
  }

  .handle:hover::after {
    background: var(--c-border-strong);
  }

  .handle.is-dragging::after,
  .handle:focus-visible::after {
    background: var(--c-focus);
  }

  .handle:focus-visible {
    outline: none;
  }
</style>
