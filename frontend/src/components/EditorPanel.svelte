<script lang="ts">
  import { STATUS_HINT, STATUS_LABEL, SUPPORT_NOTES } from '../lib/contract';
  import { formatClock } from '../lib/format';
  import { SAMPLES } from '../lib/samples';
  import { byteOffsetToIndex } from '../lib/text';
  import { playback } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import type { AnalyzeStatus, Diagnostic } from '../lib/types';

  let textarea: HTMLTextAreaElement | null = $state(null);

  const diagnostics = $derived<Diagnostic[]>(session.response?.diagnostics ?? []);
  const status = $derived<AnalyzeStatus | null>((session.response?.status as AnalyzeStatus) ?? null);
  const analyzing = $derived(session.phase === 'analyzing');

  async function generate() {
    await session.analyze();
  }

  async function loadSample(id: string) {
    const sample = SAMPLES.find((item) => item.id === id);
    if (sample) await session.loadSample(sample);
  }

  function jumpTo(diagnostic: Diagnostic) {
    const span = diagnostic.sourceSpan;
    if (!span || !textarea) return;
    const start = byteOffsetToIndex(session.source, span.start);
    const end = byteOffsetToIndex(session.source, span.end);
    textarea.focus();
    textarea.setSelectionRange(start, Math.max(end, start + 1));
  }
</script>

<div class="stack">
  <section class="section">
    <div class="section-title"><span>simai 原文</span></div>

    <div class="stack-sm">
      <label class="field-label sr-only" for="simai-source">simai 原文</label>
      <textarea
        id="simai-source"
        class="textarea"
        rows="5"
        spellcheck="false"
        bind:this={textarea}
        bind:value={session.source}
        placeholder="(120)&#123;4&#125;1,2,3,4,E"
      ></textarea>

      <div class="row row-wrap">
        <button class="btn btn--primary" onclick={generate} disabled={analyzing || !session.desktop}>
          {analyzing ? '分析中…' : '生成'}
        </button>
        <button class="btn" onclick={() => session.clearResult()} disabled={!session.result}>
          清除結果
        </button>
        <span class="spacer"></span>
        <span class="muted xsmall">{session.source.length} 字元</span>
      </div>

      {#if !session.desktop}
        <p class="field-hint">瀏覽器預覽沒有 Rust 核心，只能看下方範例的預先輸出。</p>
      {/if}
    </div>
  </section>

  <section class="section">
    <div class="section-title"><span>範例</span></div>
    <div class="samples">
      {#each SAMPLES as sample (sample.id)}
        <button
          class="sample"
          class:is-current={session.result?.sampleId === sample.id}
          onclick={() => loadSample(sample.id)}
          disabled={analyzing}
        >
          <span class="sample-title">{sample.title}</span>
          <span class="sample-source mono xsmall">{sample.source}</span>
          <span class="sample-desc xsmall muted">{sample.description}</span>
        </button>
      {/each}
    </div>
  </section>

  <section class="section">
    <div class="section-title"><span>狀態與診斷</span></div>

    {#if session.errorMessage}
      <div class="alert alert--error">
        <div class="alert-title">分析未完成</div>
        <p>{session.errorMessage}</p>
      </div>
    {/if}

    {#if analyzing}
      <p class="small muted">分析中…</p>
    {:else if !session.result}
      <p class="small muted">
        {session.desktop ? '輸入原文後按「生成」。' : '選一個範例。'}
      </p>
    {:else if status}
      <div class="stack-sm">
        <div class="row">
          <span class="badge" class:badge--danger={status !== 'ok'} class:badge--live={status === 'ok'}>
            {STATUS_LABEL[status] ?? status}
          </span>
          <span class="muted xsmall mono">schema v{session.response?.schemaVersion}</span>
        </div>
        {#if STATUS_HINT[status]}
          <p class="small">{STATUS_HINT[status]}</p>
        {/if}
      </div>
    {/if}

    {#if diagnostics.length > 0}
      <ul class="diagnostics">
        {#each diagnostics as diagnostic, index (index)}
          <li class="diagnostic">
            <div class="row row-wrap">
              <span
                class="badge"
                class:badge--danger={diagnostic.severity === 'error'}
                class:badge--quiet={diagnostic.severity !== 'error'}
              >
                {diagnostic.code}
              </span>
              {#if diagnostic.sourceSpan}
                <span class="mono xsmall muted">
                  第 {diagnostic.sourceSpan.line} 行、第 {diagnostic.sourceSpan.column} 欄
                </span>
              {/if}
              {#if diagnostic.timeSeconds !== null}
                <span class="mono xsmall muted">{formatClock(diagnostic.timeSeconds)}</span>
              {/if}
            </div>
            <p class="small">{diagnostic.message}</p>
            <div class="row row-wrap">
              {#if diagnostic.sourceSpan}
                <button class="linkish xsmall" onclick={() => jumpTo(diagnostic)} disabled={session.stale}>
                  在編輯區標出位置
                </button>
              {/if}
              {#if diagnostic.timeSeconds !== null}
                <button
                  class="linkish xsmall"
                  onclick={() => playback.seek(diagnostic.timeSeconds ?? 0)}
                >
                  跳到該時間
                </button>
              {/if}
              {#each diagnostic.noteIds as noteId (noteId)}
                <button class="linkish xsmall" onclick={() => session.selectNote(noteId)}>
                  {noteId}
                </button>
              {/each}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="section">
    <div class="section-title"><span>支援語法</span></div>
    <dl class="support">
      {#each SUPPORT_NOTES as item (item.title)}
        <dt>{item.title}</dt>
        <dd class="mono">{item.body}</dd>
      {/each}
    </dl>
  </section>
</div>

<style>
  .samples {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

  .sample {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    overflow: hidden;
    padding: var(--space-2);
    text-align: left;
    background: var(--c-control);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .sample:hover:not(:disabled) {
    background: var(--c-control-hover);
  }

  .sample:disabled {
    cursor: not-allowed;
    color: var(--c-text-dim);
  }

  .sample.is-current {
    border-color: var(--c-accent);
  }

  .sample-title {
    font-size: var(--fs-sm);
    font-weight: 600;
  }

  .sample-source {
    color: var(--c-text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sample-desc {
    line-height: var(--lh-tight);
  }

  .diagnostics {
    list-style: none;
    padding: 0;
    margin: var(--space-3) 0 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .diagnostic {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding-top: var(--space-3);
    border-top: 1px solid var(--c-border);
  }

  .diagnostic:first-child {
    padding-top: 0;
    border-top: none;
  }

  .support {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    font-size: var(--fs-xs);
  }

  .support dt {
    color: var(--c-text-dim);
  }

  .support dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
