<script lang="ts">
  import Icon from './Icon.svelte';
  import NumberField from './NumberField.svelte';
  import { IMAGE_HEIGHT, IMAGE_WIDTH } from '../lib/disc';
  import { PALM_APPROX_HINT } from '../lib/palm';
  import {
    TOUCH_AREA_PLACE,
    touchHoldFrame,
    touchHoldRingRadius,
    touchPetals,
    touchPolygon,
  } from '../lib/touch';
  import { view } from '../state/view.svelte';
  import type { SensorDisplayMode } from '../state/view.svelte';

  const SENSOR_MODES: { id: SensorDisplayMode; label: string }[] = [
    { id: 'auto', label: '有 Touch 時顯示' },
    { id: 'all', label: '一律顯示' },
    { id: 'off', label: '關閉' },
  ];

  interface LegendItem {
    key: string;
    area: string;
    name: string;
    hint: string;
    /** true = 畫音符外形，false = 畫落點參考的多邊形 */
    note?: boolean;
    hold?: boolean;
    spark?: boolean;
  }

  const LEGEND: LegendItem[] = [
    { key: 'A', area: 'A', name: 'A1–A8', hint: TOUCH_AREA_PLACE.A },
    { key: 'B', area: 'B', name: 'B1–B8', hint: TOUCH_AREA_PLACE.B },
    { key: 'C', area: 'C', name: 'C', hint: '中央；C1／C2 在核心合併為同一個 C' },
    { key: 'D', area: 'D', name: 'D1–D8', hint: TOUCH_AREA_PLACE.D },
    { key: 'E', area: 'E', name: 'E1–E8', hint: TOUCH_AREA_PLACE.E },
    {
      key: 'touch',
      area: 'A',
      name: 'Touch',
      hint: '四片三角形由外往落點收攏，收到位就是判定時間',
      note: true,
    },
    {
      key: 'hold',
      area: 'A',
      name: 'Touch Hold',
      hint: '方框＋斜向三角形；外面一整圈亮環順時針消耗，代表剩餘時間',
      note: true,
      hold: true,
    },
    {
      key: 'fireworks',
      area: 'A',
      name: '煙火 f',
      hint: '虛線外框預告，判定時間在落點擴散一次',
      note: true,
      spark: true,
    },
  ];

  const LEGEND_ORIGIN = { x: 0, y: 0 };
  /** 圖例的落點外形半徑（viewBox 單位），對應盤面的 touchRadius。 */
  const LEGEND_RADIUS = 11;

  /** 圖例外形與盤面共用 touchPolygon；這裡把「朝外」畫成朝上。 */
  function legendPath(area: string, radius: number): string {
    return legendPoly(touchPolygon(area, LEGEND_ORIGIN, radius, -Math.PI / 2));
  }

  /** 把圖例用的局部座標搬到 40×40 viewBox 的中心。 */
  function legendPoly(points: { x: number; y: number }[]): string {
    return `${points
      .map((p, i) => `${i === 0 ? 'M' : 'L'}${(20 + p.x).toFixed(2)} ${(20 + p.y).toFixed(2)}`)
      .join('')}Z`;
  }

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
        <span>顯示左手 L（粉紅）</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showRight} />
        <span>顯示右手 R（藍）</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showRecentTrail} />
        <span>顯示最近軌跡</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showFullTrack} />
        <span>
          顯示整段軌跡
          <span class="field-hint">長譜面會在盤面留下大量細線。</span>
        </span>
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
        <input type="checkbox" bind:checked={view.showPalms} />
        <span>
          顯示手掌覆蓋區
          <span class="field-hint">
            核心判定為一掌覆蓋時才會出現；畫在音符與手軌跡下層，覺得擋到就關掉。
          </span>
        </span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showButtons} />
        <span>顯示 1–8 鍵位編號</span>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={view.showFireworks} />
        <span>
          顯示煙火效果
          <span class="field-hint">時間完全跟著播放時鐘，拖曳到同一時間畫面相同。</span>
        </span>
      </label>
    </div>

    <div style="margin-top: var(--space-4)">
      <div class="field-label" id="sensor-mode-label">Touch 落點標記</div>
      <p class="field-hint" style="margin: 2px 0 var(--space-2)">
        核心輸出的 33 個 simai 可指名落點；預設關閉，需要對照時再打開。
      </p>
      <div class="row row-wrap" role="group" aria-labelledby="sensor-mode-label">
        {#each SENSOR_MODES as item (item.id)}
          <button
            class="btn"
            class:is-active={view.sensorMode === item.id}
            aria-pressed={view.sensorMode === item.id}
            onclick={() => (view.sensorMode = item.id)}>{item.label}</button
          >
        {/each}
      </div>
      <label class="check" style="margin-top: var(--space-3)">
        <input
          type="checkbox"
          bind:checked={view.showSensorLabels}
          disabled={view.sensorMode === 'off'}
        />
        <span>標記上顯示區域編號</span>
      </label>
    </div>

    <div style="margin-top: var(--space-4)">
      <NumberField
        label="最近軌跡長度"
        hint="往前顯示多少秒的移動。"
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
    <div class="section-title"><span>Touch 區圖例</span></div>
    <p class="field-hint" style="margin-bottom: var(--space-3)">
      位置與編號都取自核心輸出的落點座標。這是 Demo 的可辨識落點，不是實機感應區的精確輪廓。
    </p>
    <div class="legend">
      {#each LEGEND as item (item.key)}
        <svg class="legend-mark" viewBox="0 0 40 40" aria-hidden="true">
          {#if item.note}
            {#if item.spark}
              <path class="mark-spark" d={legendPath(item.area, LEGEND_RADIUS * 1.18)} />
            {/if}
            {#if item.hold}
              <circle
                class="mark-hold-ring"
                cx="20"
                cy="20"
                r={touchHoldRingRadius(LEGEND_RADIUS)}
              />
              <path
                class="mark-hold-frame"
                d={legendPoly(touchHoldFrame(LEGEND_ORIGIN, LEGEND_RADIUS, -Math.PI / 2))}
              />
            {/if}
            {#each touchPetals(LEGEND_ORIGIN, LEGEND_RADIUS, -Math.PI / 2, 1, !!item.hold) as petal, index (index)}
              <path class="mark-petal" d={legendPoly(petal)} />
            {/each}
          {:else}
            <path class="mark-ring" d={legendPath(item.area, LEGEND_RADIUS)} />
            <path class="mark-core" d={legendPath(item.area, 5.5)} />
          {/if}
        </svg>
        <div class="legend-text">
          <span class="legend-name mono">{item.name}</span>
          <span class="field-hint">{item.hint}</span>
        </div>
      {/each}
    </div>
  </section>

  <section class="section">
    <div class="section-title"><span>手掌覆蓋圖例</span></div>
    <div class="legend">
      <svg class="legend-mark" viewBox="0 0 40 40" aria-hidden="true">
        <circle class="mark-palm" cx="20" cy="20" r="15" />
        <path class="mark-palm-center" d="M20 14 V17 M20 23 V26 M14 20 H17 M23 20 H26" />
      </svg>
      <div class="legend-text">
        <span class="legend-name mono">手掌覆蓋區</span>
        <span class="field-hint">
          大圓虛線＋掌心十字，用該手顏色（左手粉紅、右手藍）；覆蓋到的 Touch 外圍再加一圈同色虛線。
        </span>
      </div>
      <svg class="legend-mark" viewBox="0 0 40 40" aria-hidden="true">
        <path class="mark-sensor" d={legendPath('A', 9)} />
      </svg>
      <div class="legend-text">
        <span class="legend-name mono">Touch 落點參考</span>
        <span class="field-hint">小型多邊形、灰色細線，只用來辨識 33 個落點，與覆蓋範圍無關。</span>
      </div>
    </div>
    <p class="field-hint" style="margin-top: var(--space-3)">
      覆蓋範圍、掌心與時間都取自核心輸出，畫面只負責畫出來。{PALM_APPROX_HINT}
    </p>
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
        音符飛入動畫
        <span class="field-hint">
          Tap／Hold／Slide 由中心飛向鍵位，Touch 原地由外往落點收攏。純視覺效果，不改變判定時間。
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
      只調整背景圖對齊，不影響座標與搜尋結果。
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
        hint="佔圖片寬度比例。"
        value={view.calibration.radius}
        min={0.3}
        max={0.6}
        step={0.0005}
        onValue={(value) => setCalibration('radius', value)}
      />
      <NumberField
        label="檢視縮放"
        hint="只放大畫面。"
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
    <button class="btn" style="margin-top: var(--space-3)" onclick={() => view.resetCalibration()}>
      <Icon name="rotate-ccw" />
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
  </section>
</div>

<style>
  .legend {
    display: grid;
    grid-template-columns: 34px 1fr;
    align-items: center;
    gap: var(--space-2) var(--space-3);
  }

  .legend-mark {
    width: 34px;
    height: 34px;
    display: block;
  }

  .legend-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .legend-name {
    font-size: var(--fs-sm);
    font-weight: 700;
  }

  /* 與盤面同一組筆觸，避免圖例和實際畫面長得不一樣。 */
  .mark-ring {
    fill: none;
    stroke: var(--c-text);
    stroke-width: 2.4;
    stroke-linejoin: round;
  }

  .mark-petal,
  .mark-hold-frame {
    fill: none;
    stroke: var(--c-text);
    stroke-width: 1.6;
    stroke-linejoin: round;
  }

  .mark-hold-ring {
    fill: none;
    stroke: var(--c-text-dim);
    stroke-width: 2.4;
  }

  .mark-core {
    fill: none;
    stroke: var(--c-text-dim);
    stroke-width: 1.4;
    stroke-linejoin: round;
  }

  .mark-palm {
    fill: none;
    stroke: var(--c-right);
    stroke-width: 2;
    stroke-dasharray: 6 4;
  }

  .mark-palm-center {
    fill: none;
    stroke: var(--c-right);
    stroke-width: 2;
    stroke-linecap: round;
  }

  /* 落點參考標記的圖例：與盤面同一組灰色細線。 */
  .mark-sensor {
    fill: none;
    stroke: #8d97a6;
    stroke-width: 1.6;
    stroke-linejoin: round;
  }

  .mark-spark {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 1.6;
    stroke-dasharray: 3 4;
    stroke-linejoin: round;
  }
</style>
