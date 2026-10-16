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
    onValue,
  }: Props = $props();

  const id = `field-${Math.random().toString(36).slice(2, 9)}`;

  const lo = $derived(sliderMin ?? min);
  const hi = $derived(sliderMax ?? max);
  /** 滑桿已填滿的比例：從軌道左端到圓點中心。 */
  const fill = $derived(hi > lo ? Math.min(1, Math.max(0, (value - lo) / (hi - lo))) : 0);

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
  {/if}
  {#if hint}<p class="field-hint">{hint}</p>{/if}
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
