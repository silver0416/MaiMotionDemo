<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import ContextMenu, { type MenuItem } from './ContextMenu.svelte';
  import { records, recordDate, recordTitle, type ChartRecord } from '../state/records.svelte';
  import { errorLog } from '../state/errorLog.svelte';
  import { squash } from '../lib/press';
  import { session } from '../state/session.svelte';

  interface Props {
    onCreate: () => void;
    onSearch: () => void;
    onCollapse: () => void;
    onEdit: (record: ChartRecord) => void;
    onSettings: () => void;
  }

  let { onCreate, onSearch, onCollapse, onEdit, onSettings }: Props = $props();

  const MENU_ITEMS: MenuItem[] = [
    { id: 'edit', label: '查看／編輯', icon: 'pencil' },
    { id: 'delete', label: '刪除', icon: 'trash', danger: true },
  ];

  /** 三點按鈕與右鍵共用同一個選單；fromButton 決定關閉後焦點回到哪裡。 */
  let menu = $state<{ id: string; x: number; y: number; align: 'start' | 'end'; fromButton: boolean } | null>(
    null,
  );
  const menuRecord = $derived(menu ? records.get(menu.id) : null);

  function openMenuFromButton(record: ChartRecord, event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    if (menu?.id === record.id && menu.fromButton) {
      menu = null;
      return;
    }
    confirmingId = null;
    menu = { id: record.id, x: rect.right, y: rect.bottom + 4, align: 'end', fromButton: true };
  }

  function openContextMenu(record: ChartRecord, event: MouseEvent) {
    event.preventDefault();
    confirmingId = null;
    menu = { id: record.id, x: event.clientX, y: event.clientY, align: 'start', fromButton: false };
  }

  async function closeMenu(focusBack: boolean) {
    const current = menu;
    menu = null;
    if (!focusBack || !current || !listEl) return;
    await tick();
    const row = listEl.querySelector(`[data-record-id="${CSS.escape(current.id)}"]`);
    row?.querySelector<HTMLElement>(current.fromButton ? '.more' : '.open')?.focus();
  }

  async function selectMenu(action: string) {
    const record = menuRecord;
    menu = null;
    if (!record) return;
    if (action === 'edit') {
      onEdit(record);
    } else if (action === 'delete') {
      confirmingId = record.id;
      await tick();
      // 焦點先放在「取消」，誤按 Enter 不會直接刪掉。
      listEl
        ?.querySelector<HTMLElement>(
          `[data-record-id="${CSS.escape(record.id)}"] .confirm .btn:not(.btn--danger)`,
        )
        ?.focus();
    }
  }

  /** 列上按 Shift+F10 或選單鍵，和右鍵一樣打開選單。 */
  function onRowKeydown(record: ChartRecord, event: KeyboardEvent) {
    if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
      event.preventDefault();
      const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
      confirmingId = null;
      menu = { id: record.id, x: rect.left + 24, y: rect.bottom, align: 'start', fromButton: false };
    }
  }

  /** 刪除要按兩次：第一次只把該列切成確認狀態。 */
  let confirmingId = $state<string | null>(null);

  const analyzing = $derived(session.phase === 'analyzing');

  let listEl: HTMLElement | null = $state(null);

  // 從搜尋視窗跳到既有紀錄時，把那一列捲進可視範圍。
  $effect(() => {
    const id = records.activeId;
    if (!id || !listEl) return;
    listEl.querySelector(`[data-record-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  async function open(record: ChartRecord) {
    confirmingId = null;
    if (analyzing) return;
    records.activeId = record.id;
    await session.load(record.source);
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
    <button class="btn btn--ghost btn--wide" onclick={onSearch}>
      <span class="lead lead--disc"><Icon name="search" size={14} /></span>搜尋譜面
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
    {#if !records.loaded && records.items.length === 0}
      <p class="empty small muted">讀取紀錄中…</p>
    {:else if records.items.length === 0}
      <p class="empty small muted">還沒有譜面。按「新增」貼上 simai 或 maidata.txt，或從 Majdata、simai Wiki 搜尋匯入。</p>
    {:else}
      <ul class="list" bind:this={listEl}>
        {#each records.items as record (record.id)}
          {@const active = records.activeId === record.id}
          <li
            class="item"
            class:is-active={active}
            class:is-menu={menu?.id === record.id}
            data-record-id={record.id}
            oncontextmenu={(event) => confirmingId !== record.id && openContextMenu(record, event)}
          >
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
                onkeydown={(event) => onRowKeydown(record, event)}
                onclick={() => open(record)}
                disabled={analyzing || !session.desktop}
                aria-current={active ? 'true' : undefined}
              >
                <span class="item-title">
                  <Icon name="file-text" size={13} />
                  <span class="item-title-text">{recordTitle(record)}</span>
                </span>
                <span class="item-preview xsmall mono">{recordDate(record)}</span>
              </button>
              <button
                class="more btn btn--icon"
                onclick={(event) => openMenuFromButton(record, event)}
                aria-label={`${recordTitle(record)} 的更多動作`}
                aria-haspopup="menu"
                aria-expanded={menu?.id === record.id}
                title="更多動作（也可以在列上按右鍵）"
              >
                <Icon name="more-horizontal" size={16} />
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="records-foot">
    <button class="btn btn--ghost btn--wide" onclick={onSettings}>
      <span class="lead"><Icon name="settings" size={16} /></span>設定
      {#if errorLog.entries.length > 0}
        <span class="spacer"></span>
        <span class="badge badge--danger" title="有尚未回報的錯誤紀錄">
          {errorLog.entries.length}<span class="sr-only"> 筆錯誤紀錄</span>
        </span>
      {/if}
    </button>
  </div>
</div>

{#if menu && menuRecord}
  <ContextMenu
    items={MENU_ITEMS}
    x={menu.x}
    y={menu.y}
    align={menu.align}
    label={`${recordTitle(menuRecord)} 的動作`}
    onSelect={selectMenu}
    onClose={closeMenu}
  />
{/if}

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

  .item-title :global(svg) {
    flex: none;
  }

  .item-title-text {
    min-width: 0;
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

  /* 更多動作鈕平常隱藏；滑過、鍵盤聚焦或選單開著時一定看得到。 */
  .more {
    position: absolute;
    top: 50%;
    right: var(--space-1);
    transform: translateY(-50%);
    min-height: 28px;
    min-width: 28px;
    padding: 0;
    visibility: hidden;
  }

  .item:hover .more,
  .item:focus-within .more,
  .item.is-menu .more {
    visibility: visible;
  }

  .item.is-menu {
    background: var(--c-control);
  }

  .more {
    background: none;
    border-color: transparent;
    color: var(--c-text-dim);
  }

  .more:hover:not(:disabled),
  .item.is-menu .more {
    color: var(--c-text);
    background: var(--c-control-hover);
  }

  .records-foot {
    padding: var(--space-2);
    border-top: 1px solid var(--c-border);
  }

  .records-foot .btn {
    min-height: 34px;
    padding: 0 var(--space-2);
    gap: var(--space-3);
    font-size: var(--fs-md);
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
