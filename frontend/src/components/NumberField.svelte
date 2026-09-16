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
    <input
      type="range"
      min={sliderMin ?? min}
      max={sliderMax ?? max}
      {step}
      {value}
      aria-label={`${label} 滑桿`}
      oninput={(event) => commit(event.currentTarget.value)}
    />
  {/if}
  {#if hint}<p class="field-hint">{hint}</p>{/if}
  {#if error}<p class="field-error">{error}</p>{/if}
</div>
