<script lang="ts">
  import Icon from './Icon.svelte';
  import { openExternalUrl } from '../lib/api';
  import { parseMarkdown, withoutDownloads, type Span } from '../lib/markdown';
  import { reducedMotion } from '../lib/press';
  import { releaseNotes } from '../state/releaseNotes.svelte';
  import { toasts } from '../state/toasts.svelte';
  import { RELEASES_URL } from '../state/update.svelte';

  /**
   * 查看更新內容：更新完成的提醒（或設定的「更新內容」）打開。
   * 列出舊版（不含）到這一版（含）每一版的 GitHub 發布說明，新版在前，內容與 Release 頁相同，
   * 只是不顯示下載說明（要下載的人按「在 GitHub 查看」）。
   */

  let dialog: HTMLDialogElement | null = $state(null);
  let body: HTMLDivElement | null = $state(null);
  let closing = $state(false);
  let pressedOutside = false;
  const CLOSE_MS = 140;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  const parsed = $derived(
    releaseNotes.notes.map((note) => ({ note, blocks: withoutDownloads(parseMarkdown(note.body)) })),
  );

  const rangeText = $derived.by(() => {
    const { from, to } = releaseNotes;
    if (!to) return '';
    return from && from !== to ? `從 v${from} 更新到 v${to}` : `v${to}`;
  });

  /** 只有一版就開那一版的頁面，多版開 Releases 總覽。 */
  const githubUrl = $derived(releaseNotes.notes.length === 1 ? releaseNotes.notes[0].url : RELEASES_URL);

  $effect(() => {
    if (!dialog) return;
    if (releaseNotes.open) {
      clearTimeout(closeTimer);
      closing = false;
      if (!dialog.open) {
        dialog.showModal();
        if (body) body.scrollTop = 0;
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

  function close() {
    releaseNotes.close();
  }

  function isOutside(event: MouseEvent): boolean {
    if (!dialog || event.target !== dialog) return false;
    const rect = dialog.getBoundingClientRect();
    return (
      event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom
    );
  }

  async function openLink(event: MouseEvent, url: string) {
    event.preventDefault();
    try {
      await openExternalUrl(url);
    } catch (error) {
      toasts.show({
        id: 'release-notes-link',
        tone: 'error',
        title: '無法開啟連結',
        body: typeof error === 'string' ? error : String(error),
      });
    }
  }

  function formatDate(iso: string): string {
    const date = new Date(iso);
    if (!iso || Number.isNaN(date.getTime())) return '';
    const pad = (value: number) => String(value).padStart(2, '0');
    return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())}`;
  }
</script>

{#snippet spanText(span: Span)}
  {#if span.code}<code>{span.text}</code>{:else if span.strong}<strong>{span.text}</strong>{:else}{span.text}{/if}
{/snippet}

{#snippet inline(spans: Span[])}
  {#each spans as span, index (index)}
    {#if span.href}
      {@const href = span.href}
      <a {href} target="_blank" rel="noopener noreferrer" onclick={(event) => void openLink(event, href)}
        >{@render spanText(span)}</a
      >
    {:else}
      {@render spanText(span)}
    {/if}
  {/each}
{/snippet}

<dialog
  bind:this={dialog}
  class="dialog"
  class:is-closing={closing}
  aria-labelledby="release-notes-title"
  onclose={() => releaseNotes.close()}
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
    <h2 id="release-notes-title">更新內容</h2>
    {#if rangeText}<span class="muted small range">{rangeText}</span>{/if}
    <span class="spacer"></span>
    <button class="btn btn--round" onclick={close} aria-label="關閉">
      <Icon name="x" />
    </button>
  </header>

  <div bind:this={body} class="dialog-body scroll" aria-busy={releaseNotes.loading}>
    {#if releaseNotes.loading}
      <div class="loading" role="status" aria-label="正在讀取更新內容">
        {#each [0, 1] as group (group)}
          <div class="loading-group">
            <span class="skeleton skeleton--line loading-title"></span>
            {#each [86, 70, 92, 58] as width, index (index)}
              <span class="skeleton skeleton--line" style={`width:${width - group * 8}%`}></span>
            {/each}
          </div>
        {/each}
      </div>
    {:else if releaseNotes.error}
      <div class="alert alert--error" role="alert">
        <div class="alert-title">讀不到更新內容</div>
        <p>{releaseNotes.error}</p>
        <p class="muted help">可以稍後再試一次，或到 GitHub 發布頁查看。</p>
      </div>
    {:else}
      {#each parsed as { note, blocks } (note.version)}
        <section class="release" aria-labelledby={`release-${note.version}`}>
          <div class="release-head">
            <h3 id={`release-${note.version}`}>{note.name}</h3>
            {#if note.version === releaseNotes.to}<span class="badge badge--quiet">目前版本</span>{/if}
            <span class="spacer"></span>
            <span class="muted xsmall mono">{formatDate(note.publishedAt)}</span>
          </div>
          {#if blocks.length === 0}
            <p class="muted small">這一版沒有寫更新說明。</p>
          {:else}
            <div class="md">
              {#each blocks as block, index (index)}
                {#if block.kind === 'heading'}
                  {#if block.level <= 2}
                    <h4>{@render inline(block.spans)}</h4>
                  {:else}
                    <h5>{@render inline(block.spans)}</h5>
                  {/if}
                {:else if block.kind === 'paragraph'}
                  <p>{@render inline(block.spans)}</p>
                {:else if block.kind === 'list'}
                  <svelte:element this={block.ordered ? 'ol' : 'ul'}>
                    {#each block.items as item, itemIndex (itemIndex)}
                      <li>{@render inline(item)}</li>
                    {/each}
                  </svelte:element>
                {:else if block.kind === 'code'}
                  <pre><code>{block.text}</code></pre>
                {:else}
                  <hr />
                {/if}
              {/each}
            </div>
          {/if}
        </section>
      {/each}
    {/if}
  </div>

  <footer class="dialog-foot">
    {#if releaseNotes.notes.length > 1 && !releaseNotes.loading}
      <span class="muted xsmall">共 {releaseNotes.notes.length} 版</span>
    {/if}
    <span class="spacer"></span>
    {#if releaseNotes.error && !releaseNotes.loading}
      <button class="btn" onclick={() => void releaseNotes.load()}><Icon name="refresh-cw" />再試一次</button>
    {/if}
    <button class="btn" onclick={(event) => void openLink(event, githubUrl)}>
      <Icon name="external-link" />在 GitHub 查看
    </button>
    <button class="btn btn--primary" onclick={close}><Icon name="check" />知道了</button>
  </footer>
</dialog>

<style>
  .dialog {
    width: min(640px, calc(100vw - 32px));
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
    flex-wrap: wrap;
    padding: 14px 16px 14px 20px;
    border-top: 1px solid var(--c-border);
  }

  .dialog-body {
    min-height: 0;
    padding: var(--space-4) var(--space-4) var(--space-5) var(--space-5);
  }

  .range {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* 很快就讀完時不閃骨架：延遲一小段才出現。 */
  .loading {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    animation: veil-in 160ms ease-out 120ms both;
  }

  .loading-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .loading-title {
    width: 40%;
    height: 14px;
    margin-bottom: var(--space-1);
  }

  .alert p {
    margin: 0;
  }

  .alert p + p {
    margin-top: var(--space-1);
  }

  .release + .release {
    margin-top: var(--space-5);
    padding-top: var(--space-5);
    border-top: 1px solid var(--c-border);
  }

  .release-head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .release-head h3 {
    min-width: 0;
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 700;
    color: var(--c-text-strong);
    overflow-wrap: anywhere;
  }

  .md {
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
    color: var(--c-text);
    overflow-wrap: anywhere;
  }

  .md h4,
  .md h5 {
    margin: var(--space-4) 0 var(--space-2);
    font-weight: 700;
    line-height: var(--lh-tight);
  }

  .md h4 {
    font-size: var(--fs-md);
    color: var(--c-text-strong);
  }

  .md h5 {
    font-size: var(--fs-sm);
    color: var(--c-text-dim);
  }

  .md > :first-child {
    margin-top: 0;
  }

  .md p,
  .md ul,
  .md ol,
  .md pre {
    margin: 0 0 var(--space-3);
  }

  .md > :last-child {
    margin-bottom: 0;
  }

  .md ul,
  .md ol {
    padding-left: var(--space-5);
  }

  .md li + li {
    margin-top: var(--space-1);
  }

  .md strong {
    font-weight: 700;
    color: var(--c-text-strong);
  }

  .md code {
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--c-control);
    font-family: var(--font-mono);
    font-size: 0.92em;
  }

  .md pre {
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--c-control);
    overflow-x: auto;
  }

  .md pre code {
    padding: 0;
    background: none;
    white-space: pre;
  }

  .md a {
    color: var(--c-primary);
    text-decoration: underline;
  }

  .md hr {
    margin: var(--space-4) 0;
    border: none;
    border-top: 1px solid var(--c-border);
  }

  @media (prefers-reduced-motion: reduce) {
    .dialog[open],
    .dialog[open].is-closing,
    .loading {
      animation: none;
    }
  }
</style>
