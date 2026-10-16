<script lang="ts" generics="T extends string">
  import Icon, { type IconName } from './Icon.svelte';
  import { reducedMotion } from '../lib/press';

  /**
   * 膠囊下拉選單（Claude Design「互動動畫」2a）。
   * 開啟：淡入 140ms、下移 4px 並由 0.97 放大到 1（180ms，原點右上）；選項逐一延遲 15ms、起始 40ms。
   * 收合：100ms ease-in，整塊一起淡出，不錯開。
   * 指示膠囊停在目前選項，滑鼠經過或方向鍵移動時滑過去（140ms）；選中項的勾選線條延遲 120ms 畫出。
   * 選單用 fixed 定位，不會被外層面板的 overflow 裁掉。
   */
  interface Props {
    options: readonly { id: T; label: string }[];
    value: T;
    onChange: (value: T) => void;
    /** 給輔助技術的名稱。 */
    label: string;
    icon?: IconName;
    title?: string;
    /** 選單對齊按鈕的哪一側。 */
    align?: 'start' | 'end';
  }

  let { options, value, onChange, label, icon, title, align = 'end' }: Props = $props();

  const ITEM_HEIGHT = 32;
  const ITEM_GAP = 2;
  const PAD = 6;

  let open = $state(false);
  /** 鍵盤或滑鼠目前指著的選項；null 表示指示膠囊停在選中項。 */
  let hover = $state<number | null>(null);
  let button: HTMLButtonElement | null = $state(null);
  let menu: HTMLElement | null = $state(null);
  let position = $state({ left: 0, top: 0 });
  /** 每次開啟都換一個值，讓選項的進場動畫重播。 */
  let openCount = $state(0);

  const selectedIndex = $derived(Math.max(0, options.findIndex((option) => option.id === value)));
  const indicatorIndex = $derived(hover ?? selectedIndex);
  const current = $derived(options[selectedIndex]?.label ?? '');
  const menuId = `dropdown-${Math.random().toString(36).slice(2, 9)}`;

  function place() {
    if (!button) return;
    const rect = button.getBoundingClientRect();
    const width = Math.max(168, menu?.offsetWidth ?? 168);
    const height = options.length * (ITEM_HEIGHT + ITEM_GAP) + PAD * 2;
    const margin = 8;
    let left = align === 'end' ? rect.right - width : rect.left;
    left = Math.max(margin, Math.min(left, window.innerWidth - width - margin));
    let top = rect.bottom + 6;
    if (top + height > window.innerHeight - margin) top = Math.max(margin, rect.top - 6 - height);
    position = { left, top };
  }

  function show() {
    place();
    hover = null;
    openCount += 1;
    open = true;
  }

  function hide(focusButton = false) {
    open = false;
    hover = null;
    if (focusButton) button?.focus();
  }

  function pick(index: number) {
    const option = options[index];
    if (option && option.id !== value) onChange(option.id);
    hide(true);
  }

  $effect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Node;
      if (menu?.contains(target) || button?.contains(target)) return;
      hide();
    };
    const onScroll = (event: Event) => {
      if (menu?.contains(event.target as Node)) return;
      hide();
    };
    const dismiss = () => hide();
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
    if (!open) {
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        show();
      }
      return;
    }
    const at = hover ?? selectedIndex;
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        hover = Math.min(options.length - 1, at + 1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        hover = Math.max(0, at - 1);
        break;
      case 'Home':
        event.preventDefault();
        hover = 0;
        break;
      case 'End':
        event.preventDefault();
        hover = options.length - 1;
        break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        pick(at);
        break;
      case 'Escape':
        event.preventDefault();
        event.stopPropagation();
        hide(true);
        break;
      case 'Tab':
        hide();
        break;
      default:
        break;
    }
  }

  const motion = $derived(!reducedMotion());
</script>

<span class="dropdown">
  <button
    bind:this={button}
    class="trigger"
    class:is-open={open}
    type="button"
    role="combobox"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={menuId}
    aria-activedescendant={open ? `${menuId}-${indicatorIndex}` : undefined}
    aria-label={`${label}：${current}`}
    {title}
    onclick={() => (open ? hide() : show())}
    onkeydown={onKeydown}
  >
    {#if icon}<Icon name={icon} size={13} />{/if}
    <span class="trigger-label">{current}</span>
    <span class="chevron"><Icon name="chevron-down" size={14} /></span>
  </button>

  <div
    bind:this={menu}
    id={menuId}
    class="menu"
    class:is-open={open}
    class:no-motion={!motion}
    role="listbox"
    tabindex="-1"
    aria-label={label}
    style={`left:${position.left}px;top:${position.top}px`}
    onpointerleave={() => (hover = null)}
  >
    <span
      class="indicator"
      class:is-hover={hover !== null && hover !== selectedIndex}
      style={`top:${PAD + indicatorIndex * (ITEM_HEIGHT + ITEM_GAP)}px`}
      aria-hidden="true"
    ></span>
    {#key openCount}
      {#each options as option, index (option.id)}
        <button
          id={`${menuId}-${index}`}
          class="item"
          class:is-selected={index === selectedIndex}
          type="button"
          role="option"
          tabindex="-1"
          aria-selected={index === selectedIndex}
          style={`--i:${index}`}
          onpointerenter={() => (hover = index)}
          onclick={() => pick(index)}
        >
          <span class="item-label">{option.label}</span>
          {#if index === selectedIndex}
            <svg class="tick check-draw" width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M20 6 9 17l-5-5" />
            </svg>
          {/if}
        </button>
      {/each}
    {/key}
  </div>
</span>

<style>
  .dropdown {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 32px;
    padding: 0 10px 0 12px;
    border: none;
    border-radius: var(--radius-pill);
    background: var(--c-control);
    color: var(--c-text);
    font-size: var(--fs-sm);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .trigger:hover,
  .trigger.is-open {
    background: var(--c-control-hover);
    color: var(--c-text-strong);
  }

  .trigger-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chevron {
    display: inline-flex;
    color: var(--c-text-dim);
    transition: transform 200ms var(--ease-out);
  }

  .is-open .chevron {
    transform: rotate(180deg);
    color: inherit;
  }

  .menu {
    position: fixed;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 168px;
    padding: 6px;
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-pop);
    box-shadow: var(--shadow-pop);
    transform-origin: top right;
    visibility: hidden;
    pointer-events: none;
    opacity: 0;
    transform: translateY(-4px) scale(0.97);
    transition:
      opacity 100ms ease-in,
      transform 100ms ease-in,
      visibility 0ms linear 100ms;
  }

  .menu.is-open {
    visibility: visible;
    pointer-events: auto;
    opacity: 1;
    transform: none;
    transition:
      opacity 140ms ease-out,
      transform 180ms var(--ease-out),
      visibility 0ms;
  }

  .indicator {
    position: absolute;
    left: 6px;
    right: 6px;
    height: 32px;
    border-radius: var(--radius-pill);
    background: var(--c-control-hover);
    transition:
      top 140ms var(--ease-out),
      background-color 120ms ease;
  }

  .indicator.is-hover {
    background: var(--c-control);
  }

  .item {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 32px;
    padding: 0 12px;
    border: none;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--c-text);
    font-size: var(--fs-sm);
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
    opacity: 0;
    transform: translateY(-3px);
    transition:
      opacity 80ms ease-in,
      transform 80ms ease-in;
  }

  .menu.is-open .item {
    opacity: 1;
    transform: none;
    transition:
      opacity 160ms ease-out calc(40ms + var(--i) * 15ms),
      transform 200ms var(--ease-out) calc(40ms + var(--i) * 15ms);
  }

  .item.is-selected {
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .tick {
    flex: none;
    fill: none;
    stroke: var(--c-primary);
    stroke-width: 2.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .menu.no-motion,
  .menu.no-motion .item,
  .menu.no-motion .indicator,
  .no-motion .tick path {
    transition: none;
    animation: none;
    stroke-dashoffset: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .chevron,
    .trigger {
      transition: none;
    }
  }
</style>
