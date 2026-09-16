<script lang="ts">
  import Icon from './Icon.svelte';
  import NumberField from './NumberField.svelte';
  import {
    ADVANCED_FIELDS,
    CONFIG_RANGE,
    DEFAULT_BASE,
    FIELD_LABEL,
    SCORING_LABEL,
    SCORING_V1,
    SCORING_V2,
  } from '../lib/contract';
  import { PALM_APPROX_HINT } from '../lib/palm';
  import { session } from '../state/session.svelte';
  import type { ConfigDraft, ScoringModel } from '../lib/types';

  const issues = $derived(session.configIssues);

  function errorOf(field: string): string | null {
    return issues.find((issue) => issue.field === field)?.message ?? null;
  }

  function set<K extends keyof ConfigDraft>(key: K, value: ConfigDraft[K]) {
    session.config = { ...session.config, [key]: value };
  }

  const config = $derived(session.config);
  const isV2 = $derived(config.scoringModel === SCORING_V2);
  const R = CONFIG_RANGE;

  const MODELS: { id: ScoringModel; hint: string }[] = [
    { id: SCORING_V2, hint: '先維持左右分工，再看動作負擔' },
    { id: SCORING_V1, hint: '六項成本加總，供對照' },
  ];

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

<div class="stack">
  <section class="section">
    <div class="section-title">
      <span>評分方式</span>
      {#if session.stale}
        <span class="badge badge--quiet">需重新生成</span>
      {/if}
    </div>
    <div class="segmented" role="radiogroup" aria-label="評分方式">
      {#each MODELS as model (model.id)}
        <button
          class="btn segment"
          class:is-active={config.scoringModel === model.id}
          role="radio"
          aria-checked={config.scoringModel === model.id}
          onclick={() => session.setScoringModel(model.id)}
        >
          <span class="segment-label">{SCORING_LABEL[model.id]}</span>
          <span class="segment-hint">{model.hint}</span>
        </button>
      {/each}
    </div>
    <p class="field-hint" style="margin-top: var(--space-2)">
      兩種方式的參數各自保存，切換時不互相換算；舊版權重沒有對應到下方四個偏好的精確公式。
    </p>
  </section>

  {#if isV2}
    <section class="section">
      <div class="section-title"><span>打法偏好</span></div>
      <div class="stack">
        <NumberField
          label="左右分工傾向"
          hint="越高越偏好左手打左半邊、右手打右半邊，願意多移動來維持分工。"
          value={config.homePreference}
          min={R.homePreference.min}
          max={R.homePreference.max}
          step={1}
          error={errorOf('homePreference')}
          onValue={(value) => set('homePreference', value)}
        />
        <NumberField
          label="快速移動容忍"
          hint="越高越能接受快速大跨度移動；超過這個速度才開始算負擔，不是速度上限。"
          value={config.travelComfort}
          min={R.travelComfort.min}
          max={R.travelComfort.max}
          step={1}
          unit="半徑/秒"
          error={errorOf('travelComfort')}
          onValue={(value) => set('travelComfort', value)}
        />
        <NumberField
          label="同手連打容忍"
          hint="越高越接受同一隻手連續重新擊打。"
          value={config.repeatTolerance}
          min={R.repeatTolerance.min}
          max={R.repeatTolerance.max}
          step={1}
          error={errorOf('repeatTolerance')}
          onValue={(value) => set('repeatTolerance', value)}
        />
        <NumberField
          label="Slide 換手意願"
          hint="越高越願意在 Slide 中途交給另一隻手；最高也不會鼓勵反覆換手。"
          value={config.handoverWillingness}
          min={R.handoverWillingness.min}
          max={R.handoverWillingness.max}
          step={1}
          error={errorOf('handoverWillingness')}
          onValue={(value) => set('handoverWillingness', value)}
        />
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
      </div>
      <p class="field-hint" style="margin-top: var(--space-3)">
        數值可輸入小數。這些偏好與預設值是本 Demo 的起點，尚未經玩家校準，不代表官方判定或人體能力。
      </p>
    </section>
  {:else}
    <section class="section">
      <div class="section-title">
        <span>舊版成本權重</span>
        <span class="muted xsmall">0–100，越高越在意</span>
      </div>
      <div class="stack">
        <NumberField
          label="移動距離"
          hint="越高越偏好省力、少跑動的打法。"
          value={config.distanceWeight}
          min={R.weight.min}
          max={R.weight.max}
          sliderMax={20}
          step={0.1}
          error={errorOf('distanceWeight')}
          onValue={(value) => set('distanceWeight', value)}
        />
        <NumberField
          label="移動速度負擔"
          hint="越高越避免短時間內的大跨度移動。"
          value={config.speedWeight}
          min={R.weight.min}
          max={R.weight.max}
          sliderMax={20}
          step={0.1}
          error={errorOf('speedWeight')}
          onValue={(value) => set('speedWeight', value)}
        />
        <NumberField
          label="手伸到對側"
          hint="越高越不希望左手跑到右半邊（右手同理）。"
          value={config.sideWeight}
          min={R.weight.min}
          max={R.weight.max}
          sliderMax={20}
          step={0.1}
          error={errorOf('sideWeight')}
          onValue={(value) => set('sideWeight', value)}
        />
        <NumberField
          label="雙手交叉"
          hint="越高越避免左手越過右手的姿態。"
          value={config.crossWeight}
          min={R.weight.min}
          max={R.weight.max}
          sliderMax={20}
          step={0.1}
          error={errorOf('crossWeight')}
          onValue={(value) => set('crossWeight', value)}
        />
        <NumberField
          label="同手快速連打"
          hint="越高越偏好把連續音符分給兩手。"
          value={config.repetitionWeight}
          min={R.weight.min}
          max={R.weight.max}
          sliderMax={20}
          step={0.1}
          error={errorOf('repetitionWeight')}
          onValue={(value) => set('repetitionWeight', value)}
        />
        <NumberField
          label="換手次數"
          hint="越高越不願意在 Slide 中途換手。"
          value={config.handoverWeight}
          min={R.weight.min}
          max={R.weight.max}
          sliderMax={20}
          step={0.1}
          error={errorOf('handoverWeight')}
          onValue={(value) => set('handoverWeight', value)}
        />
        <NumberField
          label="速度基準"
          hint="以每秒幾個盤面半徑為基準計算速度負擔（0.1–100）。"
          value={config.speedReference}
          min={R.speedReference.min}
          max={R.speedReference.max}
          sliderMin={0.5}
          sliderMax={20}
          step={0.5}
          unit="半徑/秒"
          error={errorOf('speedReference')}
          onValue={(value) => set('speedReference', value)}
        />
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
      </div>
      <p class="field-hint" style="margin-top: var(--space-3)">
        舊版評分只供對照。成本是本 Demo 的啟發式規則，不是官方判定或人體模型。
      </p>
    </section>
  {/if}

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
        disabled={!session.desktop || session.phase === 'analyzing' || issues.length > 0}
      >
        <Icon name="refresh-cw" />套用並重新生成
      </button>
      <button class="btn" onclick={() => session.resetConfig()}><Icon name="rotate-ccw" />還原預設</button>
    </div>
    <p class="field-hint" style="margin-top: var(--space-2)">
      還原預設會保留目前的評分方式「{SCORING_LABEL[config.scoringModel]}」。
    </p>
    {#if !session.desktop}
      <p class="field-hint" style="margin-top: var(--space-2)">瀏覽器預覽不能重跑搜尋。</p>
    {/if}
  </section>
</div>

<style>
  .segmented {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

  .segment {
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 2px;
    height: auto;
    padding: var(--space-2) var(--space-3);
    text-align: left;
    white-space: normal;
  }

  .segment-label {
    font-weight: 600;
  }

  .segment-hint {
    font-size: var(--fs-xs);
    font-weight: 400;
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
