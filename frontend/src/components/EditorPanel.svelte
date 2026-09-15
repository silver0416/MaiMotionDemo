<script lang="ts">
  import { STATUS_HINT, STATUS_LABEL, SUPPORT_NOTES } from '../lib/contract';
  import { formatClock } from '../lib/format';
  import { SAMPLES } from '../lib/samples';
  import { byteOffsetToIndex } from '../lib/text';
  import { playback } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import type { AnalyzeStatus, Diagnostic } from '../lib/types';

  let textarea: HTMLTextAreaElement | null = $state(null);
  let loadingSampleId = $state<string | null>(null);

  const diagnostics = $derived<Diagnostic[]>(session.response?.diagnostics ?? []);
  const status = $derived<AnalyzeStatus | null>((session.response?.status as AnalyzeStatus) ?? null);
  const analyzing = $derived(session.phase === 'analyzing');

  async function generate() {
    await session.analyze();
  }

  async function loadSample(id: string) {
    loadingSampleId = id;
    const sample = SAMPLES.find((item) => item.id === id);
    if (sample) await session.loadSample(sample);
    loadingSampleId = null;
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
    <div class="section-title">
      <span>simai 原文</span>
      {#if session.stale && session.result}
        <span class="badge badge--quiet">結果與目前原文不同</span>
      {/if}
    </div>

    <div class="stack-sm">
      <label class="field-label" for="simai-source">編輯區（重新生成不會改寫這裡的內容）</label>
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
        <p class="field-hint">
          目前是瀏覽器預覽，沒有 Rust 核心可呼叫，因此「生成」停用。
          下方範例顯示的是 fixtures 內預先產生的核心輸出，不是對編輯區內容的分析。
        </p>
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
          disabled={analyzing || loadingSampleId !== null}
        >
          <span class="sample-title">{sample.title}</span>
          <span class="sample-source mono xsmall">{sample.source}</span>
          <span class="sample-desc xsmall muted">{sample.description}</span>
        </button>
      {/each}
    </div>
    <p class="field-hint">
      {session.desktop
        ? '載入後會把原文與參數填入編輯區，並立即交給 Rust 重新分析。'
        : '載入後會把原文與參數填入編輯區，並顯示 fixtures 內附的核心輸出（範例模式）。'}
    </p>
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
      <p class="small">分析中，請稍候。核心一次只處理一份譜面，舊的回應會被丟棄。</p>
    {:else if !session.result}
      <p class="small muted">
        尚未產生結果。{session.desktop ? '輸入 simai 原文後按「生成」，' : '選一個範例，'}即可看到譜面與雙手動作。
      </p>
    {:else if status}
      <div class="stack-sm">
        <div class="row">
          <span class="badge" class:badge--danger={status !== 'ok'} class:badge--live={status === 'ok'}>
            {STATUS_LABEL[status] ?? status}
          </span>
          <span class="muted xsmall mono">schema v{session.response?.schemaVersion}</span>
        </div>
        <p class="small">{STATUS_HINT[status] ?? ''}</p>
      </div>
    {/if}

    {#if diagnostics.length > 0}
      <ul class="diagnostics">
        {#each diagnostics as diagnostic, index (index)}
          <li class="diagnostic">
            <div class="row row-wrap">
              <span class="badge" class:badge--danger={diagnostic.severity === 'error'}>
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
            {#if session.stale && diagnostic.sourceSpan}
              <p class="xsmall muted">編輯區內容已變更，位置可能不再對應，請重新生成。</p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="section">
    <div class="section-title"><span>支援範圍</span></div>
    <ul class="small support">
      {#each SUPPORT_NOTES as item (item)}
        <li>{item}</li>
      {/each}
    </ul>
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
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--c-text-dim);
  }
</style>
