<script lang="ts">
  import { untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import Icon from './Icon.svelte';
  import VirtualList from './VirtualList.svelte';
  import { reducedMotion } from '../lib/press';
  import type { PluginSelectItem, PluginSelectRequest } from '../lib/types';
  import { plugins } from '../state/plugins.svelte';

  /**
   * 外掛動作結束後要使用者勾選的項目（plugins.select）。項目依 group 分組、保留原順序，預設全選；
   * 確認後以外掛指定的動作再執行一次。項目可能上千筆，清單只畫看得到的列。
   */

  type Row =
    | { kind: 'group'; key: string; label: string; ids: string[] }
    | { kind: 'item'; key: string; item: PluginSelectItem };

  /** 列高必須和下面的樣式一致。 */
  const GROUP_H = 44;
  const ITEM_H = 36;
  const ITEM_DETAIL_H = 48;

  let dialog: HTMLDialogElement | null = $state(null);
  let confirmButton: HTMLButtonElement | null = $state(null);
  /** 關閉動畫期間仍顯示原本的內容。 */
  let request = $state<PluginSelectRequest | null>(null);
  let pluginName = $state('');
  const selected = new SvelteSet<string>();

  let closing = $state(false);
  let pressedOutside = false;
  const CLOSE_MS = 140;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!dialog) return;
    const pending = plugins.select;
    if (pending) {
      clearTimeout(closeTimer);
      closing = false;
      // 換成新的請求才重設勾選（預設全選）；勾選本身不會讓這裡重跑。
      untrack(() => {
        if (!dialog?.open || request !== pending.request) {
          request = pending.request;
          pluginName = pending.pluginName;
          setAll(true);
        }
      });
      if (!dialog.open) {
        dialog.showModal();
        confirmButton?.focus();
      }
    } else if (dialog.open && !closing) {
      if (reducedMotion()) {
        dialog.close();
        return;
      }
      closing = true;
      closeTimer = setTimeout(() => {
        closing = false;
        dialog?.close();
      }, CLOSE_MS);
    }
  });

  /** 依 group 分組：第一次出現的順序排組，組內保留原順序；全部沒有分組時不畫組標題。 */
  const rows = $derived.by((): Row[] => {
    const items = request?.items ?? [];
    const groups = new Map<string | null, PluginSelectItem[]>();
    for (const item of items) {
      const key = item.group ?? null;
      const list = groups.get(key);
      if (list) list.push(item);
      else groups.set(key, [item]);
    }
    const headed = groups.size > 1 || !groups.has(null);
    const out: Row[] = [];
    // 列的 key 用位置，外掛給了重複的 id 也不會讓清單出錯。
    let index = 0;
    for (const [group, list] of groups) {
      if (headed) {
        out.push({ kind: 'group', key: `g:${index}`, label: group ?? '其他', ids: list.map((item) => item.id) });
      }
      for (const item of list) out.push({ kind: 'item', key: `i:${index++}`, item });
    }
    return out;
  });

  const total = $derived(new Set((request?.items ?? []).map((item) => item.id)).size);
  const count = $derived(selected.size);

  function heightOf(row: Row): number {
    if (row.kind === 'group') return GROUP_H;
    return row.item.detail ? ITEM_DETAIL_H : ITEM_H;
  }

  function setAll(on: boolean) {
    selected.clear();
    if (on) for (const item of request?.items ?? []) selected.add(item.id);
  }

  function setGroup(ids: string[], on: boolean) {
    for (const id of ids) {
      if (on) selected.add(id);
      else selected.delete(id);
    }
  }

  function groupCount(ids: string[]): number {
    let n = 0;
    for (const id of ids) if (selected.has(id)) n += 1;
    return n;
  }

  function toggle(id: string, on: boolean) {
    if (on) selected.add(id);
    else selected.delete(id);
  }

  function close() {
    plugins.select = null;
  }

  function confirm() {
    if (!request || count === 0) return;
    const ids = [...new Set(request.items.filter((item) => selected.has(item.id)).map((item) => item.id))];
    void plugins.confirmSelect(ids);
  }

  function isOutside(event: MouseEvent): boolean {
    if (!dialog || event.target !== dialog) return false;
    const rect = dialog.getBoundingClientRect();
    return (
      event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom
    );
  }
</script>

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="plugin-select-title"
  onclose={() => {
    if (plugins.select) plugins.select = null;
  }}
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  onpointerdown={(event) => (pressedOutside = isOutside(event))}
  onclick={(event) => {
    if (pressedOutside && isOutside(event)) close();
    pressedOutside = false;
  }}
>
  <header class="dialog-head">
    <h2 id="plugin-select-title" class="title">{request?.title ?? ''}</h2>
    <span class="muted small plugin-name">{pluginName}</span>
    <span class="spacer"></span>
    <button class="btn btn--round" onclick={close} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div class="dialog-body">
    {#if request?.message}
      <p class="small message">{request.message}</p>
    {/if}
    <div class="row bar">
      <span class="small mono" role="status">已選 {count}／{total} 項</span>
      <span class="spacer"></span>
      <div class="btn-group" role="group" aria-label="全部項目">
        <button class="btn-group-item" onclick={() => setAll(true)} disabled={count === total}>全選</button>
        <button class="btn-group-item" onclick={() => setAll(false)} disabled={count === 0}>全不選</button>
      </div>
    </div>

    <!-- 每次新的請求重建清單，捲動位置回到最上面。 -->
    {#key request}
    <VirtualList items={rows} {heightOf} keyOf={(row) => row.key} label="要勾選的項目" role="group" class="list">
      {#snippet row(entry, _index, style)}
        {#if entry.kind === 'group'}
          {@const picked = groupCount(entry.ids)}
          <div class="group" {style}>
            <span class="group-name">{entry.label}</span>
            <span class="xsmall muted mono">{picked}／{entry.ids.length}</span>
            <span class="spacer"></span>
            <button
              class="btn"
              onclick={() => setGroup(entry.ids, picked < entry.ids.length)}
              aria-label={`${picked < entry.ids.length ? '全選' : '全不選'}「${entry.label}」`}
            >
              {picked < entry.ids.length ? '全選' : '全不選'}
            </button>
          </div>
        {:else}
          <label class="item" {style}>
            <input
              type="checkbox"
              checked={selected.has(entry.item.id)}
              onchange={(event) => toggle(entry.item.id, event.currentTarget.checked)}
            />
            <span class="item-text">
              <span class="item-label" title={entry.item.label}>{entry.item.label}</span>
              {#if entry.item.detail}
                <span class="item-detail" title={entry.item.detail}>{entry.item.detail}</span>
              {/if}
            </span>
          </label>
        {/if}
      {/snippet}
      {#snippet empty()}
        <p class="small muted empty">沒有可以勾選的項目。</p>
      {/snippet}
    </VirtualList>
    {/key}
  </div>

  <footer class="dialog-foot">
    <span class="spacer"></span>
    <button class="btn" onclick={close}><Icon name="x" />取消</button>
    <button bind:this={confirmButton} class="btn btn--primary" onclick={confirm} disabled={count === 0}>
      <Icon name="check" />{request?.confirmLabel ?? '確定'}
    </button>
  </footer>
</dialog>

<style>
  /* 固定高度：清單在對話框裡捲動，勾選時對話框不會忽大忽小。 */
  .dialog {
    width: min(640px, calc(100vw - 32px));
    height: min(640px, calc(100vh - 32px));
    max-height: calc(100vh - 32px);
    padding: 0;
    color: var(--c-text);
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-dialog);
    box-shadow: var(--shadow-dialog);
  }

  .dialog[open] {
    display: flex;
    flex-direction: column;
    animation: dialog-in 220ms cubic-bezier(0.2, 0.9, 0.3, 1.15);
  }

  .dialog[open].is-closing {
    animation: dialog-out 140ms ease-in forwards;
  }

  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.96);
    }
  }

  @keyframes dialog-out {
    to {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dialog[open],
    .dialog[open].is-closing {
      animation: none;
    }
  }

  .dialog::backdrop {
    background: none;
  }

  .dialog-head,
  .dialog-foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  /* Claude Design 定案的對話框：標題列左側多留空間。 */
  .dialog-head {
    padding: 16px 16px 16px 24px;
    border-bottom: 1px solid var(--c-border);
  }

  .dialog-foot {
    padding: 14px 16px 14px 20px;
    border-top: 1px solid var(--c-border);
  }

  .title,
  .plugin-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    flex: 0 1 auto;
  }

  .plugin-name {
    flex: 0 1 auto;
  }

  .dialog-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    flex: 1 1 auto;
    min-height: 0;
    padding: var(--space-4) var(--space-4) var(--space-3) var(--space-5);
  }

  .message {
    overflow-wrap: anywhere;
    line-height: var(--lh-tight);
  }

  .bar {
    flex: none;
  }

  .dialog-body :global(.list) {
    flex: 1 1 auto;
    min-height: 0;
    border-top: 1px solid var(--c-border);
  }

  .group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2) 0 0;
    border-bottom: 1px solid var(--c-border);
  }

  .group-name {
    min-width: 0;
    overflow: hidden;
    font-size: var(--fs-sm);
    font-weight: 700;
    color: var(--c-text-strong);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-2);
    border-radius: var(--radius-item);
    cursor: pointer;
    transition: background-color 120ms ease;
  }

  .item:hover {
    background: var(--c-control);
  }

  .item input {
    flex: none;
  }

  .item-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .item-label,
  .item-detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-label {
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .item-detail {
    font-size: var(--fs-xs);
    color: var(--c-text-dim);
  }

  .empty {
    padding: var(--space-3) 0;
  }
</style>
