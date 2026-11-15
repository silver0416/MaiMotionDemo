<script lang="ts">
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { normalizePath, SITE } from '$lib/nav';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';
  import Sheet from '$lib/shell/Sheet.svelte';
  import ThemeSwitch from '$lib/shell/ThemeSwitch.svelte';
  import { ui } from '$lib/shell/ui.svelte';
  import { PANEL_IDS } from './panel-ids';

  /**
   * App 外框底部導覽列「更多」打開的抽屜：外觀切換、下載桌面版、GitHub、目前版本。
   * （已經從主畫面開啟了，這裡不放安裝提示。）
   */
  const atDownload = $derived(normalizePath(page.url.pathname) === SITE.download);

  function close() {
    ui.moreOpen = false;
  }
</script>

<Sheet bind:open={ui.moreOpen} title="更多" size="sm" id={PANEL_IDS.more}>
  <div class="more">
    <section class="more-group" aria-labelledby="more-theme">
      <p class="more-heading" id="more-theme">外觀</p>
      <ThemeSwitch />
    </section>

    <ul class="more-list">
      <li>
        <a class="more-row" href={SITE.download} aria-current={atDownload ? 'page' : undefined} onclick={close}>
          <Icon name="download" size={18} />
          <span class="more-label">下載桌面版</span>
          <span class="more-trail"><Icon name="chevron-right" size={18} /></span>
        </a>
      </li>
      <li>
        <a class="more-row" href={SITE.github} target="_blank" rel="noopener">
          <ExtraIcon name="github" size={18} />
          <span class="more-label">GitHub<span class="sr-only">（在新分頁開啟）</span></span>
          <span class="more-trail"><Icon name="external-link" size={16} /></span>
        </a>
      </li>
    </ul>

    <p class="more-version">本手冊對應 App 版本 <span class="mono">v{SITE.appVersion}</span></p>
  </div>
</Sheet>

<style>
  .more {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-1) 0 var(--space-2);
  }

  .more-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* 小標題：同 App 的 .section-title（12px、600、字距 .04em、次要色）。 */
  .more-heading {
    padding: 0 var(--space-1);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .more-group :global(.theme-switch) {
    width: 100%;
  }

  .more-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* 清單列：和章節目錄的列同一個樣子（圓角 16、透明框保留高度；選中是 --c-control-hover 底＋--c-border-strong 框）。 */
  .more-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 44px;
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    color: var(--c-text);
    font-size: var(--fs-lg);
    text-decoration: none;
    transition:
      background-color 120ms ease,
      transform 90ms ease-out;
  }

  .more-row > :global(.icon) {
    color: var(--c-text-dim);
  }

  .more-label {
    flex: 1 1 auto;
    min-width: 0;
  }

  .more-trail {
    display: inline-flex;
    color: var(--c-text-dim);
  }

  @media (hover: hover) {
    .more-row:hover {
      background: var(--c-control);
      color: var(--c-text-strong);
    }
  }

  .more-row:active {
    background: var(--c-control);
    color: var(--c-text-strong);
    transform: scale(0.99);
  }

  .more-row[aria-current='page'] {
    background: var(--c-control-hover);
    border-color: var(--c-border-strong);
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .more-row:focus-visible {
    outline-offset: -2px;
  }

  .more-version {
    padding: 0 var(--space-1);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  @media (prefers-reduced-motion: reduce) {
    .more-row {
      transition-duration: 1ms;
    }
  }
</style>
