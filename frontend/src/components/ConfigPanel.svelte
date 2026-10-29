<script lang="ts">
  import Icon from './Icon.svelte';
  import NumberField from './NumberField.svelte';
  import {
    ADVANCED_FIELDS,
    CONFIG_RANGE,
    DEFAULT_BASE,
    FIELD_LABEL,
    MAIN_BASE,
    MAIN_MODELS,
    MORE_BASES,
    MORE_MODELS,
    TEMOTO_LABEL,
    type TemotoModel,
  } from '../lib/contract';
  import { PALM_APPROX_HINT } from '../lib/palm';
  import { session } from '../state/session.svelte';
  import type { ConfigDraft, V3PreferenceControls } from '../lib/types';

  const issues = $derived(session.configIssues);

  function errorOf(field: string): string | null {
    return issues.find((issue) => issue.field === field)?.message ?? null;
  }

  function set<K extends keyof ConfigDraft>(key: K, value: ConfigDraft[K]) {
    session.config = { ...session.config, [key]: value };
  }

  /** 四個行為控制存在草稿的 v3 底下（沿用既有保存資料的位置）。 */
  function setV3<K extends keyof V3PreferenceControls>(key: K, value: V3PreferenceControls[K]) {
    session.config = { ...session.config, v3: { ...session.config.v3, [key]: value } };
  }

  const config = $derived(session.config);
  const R = CONFIG_RANGE;

  /** TemotoAI-1 的卡畫面上只有名稱與顏色（使用者指定）；給輔助技術的名稱另外帶上顏色。 */
  const COLOR_NAME = {
    rurikon: '瑠璃紺',
    ayame: '菖蒲色',
    ruri: '瑠璃色',
    akane: '茜色',
    tokiwa: '常磐色',
  } as const;
  /** 選了 Temoto 時打法偏好由模型決定，四個滑桿收起；規則開關與手掌設定照常可調。 */
  const usingTemoto = $derived(config.mlModel !== null);

  // 原始 V3、V3.1、V3.2 與舊的三個 TemotoAI-1 收在「顯示更多模型」裡；
  // 目前選的是其中之一時自動展開，選中的卡不會藏起來。
  let moreOpen = $state(false);
  const moreLabel = $derived(
    MORE_MODELS.some((model) => model.id === config.mlModel)
      ? TEMOTO_LABEL
      : config.mlModel === null
        ? (MORE_BASES.find((item) => item.id === config.v3Variant)?.label ?? null)
        : null,
  );
  $effect(() => {
    if (moreLabel) moreOpen = true;
  });

  // 進階區預設收起；裡面有欄位超出範圍時自動展開，避免錯誤藏在看不到的地方。
  let advancedOpen = $state(false);
  const advancedIssue = $derived(issues.some((issue) => ADVANCED_FIELDS.has(issue.field)));
  $effect(() => {
    if (advancedIssue) advancedOpen = true;
  });

  // 關閉手掌覆蓋就是半徑 0。記住上一次的半徑，重新打開時不必再輸入一次。
  let lastPalmRadius = $state(DEFAULT_BASE.palmRadius);
  const palmEnabled = $derived(config.palmRadius > 0);

  function setPalmEnabled(enabled: boolean) {
    if (!enabled) {
      if (config.palmRadius > 0) lastPalmRadius = config.palmRadius;
      set('palmRadius', 0);
      return;
    }
    set('palmRadius', lastPalmRadius > 0 ? lastPalmRadius : DEFAULT_BASE.palmRadius);
  }
</script>

{#snippet temotoCard(model: TemotoModel)}
  <button
    class={`model-card model-card--${model.color}`}
    role="radio"
    aria-checked={config.mlModel === model.id}
    aria-label={`${TEMOTO_LABEL}（${COLOR_NAME[model.color]}）`}
    onclick={() => session.setMlModel(model.id)}
  >
    <span class="model-card-name">{TEMOTO_LABEL}</span>
  </button>
{/snippet}

<div class="stack">
  <section class="section">
    <div class="section-title" id="scoring-title">
      <span>評分方式</span>
      {#if session.stale}
        <span class="badge badge--quiet">需重新生成</span>
      {/if}
    </div>
    <div class="models" role="radiogroup" aria-labelledby="scoring-title">
      <button
        class="model-card model-card--v3"
        role="radio"
        aria-checked={config.mlModel === null && config.v3Variant === MAIN_BASE.id}
        onclick={() => session.setV3Variant(MAIN_BASE.id)}
      >
        <span class="model-card-name">{MAIN_BASE.label}</span>
      </button>
      {#each MAIN_MODELS as model (model.id)}
        {@render temotoCard(model)}
      {/each}
    </div>
    <div class="more-row">
      <button
        class="btn btn--ghost more-toggle"
        class:is-open={moreOpen}
        aria-expanded={moreOpen}
        aria-controls="more-models"
        onclick={() => (moreOpen = !moreOpen)}
      >
        {moreOpen ? '收起更多模型' : '顯示更多模型'}
        {#if !moreOpen && moreLabel}
          <span class="more-current">（已選 {moreLabel}）</span>
        {/if}
        <span class="chevron"><Icon name="chevron-down" size={14} /></span>
      </button>
    </div>
    {#if moreOpen}
      <div id="more-models" class="more-models">
        <div class="models" role="radiogroup" aria-label="更多模型">
          {#each MORE_BASES as base (base.id)}
            <button
              class="model-card model-card--v3"
              role="radio"
              aria-checked={config.mlModel === null && config.v3Variant === base.id}
              onclick={() => session.setV3Variant(base.id)}
            >
              <span class="model-card-name">{base.label}</span>
            </button>
          {/each}
          {#each MORE_MODELS as model (model.id)}
            {@render temotoCard(model)}
          {/each}
        </div>
        <p class="field-hint">
          V4 之前的版本與舊的 {TEMOTO_LABEL}，留著方便對照。V3.1、V3.2 只換了打法權重；V4 再多考慮三件事：很快連續的相鄰音符用同一隻手帶過、雙押時兩手不交叉、同時好幾個 Touch 散得太開時兩手分著拍。
        </p>
      </div>
    {/if}
    <p class="field-hint" style="margin-top: var(--space-2)">
      {TEMOTO_LABEL} 要先跑一次基座（上方兩張是 V4）再照模型修正重算，分析時間是基座的數倍；結果仍由核心確認可行，不代表官方判定。
    </p>
  </section>

  <section class="section">
    <div class="section-title">
      <span>{usingTemoto ? '規則' : '打法偏好'}</span>
    </div>
    <div class="stack">
      {#if usingTemoto}
        <p class="field-hint">打法由 {TEMOTO_LABEL} 決定，四個打法偏好滑桿不使用；下面的規則開關照常有效。</p>
      {:else}
      <NumberField
        label="左右分工傾向"
        hint="越高越偏好左手留在左側、右手留在右側。仍允許在較自然時由另一隻手跨區幫忙。"
        value={config.v3.homePreference}
        min={R.homePreference.min}
        max={R.homePreference.max}
        step={1}
        error={errorOf('homePreference')}
        onValue={(value) => setV3('homePreference', value)}
      />
      <NumberField
        label="快速移動容忍"
        hint="超過這個速度才增加高速負擔，並依移動距離累計，越短促越吃力；優先於左右分工。預設 5.2 依真人標註擬合。低於此速度仍計移動距離。"
        value={config.v3.travelComfort}
        min={R.travelComfort.min}
        max={R.travelComfort.max}
        sliderMin={1}
        sliderMax={12}
        step={0.5}
        unit="半徑/秒"
        error={errorOf('travelComfort')}
        onValue={(value) => setV3('travelComfort', value)}
      />
      <NumberField
        label="同點連打容忍"
        hint="越高越能接受同一隻手高速重複敲同一位置。一兩顆一般不會因此被強制換手。"
        value={config.v3.jackTolerance}
        min={R.jackTolerance.min}
        max={R.jackTolerance.max}
        step={1}
        error={errorOf('jackTolerance')}
        onValue={(value) => setV3('jackTolerance', value)}
      />
      <NumberField
        label="Slide 換手意願"
        hint="越高越願意在 Slide 中途交給另一隻手；最高也不會鼓勵反覆換手。"
        value={config.v3.handoverWillingness}
        min={R.handoverWillingness.min}
        max={R.handoverWillingness.max}
        step={1}
        error={errorOf('handoverWillingness')}
        onValue={(value) => setV3('handoverWillingness', value)}
      />
      {/if}
      <label class="check">
        <input
          type="checkbox"
          checked={config.allowHandover}
          onchange={(event) => set('allowHandover', event.currentTarget.checked)}
        />
        <span>
          允許 Slide 中途換手
          <span class="field-hint">關掉後一條 Slide 只能由同一隻手完成。</span>
        </span>
      </label>
      <label class="check">
        <input
          type="checkbox"
          checked={config.slideShortcut}
          onchange={(event) => set('slideShortcut', event.currentTarget.checked)}
        />
        <span>
          Slide 依判定區抄近
          <span class="field-hint">
            打開後手只照判定區走捷徑：可跳過判定區、進最後判定區就離手。預設關閉，手沿星星畫到終點，偷懶只靠手掌同時蓋住多條 Slide 或 Tap。
          </span>
        </span>
      </label>
      <label class="check">
        <input
          type="checkbox"
          checked={config.holdEarlyRelease}
          onchange={(event) => set('holdEarlyRelease', event.currentTarget.checked)}
        />
        <span>
          Hold 可以提早放手
          <span class="field-hint">
            打開後 Hold 結尾 12 幀不必按住、很短的 Hold 只看頭判，手可以先放開去接別的音符。預設關閉，Hold 一律按到結尾。
          </span>
        </span>
      </label>
    </div>
    <p class="field-hint" style="margin-top: var(--space-3)">
      數值可輸入小數。折返與單手集中負荷使用固定的內部係數，不開放調整；
      偏好與預設值是本 Demo 的起點，尚未經玩家校準，不代表官方判定或人體能力。
    </p>
  </section>

  <section class="section">
    <details class="advanced" bind:open={advancedOpen}>
      <summary>
        <span class="chevron"><Icon name="chevron-right" size={14} /></span>
        進階設定
        <span class="muted xsmall">搜尋、時間與手掌覆蓋</span>
        {#if advancedIssue}<span class="badge badge--danger">有欄位超出範圍</span>{/if}
      </summary>

      <div class="advanced-body">
        <div class="stack">
          <div class="group-title">搜尋</div>
          <NumberField
            label="搜尋寬度"
            hint="每個時間點保留多少種可能；越大越仔細也越慢（1–256）。"
            value={config.beamWidth}
            min={R.beamWidth.min}
            max={R.beamWidth.max}
            step={1}
            error={errorOf('beamWidth')}
            onValue={(value) => set('beamWidth', Math.round(value))}
          />
          <NumberField
            label="保留候選方案數"
            hint="最後要比較幾種打法（1–5，且不大於搜尋寬度）。"
            value={config.topK}
            min={R.topK.min}
            max={R.topK.max}
            step={1}
            error={errorOf('topK')}
            onValue={(value) => set('topK', Math.round(value))}
          />
          <NumberField
            label="起始秒數"
            hint="譜面開始前的空白秒數（0–120）。"
            value={session.firstSeconds}
            min={R.firstSeconds.min}
            max={R.firstSeconds.max}
            step={0.1}
            unit="秒"
            error={errorOf('firstSeconds')}
            onValue={(value) => (session.firstSeconds = value)}
          />
        </div>

        <div class="stack">
          <div class="group-title">時間與動作模型</div>
          <NumberField
            label="搜尋取樣間隔"
            hint="Slide 上多久考慮一次換手（0.02–0.2 秒）。"
            value={config.checkpointSeconds}
            min={R.checkpointSeconds.min}
            max={R.checkpointSeconds.max}
            step={0.005}
            unit="秒"
            error={errorOf('checkpointSeconds')}
            onValue={(value) => set('checkpointSeconds', value)}
          />
          <NumberField
            label="敲擊接觸時間"
            hint="Tap 觸碰後停留多久才放開（0.001–1 秒）。要滑向下一個相鄰目標時，最短只需停留一個判定幀（1/60 秒）。"
            value={config.contactSeconds}
            min={R.contactSeconds.min}
            max={R.contactSeconds.max}
            sliderMin={0.005}
            sliderMax={0.2}
            step={0.005}
            unit="秒"
            error={errorOf('contactSeconds')}
            onValue={(value) => set('contactSeconds', value)}
          />
          <NumberField
            label="換手重疊時間"
            hint="交接時兩手同時在軌道上的長度，不可超過搜尋取樣間隔。"
            value={config.handoverSeconds}
            min={R.handoverSeconds.min}
            max={R.handoverSeconds.max}
            sliderMin={0.005}
            step={0.005}
            unit="秒"
            error={errorOf('handoverSeconds')}
            onValue={(value) => set('handoverSeconds', value)}
          />
          <NumberField
            label="相鄰連擊滑移距離"
            hint="連續兩次接觸近到這個距離內、且間隔短於下方的連打判定間隔時，當成手不抬起、貼著面板滑過去（相鄰鍵約 0.77，隔兩鍵 1.41）；間隔比敲擊接觸時間還短也可以，只要至少一幀。0 表示關閉。"
            value={config.glideDistance}
            min={R.glideDistance.min}
            max={R.glideDistance.max}
            step={0.05}
            error={errorOf('glideDistance')}
            onValue={(value) => set('glideDistance', value)}
          />
          <NumberField
            label="Slide 最晚接上時間"
            hint="手最晚可以比星星晚多久才接上軌道；接上後仍要在原定終點前走完（0–2 秒）。"
            value={config.slidePickupSeconds}
            min={R.slidePickupSeconds.min}
            max={R.slidePickupSeconds.max}
            sliderMax={1}
            step={0.01}
            unit="秒"
            error={errorOf('slidePickupSeconds')}
            onValue={(value) => set('slidePickupSeconds', value)}
          />
          <NumberField
            label="兩次換手最短間隔"
            hint="避免左右手來回抖動；不可短於換手重疊時間（最多 10 秒）。"
            value={config.handoverCooldown}
            min={R.handoverCooldown.min}
            max={R.handoverCooldown.max}
            sliderMin={0.02}
            sliderMax={2}
            step={0.02}
            unit="秒"
            error={errorOf('handoverCooldown')}
            onValue={(value) => set('handoverCooldown', value)}
          />
          <NumberField
            label="開始前預備時間"
            hint="雙手在第一顆音符前多久就定位（0.01–10 秒）。"
            value={config.preparationSeconds}
            min={R.preparationSeconds.min}
            max={R.preparationSeconds.max}
            sliderMin={0.1}
            step={0.1}
            unit="秒"
            error={errorOf('preparationSeconds')}
            onValue={(value) => set('preparationSeconds', value)}
          />
          <NumberField
            label="同手連打判定間隔"
            hint="同一隻手相鄰敲擊短於這個時間就算連打負擔；也是滑移的門檻，長於這個時間就有餘裕抬手（0.001–10 秒）。"
            value={config.repetitionSeconds}
            min={R.repetitionSeconds.min}
            max={R.repetitionSeconds.max}
            sliderMin={0.02}
            sliderMax={1}
            step={0.01}
            unit="秒"
            error={errorOf('repetitionSeconds')}
            onValue={(value) => set('repetitionSeconds', value)}
          />
        </div>

        <div class="stack">
          <div class="group-title">
            手掌覆蓋 Touch
            <span class="muted xsmall">{palmEnabled ? '啟用中' : '已關閉'}</span>
          </div>
          <label class="check">
            <input
              type="checkbox"
              checked={palmEnabled}
              onchange={(event) => setPalmEnabled(event.currentTarget.checked)}
            />
            <span>
              允許一隻手掌同時覆蓋多個 Touch
              <span class="field-hint">
                關閉等同把半徑設為 0：每個 Touch 都要各自接觸，同時多顆可能因此變成無方案。
              </span>
            </span>
          </label>
          <NumberField
            label="手掌半徑"
            hint="盤面半徑為 1 的圓形近似範圍（0–1）；直接填 0 也等於關閉。"
            value={config.palmRadius}
            min={R.palmRadius.min}
            max={R.palmRadius.max}
            step={0.05}
            error={errorOf('palmRadius')}
            onValue={(value) => set('palmRadius', value)}
          />
          <NumberField
            label="備用手掌半徑"
            hint="平常以上面的手掌半徑為主；整首都找不到方案時，才改用這個較大的半徑重試，結果會註明。不大於手掌半徑或關閉手掌覆蓋時不使用。"
            value={config.palmFallbackRadius}
            min={R.palmFallbackRadius.min}
            max={R.palmFallbackRadius.max}
            step={0.05}
            error={errorOf('palmFallbackRadius')}
            onValue={(value) => set('palmFallbackRadius', value)}
          />
          <p class="field-hint">{PALM_APPROX_HINT}</p>
          <p class="field-hint">
            只有同一判定時間的 Touch／Touch Hold 會被併成一掌，Tap 與 Slide 不會；
            覆蓋期間該手不能接別處。實際是否成立由核心判定，改動後要重新生成。
          </p>
        </div>
      </div>
    </details>
  </section>

  <section class="section">
    {#if issues.length > 0}
      <div class="alert alert--error" style="margin-bottom: var(--space-3)">
        <div class="alert-title">參數超出核心允許範圍</div>
        <ul class="small">
          {#each issues as issue, index (index)}
            <li>{FIELD_LABEL[issue.field] ?? issue.field}：{issue.message}</li>
          {/each}
        </ul>
      </div>
    {/if}
    <div class="row row-wrap">
      <button
        class="btn btn--primary"
        onclick={() => session.analyze()}
        disabled={!session.desktop || issues.length > 0}
      >
        <Icon name="refresh-cw" />套用並重新生成
      </button>
      <button class="btn" onclick={() => session.resetConfig()}><Icon name="rotate-ccw" />還原預設</button>
    </div>
    {#if !session.desktop}
      <p class="field-hint" style="margin-top: var(--space-2)">瀏覽器預覽不能重跑搜尋。</p>
    {/if}
  </section>
</div>

<style>
  /* 評分方式：V4 卡佔一整列；TemotoAI-1 主卡（瑠璃紺、菖蒲）兩張並排。
     分隔列「顯示更多模型」下面同一套規則：V3、V3.1、V3.2 各佔一整列，
     舊的三張 TemotoAI-1 兩兩並排（瑠璃、茜一排，常磐第二排同尺寸）。 */
  .models {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }

  .models .model-card--v3 {
    grid-column: 1 / -1;
  }

  /* 分隔列：兩側細線，中間是無填滿的膠囊按鈕。 */
  .more-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .more-row::before,
  .more-row::after {
    content: '';
    flex: 1;
    min-width: var(--space-3);
    height: 1px;
    background: var(--c-border);
  }

  .more-toggle {
    flex: none;
    gap: var(--space-1);
    min-height: 28px;
    padding: 0 12px;
    font-size: var(--fs-xs);
  }

  .more-current {
    color: var(--c-text);
  }

  .more-toggle .chevron {
    transition: transform 160ms var(--ease-out);
  }

  .more-toggle.is-open .chevron {
    transform: rotate(180deg);
  }

  .more-models {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-2);
    animation: more-in 180ms var(--ease-out);
  }

  @keyframes more-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .more-models {
      animation: none;
    }

    .more-toggle .chevron {
      transition: none;
    }
  }

  /* V3 卡：中性色的扇形，選中時填滿反白色。 */
  .model-card--v3 {
    --model: var(--c-border-strong);
    --model-fan: var(--c-border-strong);
    --model-fill: var(--c-invert-bg);
    --on-model: var(--c-on-invert);
    min-height: 56px;
  }

  .advanced > summary {
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    font-weight: 600;
    color: var(--c-text);
  }

  .advanced > summary::-webkit-details-marker {
    display: none;
  }

  .chevron {
    display: inline-flex;
    transition: transform 120ms ease;
  }

  .advanced[open] .chevron {
    transform: rotate(90deg);
  }

  .advanced[open] > summary {
    margin-bottom: var(--space-4);
  }

  .advanced-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .group-title {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--c-text-dim);
  }
</style>
