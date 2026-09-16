<script lang="ts">
  import Icon from './Icon.svelte';
  import { formatClock } from '../lib/format';
  import { playback } from '../state/playback.svelte';
  import { session } from '../state/session.svelte';
  import type { Diagnostic } from '../lib/types';

  interface Props {
    diagnostics: Diagnostic[];
    /** 有原文輸入框時才提供：把游標選到診斷的原文位置。 */
    onLocate?: (diagnostic: Diagnostic) => void;
    /** 診斷屬於已套用到盤面的結果時才允許跳時間或選音符。 */
    linked?: boolean;
  }

  let { diagnostics, onLocate, linked = false }: Props = $props();
</script>

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
        {#if diagnostic.sourceSpan && onLocate}
          <button class="linkish xsmall" onclick={() => onLocate(diagnostic)}>
            <Icon name="text-cursor" size={12} />標出原文位置
          </button>
        {/if}
        {#if linked && diagnostic.timeSeconds !== null}
          <button class="linkish xsmall" onclick={() => playback.seek(diagnostic.timeSeconds ?? 0)}>
            <Icon name="clock" size={12} />跳到該時間
          </button>
        {/if}
        {#if linked}
          {#each diagnostic.noteIds as noteId (noteId)}
            <button class="linkish xsmall" onclick={() => session.selectNote(noteId)}>
              <Icon name="crosshair" size={12} />{noteId}
            </button>
          {/each}
        {/if}
      </div>
    </li>
  {/each}
</ul>

<style>
  .diagnostics {
    list-style: none;
    padding: 0;
    margin: 0;
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
</style>
