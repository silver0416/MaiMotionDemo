<script lang="ts">
  import DiscStage from './components/DiscStage.svelte';
  import Transport from './components/Transport.svelte';
  import EditorPanel from './components/EditorPanel.svelte';
  import SolutionPanel from './components/SolutionPanel.svelte';
  import NoteInspector from './components/NoteInspector.svelte';
  import ConfigPanel from './components/ConfigPanel.svelte';
  import ViewPanel from './components/ViewPanel.svelte';
  import { STATUS_LABEL } from './lib/contract';
  import { findSample } from './lib/samples';
  import { playback } from './state/playback.svelte';
  import { session } from './state/session.svelte';
  import type { AnalyzeStatus } from './lib/types';

  type Tab = 'edit' | 'solution' | 'note' | 'config' | 'view';

  const TABS: { id: Tab; label: string }[] = [
    { id: 'edit', label: '編輯' },
    { id: 'solution', label: '方案' },
    { id: 'note', label: '音符' },
    { id: 'config', label: '參數' },
    { id: 'view', label: '顯示' },
  ];

  let tab = $state<Tab>('edit');

  const status = $derived<AnalyzeStatus | null>((session.response?.status as AnalyzeStatus) ?? null);
  const sample = $derived(findSample(session.result?.sampleId ?? null));
  const isSample = $derived(session.result?.origin === 'sample');

  // 單一播放時鐘：整個應用只有這一個 requestAnimationFrame 迴圈。
  $effect(() => {
    let frame = requestAnimationFrame(function loop(now: number) {
      playback.frame(now);
      frame = requestAnimationFrame(loop);
    });
    return () => cancelAnimationFrame(frame);
  });

  // 在盤面點到音符時，右側自動切到音符明細。
  $effect(() => {
    if (session.selectedNoteId) tab = 'note';
  });

  function isTyping(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    const name = target.tagName;
    return (
      name === 'INPUT' || name === 'TEXTAREA' || name === 'SELECT' || target.isContentEditable
    );
  }

  function onKeydown(event: KeyboardEvent) {
    if (isTyping(event.target) || event.ctrlKey || event.metaKey || event.altKey) return;
    switch (event.key) {
      case ' ':
        event.preventDefault();
        playback.toggle();
        break;
      case 'ArrowLeft':
        event.preventDefault();
        playback.nudge(event.shiftKey ? -1 : -0.1);
        break;
      case 'ArrowRight':
        event.preventDefault();
        playback.nudge(event.shiftKey ? 1 : 0.1);
        break;
      case 'Home':
        event.preventDefault();
        playback.reset();
        break;
      case 'l':
      case 'L':
        event.preventDefault();
        playback.setLoop(!playback.loopEnabled);
        break;
      default:
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <header class="app-bar">
    <h1>MaiMotionDemo</h1>
    <span class="muted small">simai 雙手動作檢視器</span>
    <span class="spacer"></span>
    {#if session.desktop}
      <span class="badge badge--live">桌面版・呼叫 Rust 核心</span>
    {:else}
      <span class="badge badge--sample">瀏覽器預覽・僅能顯示範例</span>
    {/if}
    {#if status}
      <span class="badge" class:badge--danger={status !== 'ok'} class:badge--quiet={status === 'ok'}>
        {STATUS_LABEL[status] ?? status}
      </span>
    {/if}
    {#if session.phase === 'analyzing'}
      <span class="badge badge--quiet">分析中…</span>
    {/if}
  </header>

  <main class="layout">
    <section class="stage-column">
      {#if isSample}
        <div class="alert alert--warn banner">
          <span class="badge badge--sample">範例模式</span>
          <span>
            畫面顯示的是 fixtures 內預先產生的核心輸出{sample ? `（${sample.title}）` : ''}，
            不是對編輯區內容的分析。正式生成必須在桌面版呼叫 Rust。
          </span>
        </div>
      {/if}
      {#if session.stale && session.result}
        <div class="alert banner">
          <span class="badge badge--quiet">結果已過期</span>
          <span>編輯區的原文或參數已變更，下面顯示的仍是上一次的結果。</span>
        </div>
      {/if}
      {#if session.errorMessage}
        <div class="alert alert--error banner">
          <span class="badge badge--danger">未完成</span>
          <span>{session.errorMessage}</span>
        </div>
      {/if}

      <div class="stage-area">
        <DiscStage />
      </div>
      <div class="transport-slot">
        <Transport />
      </div>
    </section>

    <aside class="side-column">
      <div class="tabbar" role="group" aria-label="側欄面板">
        {#each TABS as item (item.id)}
          <button
            class="btn tab"
            class:is-active={tab === item.id}
            aria-pressed={tab === item.id}
            onclick={() => (tab = item.id)}
          >
            {item.label}
          </button>
        {/each}
      </div>
      <div class="side-body scroll">
        {#if tab === 'edit'}
          <EditorPanel />
        {:else if tab === 'solution'}
          <SolutionPanel />
        {:else if tab === 'note'}
          <NoteInspector />
        {:else if tab === 'config'}
          <ConfigPanel />
        {:else}
          <ViewPanel />
        {/if}
      </div>
    </aside>
  </main>
</div>

<style>
  .app {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    background: var(--c-bg);
  }

  .app-bar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    background: var(--c-panel);
    border-bottom: 1px solid var(--c-border);
  }

  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) clamp(320px, 27vw, 400px);
    grid-template-rows: minmax(0, 1fr);
    gap: var(--space-3);
    padding: var(--space-3);
    min-height: 0;
  }

  /* 直向 flex：橫幅可能有 0–3 條，盤面一律吃剩下的空間，
     播放列固定不縮，因此任何視窗尺寸下播放控制都留在畫面內。 */
  .stage-column {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    min-width: 0;
  }

  .transport-slot {
    flex: none;
  }

  /* 盤面所在的儲存格：可用高度扣掉播放列之後剩下的空間，
     DiscStage 以絕對定位置中並等比例縮放，播放列因此永遠留在視窗內。 */
  .stage-area {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
  }

  .banner {
    display: flex;
    flex: none;
    align-items: baseline;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .side-column {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
  }

  .tabbar {
    display: flex;
    gap: var(--space-1);
    padding: var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }

  .tab {
    flex: 1 1 0;
    min-width: 0;
  }

  .side-body {
    min-height: 0;
  }

  /* 只有真的很窄（低於 900px）才改成上下排列並允許整頁捲動。
     一般桌面視窗（含 125%／150% 縮放後的 1440×960）維持左右排列，播放列不會被推到視窗外。 */
  @media (max-width: 899px) {
    .layout {
      display: block;
      overflow: auto;
    }

    /* 窄視窗改用 flex 直向排列，高度直接由內容決定，避免 grid 的內在尺寸把盤面壓扁。 */
    .stage-column {
      display: flex;
      flex-direction: column;
      margin-bottom: var(--space-3);
    }

    .stage-area {
      flex: 0 0 auto;
      height: min(52vh, 460px);
      min-height: 260px;
    }

    .side-column {
      min-height: 360px;
      height: min(70vh, 620px);
    }
  }
</style>
