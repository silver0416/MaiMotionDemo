<script lang="ts">
  import { fly } from 'svelte/transition';
  import Icon, { type IconName } from './Icon.svelte';
  import { toasts, type ToastTone } from '../state/toasts.svelte';

  const TONE_ICON: Record<ToastTone, IconName> = {
    busy: 'loader',
    ok: 'circle-check',
    warn: 'triangle-alert',
    error: 'circle-alert',
    info: 'info',
  };

  const reduceMotion =
    typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
</script>

<!-- 分析狀態不擋住盤面：固定在右下角，錯誤類通知用 alert 讓輔助技術立即朗讀。 -->
<section class="toaster" aria-label="狀態通知">
  {#each toasts.items as toast (toast.id)}
    <div
      class="toast toast--{toast.tone}"
      role={toast.tone === 'error' || toast.tone === 'warn' ? 'alert' : 'status'}
      transition:fly={{ x: reduceMotion ? 0 : 24, duration: reduceMotion ? 0 : 180 }}
    >
      <span class="toast-icon">
        <Icon name={TONE_ICON[toast.tone]} size={16} spin={toast.tone === 'busy'} />
      </span>
      <div class="toast-text">
        <div class="toast-title">{toast.title}</div>
        {#if toast.body}
          <p class="toast-body">{toast.body}</p>
        {/if}
        {#if toast.progress !== undefined}
          {@const value = Math.min(100, Math.max(0, toast.progress))}
          <div
            class="toast-progress"
            role="progressbar"
            aria-label={toast.title}
            aria-valuenow={value}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <div class="toast-progress-fill" style={`transform:scaleX(${value / 100})`}></div>
          </div>
        {/if}
        {#if toast.action}
          {@const action = toast.action}
          <button class="btn toast-action" onclick={() => action.run()}>
            <Icon name={action.icon ?? 'refresh-cw'} />{action.label}
          </button>
        {/if}
      </div>
      {#if toast.tone !== 'busy'}
        <button class="toast-close btn btn--icon" onclick={() => toasts.dismiss(toast.id)} aria-label="關閉通知">
          <Icon name="x" size={14} />
        </button>
      {/if}
    </div>
  {/each}
</section>

<style>
  .toaster {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 20;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--space-2);
    width: min(340px, calc(100vw - 32px));
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-3);
    background: var(--c-panel);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px #000000;
    pointer-events: auto;
  }

  .toast--error {
    border-color: var(--c-danger);
  }

  .toast--warn {
    border-color: var(--c-accent);
  }

  .toast-icon {
    padding-top: 2px;
    color: var(--c-text-dim);
  }

  .toast--ok .toast-icon {
    color: var(--c-right);
  }

  .toast--warn .toast-icon {
    color: var(--c-accent);
  }

  .toast--error .toast-icon {
    color: var(--c-danger);
  }

  .toast-text {
    flex: 1 1 auto;
    min-width: 0;
  }

  .toast-title {
    font-size: var(--fs-md);
    font-weight: 600;
  }

  .toast-body {
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--c-text-dim);
    line-height: var(--lh-tight);
    overflow-wrap: anywhere;
  }

  .toast-action {
    margin-top: var(--space-2);
  }

  .toast-progress {
    height: 4px;
    margin-top: var(--space-2);
    background: var(--c-control);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  /* 用 scaleX 而不是 width：進度更新時只做合成，不重排版面。 */
  .toast-progress-fill {
    height: 100%;
    background: var(--c-right);
    transform-origin: left center;
    transition: transform 240ms ease-out;
  }

  @media (prefers-reduced-motion: reduce) {
    .toast-progress-fill {
      transition: none;
    }
  }

  .toast-close {
    min-height: 26px;
    min-width: 26px;
    padding: 0;
  }
</style>
