<script lang="ts">
  interface Props {
    label: string;
    hint?: string;
    value: number;
    min: number;
    max: number;
    step: number;
    unit?: string;
    error?: string | null;
    slider?: boolean;
    /** 滑桿的常用範圍；數字欄仍接受 min–max 的完整合法範圍。 */
    sliderMin?: number;
    sliderMax?: number;
    /** 滑桿下方的刻度與數字（例：對應的遊戲速度）；位置和圓點中心對齊。 */
    ticks?: { value: number; label: string }[];
    onValue: (value: number) => void;
  }

  const {
    label,
    hint = '',
    value,
    min,
    max,
    step,
    unit = '',
    error = null,
    slider = true,
    sliderMin,
    sliderMax,
    ticks = [],
    onValue,
  }: Props = $props();

  const id = `field-${Math.random().toString(36).slice(2, 9)}`;

  const lo = $derived(sliderMin ?? min);
  const hi = $derived(sliderMax ?? max);
  /** 某個值在滑桿上的比例 0–1。 */
  function ratio(at: number): number {
    return hi > lo ? Math.min(1, Math.max(0, (at - lo) / (hi - lo))) : 0;
  }

  /** 滑桿已填滿的比例：從軌道左端到圓點中心。 */
  const fill = $derived(ratio(value));

  function commit(raw: string) {
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) return;
    onValue(parsed);
  }
</script>

<div class="field">
  <div class="field-head">
    <label class="field-label" for={id}>{label}</label>
    <span class="row">
      <input
        {id}
        class="input input--narrow mono"
        class:is-invalid={Boolean(error)}
        type="number"
        {min}
        {max}
        {step}
        {value}
        oninput={(event) => commit(event.currentTarget.value)}
      />
      {#if unit}<span class="muted xsmall">{unit}</span>{/if}
    </span>
  </div>
  {#if slider}
    <div class="slider">
      <span class="slider-fill" style={`--p:${fill}`}></span>
      <input
        type="range"
        min={lo}
        max={hi}
        {step}
        {value}
        aria-label={`${label} 滑桿`}
        oninput={(event) => commit(event.currentTarget.value)}
      />
    </div>
    {#if ticks.length > 0}
      <div class="slider-ticks mono" aria-hidden="true">
        {#each ticks as tick (tick.label)}
          <span class="slider-tick" style={`--p:${ratio(tick.value)}`}>{tick.label}</span>
        {/each}
      </div>
    {/if}
  {/if}
  {#if hint}<p class="field-hint help">{hint}</p>{/if}
  {#if error}<p class="field-error">{error}</p>{/if}
</div>

<style>
  /* 填滿的部分疊在原生軌道上；圓點半徑 10px，填滿寬度扣掉圓點的行程才會對到圓點中心。 */
  .slider {
    position: relative;
  }

  .slider-fill {
    position: absolute;
    top: 10px;
    left: 0;
    height: 8px;
    z-index: 1;
    width: calc(10px + (100% - 20px) * var(--p, 0));
    border-radius: var(--radius-pill);
    background: var(--c-primary);
    pointer-events: none;
  }

  .slider input {
    position: relative;
    z-index: 2;
    display: block;
  }

  .slider input::-webkit-slider-runnable-track {
    background: transparent;
  }

  /* 刻度：一條短線＋數字，中心和該值的圓點中心對齊（同 .slider-fill 的換算）。 */
  .slider-ticks {
    position: relative;
    height: 18px;
    margin-top: calc(-1 * var(--space-1));
  }

  .slider-tick {
    position: absolute;
    top: 0;
    left: calc(10px + (100% - 20px) * var(--p, 0));
    transform: translateX(-50%);
    padding-top: 5px;
    font-size: var(--fs-xs);
    line-height: 1;
    color: var(--c-text-dim);
    white-space: nowrap;
  }

  .slider-tick::before {
    content: '';
    position: absolute;
    top: 0;
    left: calc(50% - 0.5px);
    width: 1px;
    height: 3px;
    background: var(--c-border-strong);
  }

  .slider::before {
    content: '';
    position: absolute;
    top: 10px;
    left: 0;
    right: 0;
    height: 8px;
    border-radius: var(--radius-pill);
    background: var(--c-control-hover);
  }
</style>
