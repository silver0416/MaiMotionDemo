<script lang="ts">
  import NumberField from './NumberField.svelte';
  import { DEFAULT_CALIBRATION, IMAGE_HEIGHT, IMAGE_WIDTH } from '../lib/disc';
  import { view } from '../state/view.svelte';

  function setCalibration(key: 'centerX' | 'centerY' | 'radius', value: number) {
    view.calibration = { ...view.calibration, [key]: value };
  }

  const pixels = $derived({
    x: view.calibration.centerX * IMAGE_WIDTH,
    y: view.calibration.centerY * IMAGE_HEIGHT,
    r: view.calibration.radius * IMAGE_WIDTH,
  });
</script>

<div class="stack">
  <section class="section">
    <div class="section-title"><span>顯示內容</span></div>
    <div class="stack-sm">
      <label class="check">
        <input type="checkbox" bind:checked={view.showNotes} />
        <span>顯示譜面音符</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showLeft} />
        <span>顯示左手 L（實線・圓形）</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showRight} />
        <span>顯示右手 R（虛線・方形）</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showFullTrack} />
        <span>顯示整段軌跡</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showRecentTrail} />
        <span>顯示最近軌跡</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showAssignmentTags} />
        <span>在音符旁標出 L／R</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showHandovers} />
        <span>顯示換手標記</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showButtons} />
        <span>顯示 1–8 鍵位編號</span>
      </label>
    </div>

    <div style="margin-top: var(--space-4)">
      <NumberField
        label="最近軌跡長度"
        hint="顯示目前時間往前多少秒的移動。"
        value={view.trailSeconds}
        min={0.2}
        max={4}
        step={0.1}
        unit="秒"
        onValue={(value) => (view.trailSeconds = value)}
      />
    </div>
  </section>

  <section class="section">
    <div class="section-title"><span>音符呈現</span></div>
    <div class="row row-wrap" role="group" aria-label="音符顯示方式">
      <button
        class="btn"
        class:is-active={view.noteMode === 'window'}
        aria-pressed={view.noteMode === 'window'}
        onclick={() => (view.noteMode = 'window')}>依時間出現</button
      >
      <button
        class="btn"
        class:is-active={view.noteMode === 'all'}
        aria-pressed={view.noteMode === 'all'}
        onclick={() => (view.noteMode = 'all')}>全部同時顯示</button
      >
    </div>
    <label class="check" style="margin-top: var(--space-3)">
      <input type="checkbox" bind:checked={view.approach} disabled={view.noteMode !== 'window'} />
      <span>
        音符由中心飛向鍵位
        <span class="field-hint">
          純視覺效果，不改變判定時間；分析與軌跡一律使用 Rust 回傳的秒數。
        </span>
      </span>
    </label>
    {#if view.noteMode === 'window' && view.approach}
      <div style="margin-top: var(--space-3)">
        <NumberField
          label="飛入時間"
          hint="音符提前多久出現。"
          value={view.approachSeconds}
          min={0.2}
          max={2}
          step={0.05}
          unit="秒"
          onValue={(value) => (view.approachSeconds = value)}
        />
      </div>
    {/if}
  </section>

  <section class="section">
    <div class="section-title"><span>盤面校準與縮放</span></div>
    <p class="field-hint" style="margin-bottom: var(--space-3)">
      音符與手的位置一律由 Rust 的盤面座標換算（中心 0,0、半徑 1）。
      這裡只是把背景圖片對齊到同一個圓，不會改變任何座標或搜尋結果。
      預設值由 resource/maimai.png 的外圈以最小平方圓擬合得到。
    </p>

    <label class="check" style="margin-bottom: var(--space-3)">
      <input type="checkbox" bind:checked={view.showCalibrationRing} />
      <span>顯示校準圓環與十字線</span>
    </label>

    <div class="stack">
      <NumberField
        label="圓心 X"
        hint="佔圖片寬度比例。"
        value={view.calibration.centerX}
        min={0.3}
        max={0.7}
        step={0.0005}
        onValue={(value) => setCalibration('centerX', value)}
      />
      <NumberField
        label="圓心 Y"
        hint="佔圖片高度比例。"
        value={view.calibration.centerY}
        min={0.3}
        max={0.7}
        step={0.0005}
        onValue={(value) => setCalibration('centerY', value)}
      />
      <NumberField
        label="盤面半徑"
        hint="佔圖片寬度比例，對應盤面座標的 1.0。"
        value={view.calibration.radius}
        min={0.3}
        max={0.6}
        step={0.0005}
        onValue={(value) => setCalibration('radius', value)}
      />
      <NumberField
        label="檢視縮放"
        hint="只放大畫面，不影響座標。"
        value={view.zoom}
        min={0.6}
        max={2}
        step={0.05}
        unit="×"
        onValue={(value) => (view.zoom = value)}
      />
    </div>

    <p class="xsmall muted mono" style="margin-top: var(--space-3)">
      圓心 {pixels.x.toFixed(1)}, {pixels.y.toFixed(1)} px・半徑 {pixels.r.toFixed(1)} px
      （圖片 {IMAGE_WIDTH}×{IMAGE_HEIGHT}）
    </p>
    <p class="xsmall muted">
      預設：{DEFAULT_CALIBRATION.centerX} / {DEFAULT_CALIBRATION.centerY} / {DEFAULT_CALIBRATION.radius}
    </p>
    <button class="btn" style="margin-top: var(--space-3)" onclick={() => view.resetCalibration()}>
      還原校準與縮放
    </button>
  </section>

  <section class="section">
    <div class="section-title"><span>鍵盤操作</span></div>
    <dl class="kv">
      <dt>空白鍵</dt>
      <dd>播放／暫停</dd>
      <dt>← →</dt>
      <dd>後退／前進 0.1 秒</dd>
      <dt>Shift + ← →</dt>
      <dd>後退／前進 1 秒</dd>
      <dt>Home</dt>
      <dd>回到起點</dd>
      <dt>L</dt>
      <dd>切換循環片段</dd>
    </dl>
    <p class="field-hint" style="margin-top: var(--space-2)">在輸入框中打字時不會觸發這些快捷鍵。</p>
  </section>
</div>
