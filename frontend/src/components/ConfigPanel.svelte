<script lang="ts">
  import Icon from './Icon.svelte';
  import NumberField from './NumberField.svelte';
  import { DEFAULT_CONFIG } from '../lib/contract';
  import { PALM_APPROX_HINT } from '../lib/palm';
  import { session } from '../state/session.svelte';
  import type { SolverConfig } from '../lib/types';

  const issues = $derived(session.configIssues);

  function errorOf(field: string): string | null {
    return issues.find((issue) => issue.field === field)?.message ?? null;
  }

  function set<K extends keyof SolverConfig>(key: K, value: SolverConfig[K]) {
    session.config = { ...session.config, [key]: value };
  }

  const config = $derived(session.config);

  // 關閉手掌覆蓋就是半徑 0。記住上一次的半徑，重新打開時不必再輸入一次。
  let lastPalmRadius = $state(DEFAULT_CONFIG.palmRadius);
  const palmEnabled = $derived(config.palmRadius > 0);

  function setPalmEnabled(enabled: boolean) {
    if (!enabled) {
      if (config.palmRadius > 0) lastPalmRadius = config.palmRadius;
      set('palmRadius', 0);
      return;
    }
    set('palmRadius', lastPalmRadius > 0 ? lastPalmRadius : DEFAULT_CONFIG.palmRadius);
  }
</script>

<div class="stack">
  <section class="section">
    <div class="section-title">
      <span>搜尋設定</span>
      {#if session.stale}
        <span class="badge badge--quiet">需重新生成</span>
      {/if}
    </div>

    <div class="stack">
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

      <NumberField
        label="搜尋寬度"
        hint="每個時間點保留多少種可能；越大越仔細也越慢（1–256）。"
        value={config.beamWidth}
        min={1}
        max={256}
        step={1}
        error={errorOf('beamWidth')}
        onValue={(value) => set('beamWidth', Math.round(value))}
      />

      <NumberField
        label="保留候選方案數"
        hint="最後要比較幾種打法（1–5，且不大於搜尋寬度）。"
        value={config.topK}
        min={1}
        max={5}
        step={1}
        error={errorOf('topK')}
        onValue={(value) => set('topK', Math.round(value))}
      />

      <NumberField
        label="起始秒數"
        hint="譜面開始前的空白秒數（0–120）。"
        value={session.firstSeconds}
        min={0}
        max={120}
        step={0.1}
        unit="秒"
        error={errorOf('firstSeconds')}
        onValue={(value) => (session.firstSeconds = value)}
      />
    </div>
  </section>

  <section class="section">
    <div class="section-title"><span>時間與動作模型</span></div>
    <div class="stack">
      <NumberField
        label="搜尋取樣間隔"
        hint="Slide 上多久考慮一次換手（0.02–0.2 秒）。"
        value={config.checkpointSeconds}
        min={0.02}
        max={0.2}
        step={0.005}
        unit="秒"
        error={errorOf('checkpointSeconds')}
        onValue={(value) => set('checkpointSeconds', value)}
      />
      <NumberField
        label="敲擊接觸時間"
        hint="Tap 觸碰後停留多久才放開。"
        value={config.contactSeconds}
        min={0.005}
        max={0.2}
        step={0.005}
        unit="秒"
        error={errorOf('contactSeconds')}
        onValue={(value) => set('contactSeconds', value)}
      />
      <NumberField
        label="換手重疊時間"
        hint="交接時兩手同時在軌道上的長度，不可超過搜尋取樣間隔。"
        value={config.handoverSeconds}
        min={0.005}
        max={0.2}
        step={0.005}
        unit="秒"
        error={errorOf('handoverSeconds')}
        onValue={(value) => set('handoverSeconds', value)}
      />
      <NumberField
        label="相鄰連擊滑移距離"
        hint="連續兩次接觸近到這個距離內、且間隔短於下方的連打判定間隔時，當成手不抬起、貼著面板滑過去（相鄰鍵約 0.77，隔兩鍵 1.41）。0 表示關閉。"
        value={config.glideDistance}
        min={0}
        max={2}
        step={0.05}
        error={errorOf('glideDistance')}
        onValue={(value) => set('glideDistance', value)}
      />
      <NumberField
        label="Slide 最晚接上時間"
        hint="手最晚可以比星星晚多久才接上軌道；接上後仍要在原定終點前走完。"
        value={config.slidePickupSeconds}
        min={0}
        max={1}
        step={0.01}
        unit="秒"
        error={errorOf('slidePickupSeconds')}
        onValue={(value) => set('slidePickupSeconds', value)}
      />
      <NumberField
        label="兩次換手最短間隔"
        hint="避免左右手來回抖動。"
        value={config.handoverCooldown}
        min={0.02}
        max={2}
        step={0.02}
        unit="秒"
        error={errorOf('handoverCooldown')}
        onValue={(value) => set('handoverCooldown', value)}
      />
      <NumberField
        label="開始前預備時間"
        hint="雙手在第一顆音符前多久就定位（最多 10 秒）。"
        value={config.preparationSeconds}
        min={0.1}
        max={10}
        step={0.1}
        unit="秒"
        error={errorOf('preparationSeconds')}
        onValue={(value) => set('preparationSeconds', value)}
      />
      <NumberField
        label="速度基準"
        hint="以每秒幾個盤面半徑為基準計算速度負擔。"
        value={config.speedReference}
        min={0.5}
        max={20}
        step={0.5}
        unit="半徑/秒"
        error={errorOf('speedReference')}
        onValue={(value) => set('speedReference', value)}
      />
      <NumberField
        label="同手連打判定間隔"
        hint="同一隻手相鄰敲擊短於這個時間就開始加成本；也是滑移的門檻，長於這個時間就有餘裕抬手。"
        value={config.repetitionSeconds}
        min={0.02}
        max={1}
        step={0.01}
        unit="秒"
        error={errorOf('repetitionSeconds')}
        onValue={(value) => set('repetitionSeconds', value)}
      />
    </div>
  </section>

  <section class="section">
    <div class="section-title">
      <span>手掌覆蓋 Touch</span>
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

    <div style="margin-top: var(--space-3)">
      <NumberField
        label="手掌半徑"
        hint="盤面半徑為 1 的圓形近似範圍（0–1）；直接填 0 也等於關閉。"
        value={config.palmRadius}
        min={0}
        max={1}
        step={0.05}
        error={errorOf('palmRadius')}
        onValue={(value) => set('palmRadius', value)}
      />
    </div>

    <p class="field-hint" style="margin-top: var(--space-3)">{PALM_APPROX_HINT}</p>
    <p class="field-hint" style="margin-top: var(--space-2)">
      只有同一判定時間的 Touch／Touch Hold 會被併成一掌，Tap 與 Slide 不會；
      覆蓋期間該手不能接別處。實際是否成立由核心判定，改動後要重新生成。
    </p>
  </section>

  <section class="section">
    <div class="section-title">
      <span>成本權重</span>
      <span class="muted xsmall">0–100，越高越在意</span>
    </div>
    <div class="stack">
      <NumberField
        label="移動距離"
        hint="越高越偏好省力、少跑動的打法。"
        value={config.distanceWeight}
        min={0}
        max={20}
        step={0.1}
        error={errorOf('distanceWeight')}
        onValue={(value) => set('distanceWeight', value)}
      />
      <NumberField
        label="移動速度負擔"
        hint="越高越避免短時間內的大跨度移動。"
        value={config.speedWeight}
        min={0}
        max={20}
        step={0.1}
        error={errorOf('speedWeight')}
        onValue={(value) => set('speedWeight', value)}
      />
      <NumberField
        label="手伸到對側"
        hint="越高越不希望左手跑到右半邊（右手同理）。"
        value={config.sideWeight}
        min={0}
        max={20}
        step={0.1}
        error={errorOf('sideWeight')}
        onValue={(value) => set('sideWeight', value)}
      />
      <NumberField
        label="雙手交叉"
        hint="越高越避免左手越過右手的姿態。"
        value={config.crossWeight}
        min={0}
        max={20}
        step={0.1}
        error={errorOf('crossWeight')}
        onValue={(value) => set('crossWeight', value)}
      />
      <NumberField
        label="同手快速連打"
        hint="越高越偏好把連續音符分給兩手。"
        value={config.repetitionWeight}
        min={0}
        max={20}
        step={0.1}
        error={errorOf('repetitionWeight')}
        onValue={(value) => set('repetitionWeight', value)}
      />
      <NumberField
        label="換手次數"
        hint="越高越不願意在 Slide 中途換手。"
        value={config.handoverWeight}
        min={0}
        max={20}
        step={0.1}
        error={errorOf('handoverWeight')}
        onValue={(value) => set('handoverWeight', value)}
      />
    </div>
  </section>

  <section class="section">
    {#if issues.length > 0}
      <div class="alert alert--error" style="margin-bottom: var(--space-3)">
        <div class="alert-title">參數超出核心允許範圍</div>
        <ul class="small">
          {#each issues as issue, index (index)}
            <li>{issue.field}：{issue.message}</li>
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
    {#if !session.desktop}
      <p class="field-hint" style="margin-top: var(--space-2)">瀏覽器預覽不能重跑搜尋。</p>
    {/if}
  </section>
</div>
