<script lang="ts">
  import Icon from './Icon.svelte';
  import { formatBytes } from '../lib/video';
  import type { PluginAction, PluginInfo } from '../lib/types';
  import { plugins, type PluginRun } from '../state/plugins.svelte';

  /**
   * 設定頁「外掛」：安裝、移除，顯示外掛回報的摘要、數字與動作，執行中顯示進度與取消。
   * 外掛做什麼由外掛自己說，這裡不寫死任何特定外掛。只在桌面版顯示（瀏覽器預覽沒有外掛）。
   */

  type Confirming =
    | { pluginId: string; kind: 'remove' }
    | { pluginId: string; kind: 'action'; action: PluginAction };

  /** 要按第二次才執行的動作與移除；同一時間只有一個。 */
  let confirming = $state<Confirming | null>(null);
  /** 移除時連同外掛的資料一起刪除；預設不刪。 */
  let deleteData = $state(false);

  /** 取消還沒按第二次的確認；有取消時回傳 true（設定頁的 Esc 先收這個）。 */
  export function cancelConfirm(): boolean {
    if (!confirming) return false;
    confirming = null;
    return true;
  }

  function press(plugin: PluginInfo, action: PluginAction) {
    if (action.confirm) {
      confirming = { pluginId: plugin.id, kind: 'action', action };
      return;
    }
    confirming = null;
    void plugins.runAction(plugin, action);
  }

  function confirmAction(plugin: PluginInfo, action: PluginAction) {
    confirming = null;
    void plugins.runAction(plugin, action);
  }

  function askRemove(plugin: PluginInfo) {
    deleteData = false;
    confirming = { pluginId: plugin.id, kind: 'remove' };
  }

  async function remove(plugin: PluginInfo) {
    if (await plugins.remove(plugin, deleteData)) confirming = null;
  }

  function percent(run: PluginRun): number {
    if (run.total === null || run.total <= 0) return 0;
    return Math.min(100, Math.max(0, Math.round((100 * run.done) / run.total)));
  }

  function countText(run: PluginRun): string {
    if (run.total !== null) return `${run.done}／${run.total}`;
    if (run.done > 0) return `${run.done}`;
    return '執行中';
  }

  const empty = $derived(plugins.loaded && !plugins.listError && plugins.list.length === 0);
</script>

<section class="block" aria-labelledby="settings-plugins">
  <div class="block-head">
    <h3 id="settings-plugins">外掛</h3>
    <button
      class="btn"
      class:btn--primary={empty}
      onclick={() => void plugins.install()}
      disabled={plugins.installing || plugins.anyRunning}
      title={plugins.anyRunning ? '有外掛正在執行，完成後才能安裝' : undefined}
    >
      <Icon name={plugins.installing ? 'loader' : 'plus'} spin={plugins.installing} />
      {plugins.installing ? '安裝中…' : '安裝外掛…'}
    </button>
  </div>

  {#if !plugins.loaded}
    <div class="loading" role="status" aria-label="讀取外掛">
      <span class="skeleton skeleton--line" style="width:40%"></span>
      <span class="skeleton skeleton--line" style="width:70%"></span>
    </div>
  {:else if plugins.listError}
    <p class="field-error">讀不到已安裝的外掛：{plugins.listError}</p>
  {:else if plugins.list.length === 0}
    <p class="field-hint">
      還沒有安裝外掛。<span class="help">外掛是另外安裝的程式（.mmplugin 檔），可以替這個 App 加上額外的功能；裝好後會列在這裡。</span>
    </p>
  {:else}
    <p class="field-hint help">外掛是另外安裝的程式。再安裝一次同一個外掛的新檔會換成新版。</p>
    {#each plugins.list as plugin (plugin.id)}
      {@const panel = plugins.panels[plugin.id]}
      {@const panelError = plugins.panelErrors[plugin.id]}
      {@const run = plugins.runs[plugin.id]}
      {@const removing = plugins.removing === plugin.id}
      {@const pending = confirming?.pluginId === plugin.id ? confirming : null}
      <div class="plugin">
        <div class="plugin-head">
          <div class="data-title plugin-title">
            <span class="plugin-name">{plugin.name}</span>
            <span class="xsmall muted mono">{plugin.version}</span>
          </div>
          <button
            class="btn btn--icon"
            onclick={() => askRemove(plugin)}
            disabled={!!run || removing || pending?.kind === 'remove'}
            aria-label={`移除 ${plugin.name}`}
            title={run ? '執行中不能移除' : '移除外掛'}
          >
            <Icon name="trash" />
          </button>
        </div>

        {#if panel?.summary}
          <p class="small summary">{panel.summary}</p>
        {:else if !panel && !panelError}
          <span class="skeleton skeleton--line summary-skeleton"></span>
        {/if}
        {#if panelError}
          <p class="field-error">{panelError}</p>
        {/if}
        {#if plugin.description}
          <p class="field-hint help">{plugin.description}</p>
        {/if}
        {#if panel && panel.facts.length > 0}
          <dl class="kv facts help">
            {#each panel.facts as fact, index (index)}
              <dt>{fact.label}</dt>
              <dd>{fact.value}</dd>
            {/each}
          </dl>
        {/if}
        <p class="xsmall muted mono">資料 {formatBytes(plugin.dataBytes)}</p>

        {#if panel && panel.actions.length > 0}
          <div class="plugin-actions">
            {#each panel.actions as action (action.id)}
              <button
                class="btn"
                class:btn--primary={action.primary}
                onclick={() => press(plugin, action)}
                disabled={!!run || removing}
              >
                {action.label}{action.save ? '…' : ''}
              </button>
            {/each}
          </div>
        {/if}

        {#if run}
          <div class="run">
            <div
              class="bar-track"
              class:is-indeterminate={run.total === null}
              role="progressbar"
              aria-label={`${plugin.name} 的進度`}
              aria-valuemin={0}
              aria-valuemax={run.total ?? undefined}
              aria-valuenow={run.total !== null ? run.done : undefined}
              aria-valuetext={run.total !== null ? `${run.done}／${run.total}` : '執行中'}
            >
              {#if run.total !== null}
                <div class="bar-fill" style={`width:${percent(run)}%`}></div>
              {/if}
            </div>
            <div class="run-line">
              <span class="run-count xsmall mono">{countText(run)}</span>
              <span class="run-message xsmall muted" title={run.message ?? undefined}>{run.message ?? ''}</span>
              <button class="btn" onclick={() => void plugins.cancel(plugin.id)} disabled={run.cancelling}>
                <!-- 取消中只換圖示、字不變，按鈕寬度不跳。 -->
                <Icon name={run.cancelling ? 'loader' : 'x'} spin={run.cancelling} />取消
              </button>
            </div>
          </div>
        {/if}

        {#if pending?.kind === 'action'}
          <div class="confirm">
            <p class="small confirm-text">{pending.action.confirm}</p>
            <div class="data-actions">
              <button class="btn" onclick={() => (confirming = null)}>取消</button>
              <button class="btn" onclick={() => confirmAction(plugin, pending.action)} disabled={!!run}>
                <Icon name="check" />確定{pending.action.label}
              </button>
            </div>
          </div>
        {:else if pending?.kind === 'remove'}
          <div class="confirm">
            <div class="confirm-body">
              <p class="field-error">再按一次「確定移除」才會移除「{plugin.name}」。</p>
              <label class="check">
                <input type="checkbox" bind:checked={deleteData} disabled={removing} />
                <span>
                  連同資料一起刪除<span class="muted mono">（{formatBytes(plugin.dataBytes)}）</span>
                  <span class="field-hint help">不刪的話，之後再安裝同一個外掛可以接著用原本的資料。</span>
                </span>
              </label>
            </div>
            <div class="data-actions">
              <button class="btn" onclick={() => (confirming = null)} disabled={removing}>取消</button>
              <button class="btn btn--danger" onclick={() => void remove(plugin)} disabled={removing || !!run}>
                <Icon name={removing ? 'loader' : 'trash'} spin={removing} />確定移除
              </button>
            </div>
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</section>

<style>
  /* 和設定頁其他區塊一樣：區塊之間只用分隔線，不再加一層外框。 */
  .block {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }

  .block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 30px;
  }

  .loading {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* 一個外掛一段，段與段之間一條分隔線（同設定頁的 .data-row）。 */
  .plugin {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .plugin + .plugin {
    padding-top: var(--space-3);
    border-top: 1px solid var(--c-border);
  }

  .plugin-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .data-title {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-weight: 600;
  }

  .plugin-title {
    flex: 1 1 auto;
    min-width: 0;
  }

  .plugin-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .summary {
    overflow-wrap: anywhere;
  }

  .summary-skeleton {
    width: 55%;
  }

  .facts dd {
    overflow-wrap: anywhere;
  }

  .plugin-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .run {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* 還不知道總數：整條軌道用骨架的脈動表示仍在進行（不另做循環動畫）。 */
  .bar-track.is-indeterminate {
    animation: skeleton-pulse 1.3s ease-in-out infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .bar-track.is-indeterminate {
      animation: none;
    }
  }

  /* 進度訊息可能是很長的名稱：單行截斷，數字與取消鈕位置固定，版面不跳。 */
  .run-line {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .run-count {
    flex: none;
    min-width: 72px;
  }

  .run-message {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .run-line .btn {
    flex: none;
  }

  .confirm {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
    padding-top: var(--space-2);
  }

  .confirm-text,
  .confirm-body {
    flex: 1 1 auto;
    min-width: 0;
  }

  .confirm-text {
    overflow-wrap: anywhere;
  }

  .confirm-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .data-actions {
    display: flex;
    gap: var(--space-2);
    flex: none;
  }

  @media (max-width: 559px) {
    .confirm {
      flex-direction: column;
      gap: var(--space-2);
    }
  }
</style>
