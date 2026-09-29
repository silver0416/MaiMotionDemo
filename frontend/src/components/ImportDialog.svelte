<script lang="ts">
  import Icon from './Icon.svelte';
  import { reducedMotion } from '../lib/press';
  import { recordTitle, records } from '../state/records.svelte';
  import {
    importItems,
    importRequest,
    previewImport,
    type ImportPreview,
    type ImportSummary,
  } from '../state/recordsIO.svelte';
  import { toasts } from '../state/toasts.svelte';

  // 內容來自 importRequest：「新增」視窗開啟或貼上的標記檔。
  const items = $derived(importRequest.items);
  const tags = $derived(importRequest.tags);
  /** 讀檔時整份放棄的項目（壞掉的 JSON、原譜缺漏等） */
  const errors = $derived(importRequest.errors);
  const open = $derived(importRequest.open);

  let dialog: HTMLDialogElement | null = $state(null);
  let preview = $state<ImportPreview[] | null>(null);
  let mode = $state<'fill' | 'replace'>('fill');
  let organize = $state(true);
  let running = $state(false);
  let summary = $state<ImportSummary | null>(null);

  const summaryText = $derived.by(() => {
    if (!summary) return '';
    const parts = [`新增 ${summary.created} 筆、合併 ${summary.merged} 筆紀錄`, `手順新增 ${summary.added} 顆`];
    if (summary.conflicts > 0) {
      parts.push(`${summary.conflicts} 顆手順不同（${mode === 'fill' ? '保留你的' : '已改用匯入的'}）`);
    }
    if (summary.markers > 0) parts.push(`加入 ${summary.markers} 個時間軸標籤`);
    return `${parts.join('，')}。`;
  });

  const usable = $derived(preview ? preview.filter((item) => !item.error).length : 0);
  const newCount = $derived(preview ? preview.filter((item) => !item.error && !item.existing).length : 0);
  const hasOrganize = $derived(
    items.some((item) => item.record.folder || item.record.tags.length > 0 || item.record.markers.length > 0),
  );

  let closing = $state(false);
  let pressedOutside = false;
  const CLOSE_MS = 140;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!dialog) return;
    if (open) {
      clearTimeout(closeTimer);
      closing = false;
      if (!dialog.open) {
        summary = null;
        preview = null;
        dialog.showModal();
        void previewImport(items).then((result) => (preview = result));
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

  function isOutside(event: MouseEvent): boolean {
    if (!dialog || event.target !== dialog) return false;
    const rect = dialog.getBoundingClientRect();
    return (
      event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom
    );
  }

  async function run() {
    if (running || usable === 0) return;
    running = true;
    try {
      summary = await importItems(items, tags, { mode, organize });
      const parts = [];
      if (summary.created > 0) parts.push(`新增 ${summary.created} 筆紀錄`);
      if (summary.merged > 0) parts.push(`合併 ${summary.merged} 筆`);
      toasts.show({
        id: 'records-import',
        tone: summary.failed.length > 0 ? 'warn' : 'ok',
        title: '已匯入標記檔',
        body: `${parts.join('、')}；手順新增 ${summary.added} 顆${summary.failed.length > 0 ? `，${summary.failed.length} 份未匯入` : ''}。`,
      });
    } finally {
      running = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="import-title"
  onclose={() => (importRequest.open = false)}
  oncancel={(event) => {
    event.preventDefault();
    importRequest.open = false;
  }}
  onpointerdown={(event) => (pressedOutside = isOutside(event))}
  onclick={(event) => {
    if (pressedOutside && isOutside(event)) importRequest.open = false;
    pressedOutside = false;
  }}
>
  <header class="dialog-head">
    <h2 id="import-title">匯入標記檔</h2>
    <span class="muted small">{items.length} 份</span>
    <span class="spacer"></span>
    <button class="btn btn--icon" onclick={() => (importRequest.open = false)} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div class="dialog-body scroll">
    {#if errors.length > 0}
      <div class="block">
        <div class="alert alert--warn">
          <div class="alert-title">{errors.length} 項無法讀取，已略過</div>
          <ul class="plain xsmall">
            {#each errors as error, index (index)}
              <li>{error}</li>
            {/each}
          </ul>
        </div>
      </div>
    {/if}

    <div class="block">
      {#if !preview}
        <p class="small muted"><Icon name="loader" spin size={14} /> 比對既有紀錄中…</p>
      {:else}
        <ul class="items">
          {#each preview as item, index (index)}
            <li class="entry">
              <div class="entry-head">
                <span class="entry-title">{item.title}</span>
                {#if item.error}
                  <span class="badge badge--danger">無法匯入</span>
                {:else if item.existing}
                  <span class="badge badge--quiet">合併</span>
                {:else}
                  <span class="badge">新紀錄</span>
                {/if}
              </div>
              <div class="entry-meta xsmall muted">
                {#if item.error}
                  {item.error}
                {:else}
                  {#if item.existing}<span>併入「{recordTitle(item.existing)}」</span>{/if}
                  <span>{item.labeled} 顆手順</span>
                  {#if item.folder}<span><Icon name="folder" size={11} /> {item.folder}</span>{/if}
                  {#if item.tags.length > 0}
                    <span class="tags">
                      {#each item.tags as tag (tag)}
                        <span class="tag"
                          ><span
                            class="dot"
                            style={`background:${tags.find((t) => t.name === tag)?.color ?? records.tagColor(tag)}`}
                          ></span>{tag}</span
                        >
                      {/each}
                    </span>
                  {/if}
                  {#if item.markers > 0}<span>{item.markers} 個時間軸標籤</span>{/if}
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="block stack-sm">
      <fieldset class="modes">
        <legend class="field-label">已有紀錄、同一顆音符手順不同時</legend>
        <label class="check">
          <input type="radio" name="bundle-import-mode" value="fill" bind:group={mode} />
          <span>保留我的，只補我還沒標的</span>
        </label>
        <label class="check">
          <input type="radio" name="bundle-import-mode" value="replace" bind:group={mode} />
          <span>改用匯入的</span>
        </label>
      </fieldset>
      {#if hasOrganize}
        <label class="check">
          <input type="checkbox" bind:checked={organize} />
          <span>
            套用檔案裡的資料夾、標籤與時間軸標籤
            <span class="field-hint"
              >資料夾與標籤依名稱對應，沒有就建立；已在其他資料夾的紀錄不移動。時間軸標籤同時間同名的不重複加入。</span
            >
          </span>
        </label>
      {/if}
      {#if summary}
        <div class="alert" class:alert--warn={summary.failed.length > 0} role="status">
          {summaryText}
          {#if summary.failed.length > 0}
            <ul class="plain xsmall">
              {#each summary.failed as failure, index (index)}
                <li>{failure}</li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <footer class="dialog-foot">
    {#if preview && !summary}
      <span class="muted xsmall">{newCount} 份新紀錄、{usable - newCount} 份合併到既有紀錄</span>
    {/if}
    <span class="spacer"></span>
    <button class="btn" onclick={() => (importRequest.open = false)}>
      <Icon name={summary ? 'check' : 'x'} />{summary ? '關閉' : '取消'}
    </button>
    {#if !summary}
      <button class="btn btn--primary" onclick={run} disabled={!preview || usable === 0 || running}>
        <Icon name={running ? 'loader' : 'upload'} spin={running} />匯入 {usable} 份
      </button>
    {/if}
  </footer>
</dialog>

<style>
  .dialog {
    width: min(620px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    padding: 0;
    color: var(--c-text);
    background: var(--c-panel);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
    box-shadow: 0 24px 64px #000000;
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
    padding: var(--space-3) var(--space-4);
  }

  .dialog-head {
    border-bottom: 1px solid var(--c-border);
  }

  .dialog-foot {
    border-top: 1px solid var(--c-border);
  }

  .dialog-body {
    min-height: 0;
  }

  .block {
    padding: var(--space-4);
    border-bottom: 1px solid var(--c-border);
  }

  .block:last-child {
    border-bottom: none;
  }

  .items,
  .plain {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .items {
    display: flex;
    flex-direction: column;
  }

  .entry {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--c-border);
  }

  .entry:last-child {
    border-bottom: none;
  }

  .entry-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .entry-title {
    flex: 1 1 auto;
    min-width: 0;
    font-size: var(--fs-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
  }

  .entry-meta :global(svg) {
    vertical-align: -1px;
  }

  .tags {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .tag {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 999px;
  }

  .modes {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    border: none;
  }
</style>
