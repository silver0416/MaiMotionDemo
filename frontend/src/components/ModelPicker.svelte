<script lang="ts">
  import Icon from './Icon.svelte';
  import { reducedMotion } from '../lib/press';
  import {
    MAIN_BASE,
    MAIN_MODELS,
    MORE_BASES,
    MORE_MODELS,
    TEMOTO_COLOR_NAME,
    sameScoring,
    type ScoringChoice,
    type TemotoColor,
    type TemotoModel,
  } from '../lib/contract';
  import type { BaseChoice } from '../lib/types';

  /**
   * 比對頁的模型選單（2026-11-08 使用者指定）：按鈕是膠囊，選項沿用參數頁的評分方式卡，
   * 一律半寬、高度是原本 AI 卡的 2/3（扇形跟著縮成 2/3）。參數頁直接看得到的排上面，
   * 「顯示更多模型」裡的排在分隔線下面，各自的順序同參數頁。
   * 開合動畫與鍵盤操作照 Dropdown：焦點留在按鈕（aria-activedescendant），
   * 左右鍵逐張、上下鍵跨一排、Enter／Space 選定、Esc 關閉。
   * rate 給的暫存吻合率標在卡片左上角；標註在那次比對之後改過的加「舊」。
   */
  interface Rate {
    percent: number | null;
    outdated: boolean;
  }

  interface Props {
    value: ScoringChoice;
    onChange: (choice: ScoringChoice) => void;
    /** 每個選項暫存的吻合率；沒比對過回傳 null。 */
    rate?: (choice: ScoringChoice) => Rate | null;
    /** 選單對齊並同寬的元素（例如按鈕所在的那一列）；省略時對齊按鈕。 */
    anchor?: HTMLElement | null;
    /** 給輔助技術的名稱。 */
    label?: string;
  }

  let { value, onChange, rate, anchor = null, label = '比對的模型' }: Props = $props();

  interface Option {
    choice: ScoringChoice;
    name: string;
    /** null 是基座（中性色的卡）。 */
    color: TemotoColor | null;
    /** 給輔助技術的名稱：TemotoAI 另外帶上顏色（畫面上只有系列名稱與顏色）。 */
    spoken: string;
  }

  const baseOption = (item: { id: BaseChoice; label: string }): Option => ({
    choice: { kind: 'base', id: item.id },
    name: item.label,
    color: null,
    spoken: item.label,
  });
  const modelOption = (item: TemotoModel): Option => ({
    choice: { kind: 'ml', id: item.id },
    name: item.label,
    color: item.color,
    spoken: `${item.label}（${TEMOTO_COLOR_NAME[item.color]}）`,
  });

  const SHOWN: readonly Option[] = [baseOption(MAIN_BASE), ...MAIN_MODELS.map(modelOption)];
  const MORE: readonly Option[] = [...MORE_BASES.map(baseOption), ...MORE_MODELS.map(modelOption)];
  const OPTIONS: readonly Option[] = [...SHOWN, ...MORE];
  const COLUMNS = 2;
  /** 每張卡在畫面上的列與欄：分隔線下面從新的一列開始。 */
  const CELLS = OPTIONS.map((_, index) => {
    const shown = index < SHOWN.length;
    const at = shown ? index : index - SHOWN.length;
    return {
      row: (shown ? 0 : Math.ceil(SHOWN.length / COLUMNS)) + Math.floor(at / COLUMNS),
      col: at % COLUMNS,
    };
  });

  const MARGIN = 8;
  const GAP = 6;
  const MIN_WIDTH = 280;

  let open = $state(false);
  /** 鍵盤或滑鼠指著的選項；null 表示停在選中項。 */
  let hover = $state<number | null>(null);
  /** 用方向鍵移動時才畫焦點框；滑鼠一動就收起來。 */
  let keyboard = $state(false);
  let button: HTMLButtonElement | null = $state(null);
  let menu: HTMLElement | null = $state(null);
  let position = $state({ left: 0, top: 0, width: 320, maxHeight: 420, above: false });
  /** 每次開啟都換一個值，讓卡片的進場動畫重播。 */
  let openCount = $state(0);

  const selected = $derived(OPTIONS.findIndex((option) => sameScoring(option.choice, value)));
  const active = $derived(hover ?? Math.max(0, selected));
  const current = $derived(selected >= 0 ? OPTIONS[selected] : null);
  const menuId = `model-picker-${Math.random().toString(36).slice(2, 9)}`;
  const motion = $derived(!reducedMotion());

  function dotStyle(color: TemotoColor | null | undefined): string | undefined {
    return color ? `--dot: var(--c-${color}-deep)` : undefined;
  }

  function rateText(info: Rate): string {
    return `${info.outdated ? '舊 ' : ''}${info.percent === null ? '—' : `${info.percent}%`}`;
  }

  function spokenOf(option: Option, info: Rate | null): string {
    if (!info) return option.spoken;
    const rateWords = info.percent === null ? '沒有可比對的步驟' : `暫存吻合 ${info.percent}%`;
    return `${option.spoken}，${rateWords}${info.outdated ? '，標註之後改過' : ''}`;
  }

  /** 選單和 anchor 左緣對齊、同寬；下方放不下且上方比較寬時改開在上方，再放不下就捲動。 */
  function place() {
    if (!button) return;
    const trigger = button.getBoundingClientRect();
    const frame = (anchor ?? button).getBoundingClientRect();
    const width = Math.min(Math.max(frame.width, MIN_WIDTH), window.innerWidth - MARGIN * 2);
    const left = Math.max(MARGIN, Math.min(frame.left, window.innerWidth - width - MARGIN));
    const natural = menu?.scrollHeight ?? 0;
    const below = window.innerHeight - trigger.bottom - GAP - MARGIN;
    const above = trigger.top - GAP - MARGIN;
    if (natural <= below || below >= above) {
      position = { left, top: trigger.bottom + GAP, width, maxHeight: Math.max(below, 120), above: false };
    } else {
      const height = Math.min(natural, above);
      position = { left, top: trigger.top - GAP - height, width, maxHeight: above, above: true };
    }
  }

  function show() {
    place();
    hover = null;
    keyboard = false;
    openCount += 1;
    open = true;
    // 選中的卡在捲動範圍外時（視窗很矮）捲到看得到的地方。
    requestAnimationFrame(() => {
      if (open) document.getElementById(`${menuId}-${active}`)?.scrollIntoView({ block: 'nearest' });
    });
  }

  function hide(focusButton = false) {
    open = false;
    hover = null;
    keyboard = false;
    if (focusButton) button?.focus();
  }

  function pick(index: number) {
    const option = OPTIONS[index];
    hide(true);
    if (option && !sameScoring(option.choice, value)) onChange(option.choice);
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

  /** 上下鍵：移到上一列或下一列的同一欄；那一列沒有這一欄就停在那一列最後一張。 */
  function vertical(from: number, step: 1 | -1): number {
    const { row, col } = CELLS[from];
    let target = from;
    for (let index = 0; index < CELLS.length; index += 1) {
      if (CELLS[index].row !== row + step) continue;
      target = index;
      if (CELLS[index].col === col) break;
    }
    return target;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (!open) {
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp' || event.key === 'Enter' || event.key === ' ') {
        // 自己開選單，不讓全域快捷鍵（空白鍵播放、方向鍵移動時間）接走。
        event.preventDefault();
        event.stopPropagation();
        show();
      }
      return;
    }
    if (event.key === 'Tab') {
      hide();
      return;
    }
    // 選單開著時按鍵都歸選單，標註與播放的快捷鍵不作用。
    event.stopPropagation();
    let next: number;
    switch (event.key) {
      case 'ArrowRight':
        next = Math.min(OPTIONS.length - 1, active + 1);
        break;
      case 'ArrowLeft':
        next = Math.max(0, active - 1);
        break;
      case 'ArrowDown':
        next = vertical(active, 1);
        break;
      case 'ArrowUp':
        next = vertical(active, -1);
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = OPTIONS.length - 1;
        break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        pick(active);
        return;
      case 'Escape':
        event.preventDefault();
        hide(true);
        return;
      default:
        return;
    }
    event.preventDefault();
    hover = next;
    keyboard = true;
    document.getElementById(`${menuId}-${next}`)?.scrollIntoView({ block: 'nearest' });
  }
</script>

{#snippet cards(list: readonly Option[], start: number, name: string)}
  <div class="grid" role="group" aria-label={name}>
    {#each list as option, offset (option.choice.id)}
      {@const index = start + offset}
      {@const info = rate?.(option.choice) ?? null}
      <button
        id={`${menuId}-${index}`}
        class={`model-card model-card--${option.color ?? 'v3'} card`}
        class:is-active={index === selected}
        class:is-current={keyboard && index === active}
        type="button"
        role="option"
        tabindex="-1"
        aria-selected={index === selected}
        aria-label={spokenOf(option, info)}
        style={`--i:${index}`}
        onpointerenter={() => {
          hover = index;
          keyboard = false;
        }}
        onclick={() => pick(index)}
      >
        {#if info}
          <span class="rate mono" class:is-old={info.outdated} aria-hidden="true">{rateText(info)}</span>
        {/if}
        <span class="model-card-name">{option.name}</span>
      </button>
    {/each}
  </div>
{/snippet}

<span class="picker">
  <button
    bind:this={button}
    class="trigger"
    class:is-open={open}
    type="button"
    role="combobox"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={menuId}
    aria-activedescendant={open ? `${menuId}-${active}` : undefined}
    aria-label={`${label}：${current?.spoken ?? '未選'}`}
    onclick={() => (open ? hide() : show())}
    onkeydown={onKeydown}
  >
    <span class="dot" style={dotStyle(current?.color)} aria-hidden="true"></span>
    <span class="trigger-label">{current?.name ?? '選擇模型'}</span>
    <span class="chevron"><Icon name="chevron-down" size={14} /></span>
  </button>

  <div
    bind:this={menu}
    id={menuId}
    class="menu"
    class:is-open={open}
    class:is-above={position.above}
    class:no-motion={!motion}
    role="listbox"
    tabindex="-1"
    aria-label={label}
    style={`left:${position.left}px;top:${position.top}px;width:${position.width}px;max-height:${position.maxHeight}px`}
    onpointerleave={() => (hover = null)}
  >
    {#key openCount}
      {@render cards(SHOWN, 0, '主要模型')}
      <div class="pop-divider" aria-hidden="true"></div>
      {@render cards(MORE, SHOWN.length, '更多模型')}
    {/key}
  </div>
</span>

<style>
  .picker {
    display: inline-flex;
    min-width: 0;
  }

  /* 按鈕照 Dropdown 的膠囊，名稱前多一顆模型色的圓點（基座是中性色）。 */
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
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
      color 120ms ease,
      transform 90ms ease-out;
  }

  .trigger:hover,
  .trigger.is-open {
    background: var(--c-control-hover);
    color: var(--c-text-strong);
  }

  .trigger:active {
    transform: scale(0.96);
  }

  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--dot, var(--c-border-strong));
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

  /* 選單：.pop 的面板樣式；一直掛著、用 visibility 開合，和 Dropdown 同一套動畫。 */
  .menu {
    position: fixed;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    overflow-x: hidden;
    overflow-y: auto;
    overscroll-behavior: contain;
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-pop);
    box-shadow: var(--shadow-pop);
    transform-origin: top left;
    visibility: hidden;
    pointer-events: none;
    opacity: 0;
    transform: translateY(-4px) scale(0.97);
    transition:
      opacity 100ms ease-in,
      transform 100ms ease-in,
      visibility 0ms linear 100ms;
  }

  .menu.is-above {
    transform-origin: bottom left;
    transform: translateY(4px) scale(0.97);
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

  .grid {
    flex: none;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px;
  }

  .menu .pop-divider {
    flex: none;
  }

  /* 卡片：參數頁的 .model-card，一律半寬、高度 2/3（72 → 48），名稱用卡片窄時的 16px。 */
  .card {
    min-height: 48px;
    padding: 8px 12px;
    animation: pop-fade 160ms ease-out both;
    animation-delay: calc(40ms + var(--i, 0) * 12ms);
  }

  .card .model-card-name {
    font-size: 16px;
  }

  /* 扇形跟著縮成 2/3：半徑約為卡寬的 20.5%。 */
  .card::before {
    right: -20.5cqi;
    bottom: -20.5cqi;
    width: 41cqi;
    height: 41cqi;
  }

  /* 方向鍵移到的卡：和全域焦點框一樣。 */
  .card.is-current {
    outline: 2px solid var(--c-focus);
    outline-offset: 2px;
  }

  /* 暫存的吻合率：左上角小字，避開右下角的扇形；選中時跟名稱一起換成卡片填滿後的字色。 */
  .rate {
    position: absolute;
    top: 6px;
    left: 12px;
    font-size: var(--fs-xs);
    line-height: 1.2;
    color: var(--c-text);
    white-space: nowrap;
    transition: color 200ms ease 120ms;
  }

  .rate.is-old {
    color: var(--c-text-dim);
  }

  .card.is-active .rate {
    color: var(--on-model);
  }

  .menu.no-motion,
  .menu.no-motion .card,
  .no-motion .rate {
    transition: none;
    animation: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .chevron,
    .trigger,
    .rate {
      transition: none;
    }

    .card {
      animation: none;
    }
  }
</style>
