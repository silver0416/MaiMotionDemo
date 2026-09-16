<script lang="ts">
  import Icon from './Icon.svelte';
  import { records, recordPreview, recordTitle, type ChartRecord } from '../state/records.svelte';
  import { squash } from '../lib/press';
  import { session } from '../state/session.svelte';

  interface Props {
    onCreate: () => void;
    onCollapse: () => void;
  }

  let { onCreate, onCollapse }: Props = $props();

  /** 刪除要按兩次：第一次只把該列切成確認狀態。 */
  let confirmingId = $state<string | null>(null);

  const analyzing = $derived(session.phase === 'analyzing');

  async function open(record: ChartRecord) {
    confirmingId = null;
    if (analyzing) return;
    records.activeId = record.id;
    await session.analyze(record.source);
  }

  function remove(record: ChartRecord) {
    const wasActive = records.activeId === record.id;
    records.remove(record.id);
    confirmingId = null;
    if (wasActive) session.clearResult();
  }
</script>

<div class="records">
  <div class="records-head">
    <button id="records-collapse" class="btn btn--ghost btn--wide" use:squash onclick={onCollapse}>
      <span class="lead"><Icon name="chevron-left" size={16} /></span>收合側欄
    </button>
    <button class="btn btn--ghost btn--wide" onclick={onCreate}>
      <span class="lead lead--disc"><Icon name="plus" size={14} /></span>新增
    </button>
  </div>

  <div class="records-title">
    <span>譜面紀錄</span>
    <span class="mono">{records.items.length}</span>
  </div>

  {#if !session.desktop}
    <p class="records-note">
      <span class="badge badge--sample">瀏覽器預覽</span>
      <span class="field-hint">沒有 Rust 核心，只能瀏覽介面，無法生成。</span>
    </p>
  {/if}

  <div class="records-body scroll">
    {#if records.items.length === 0}
      <p class="empty small muted">還沒有譜面。按「新增」貼上 simai 或 maidata.txt。</p>
    {:else}
      <ul class="list">
        {#each records.items as record (record.id)}
          {@const active = records.activeId === record.id}
          <li class="item" class:is-active={active}>
            {#if confirmingId === record.id}
              <div class="confirm">
                <span class="small">刪除這筆紀錄？</span>
                <div class="row">
                  <button class="btn btn--danger" onclick={() => remove(record)}>
                    <Icon name="trash" />刪除
                  </button>
                  <button class="btn" onclick={() => (confirmingId = null)}>
                    <Icon name="x" />取消
                  </button>
                </div>
              </div>
            {:else}
              <button
                class="open"
                onclick={() => open(record)}
                disabled={analyzing || !session.desktop}
                aria-current={active ? 'true' : undefined}
              >
                <span class="item-title mono">
                  <Icon name="file-text" size={13} />
                  {recordTitle(record)}
                </span>
                <span class="item-preview xsmall">{recordPreview(record)}</span>
              </button>
              <button
                class="remove btn btn--icon"
                onclick={() => (confirmingId = record.id)}
                aria-label={`刪除 ${recordTitle(record)}`}
              >
                <Icon name="trash" size={14} />
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  /* 瀏覽器預覽提示只在沒有核心時出現，用 flex 讓清單永遠吃掉剩下的高度。 */
  .records {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .records-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }

  .records-head .btn {
    min-height: 34px;
    padding: 0 var(--space-2);
    gap: var(--space-3);
    font-size: var(--fs-md);
  }

  /* 圖示固定佔同樣寬度，兩個入口的文字對齊。 */
  .lead {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
  }

  .lead--disc {
    color: var(--c-text);
    background: var(--c-control-hover);
    border-radius: 999px;
  }

  .records-title {
    display: flex;
    justify-content: space-between;
    padding: var(--space-3) var(--space-3) var(--space-2);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--c-text-dim);
  }

  .records-note {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: 0 var(--space-3) var(--space-2);
  }

  .records-body {
    flex: 1 1 auto;
    min-height: 0;
  }

  .empty {
    padding: var(--space-2) var(--space-3);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2) var(--space-3);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item {
    position: relative;
    display: flex;
    align-items: stretch;
    border-radius: var(--radius-md);
  }

  .item:hover,
  .item:focus-within {
    background: var(--c-control);
  }

  .item.is-active {
    background: var(--c-control-hover);
  }

  .open {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1 1 auto;
    min-width: 0;
    padding: var(--space-2);
    padding-right: 36px;
    text-align: left;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .item.is-active .open {
    border-color: var(--c-border-strong);
  }

  .open:disabled {
    cursor: default;
  }

  .item-title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--c-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item.is-active .item-title {
    color: var(--c-text-strong);
  }

  .item-preview {
    padding-left: 21px;
    color: var(--c-text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* 刪除鈕平常隱藏，但鍵盤聚焦到這一列時一定看得到。 */
  .remove {
    position: absolute;
    top: 50%;
    right: var(--space-1);
    transform: translateY(-50%);
    min-height: 28px;
    min-width: 28px;
    padding: 0;
    visibility: hidden;
  }

  .item:hover .remove,
  .item:focus-within .remove {
    visibility: visible;
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2);
    border: 1px solid var(--c-danger);
    border-radius: var(--radius-md);
  }
</style>
