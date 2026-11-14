<script lang="ts">
  import { page } from '$app/state';
  import Icon from '$app/components/Icon.svelte';
  import { normalizePath, SITE } from '$lib/nav';
  import ExtraIcon from './ExtraIcon.svelte';
  import NavTree from './NavTree.svelte';
  import Sheet from './Sheet.svelte';
  import ThemeSwitch from './ThemeSwitch.svelte';
  import { ui } from './ui.svelte';

  /**
   * 手機寬度（< 768px）頂列「選單」按鈕打開的抽屜：從底部滑入，標題「目錄」。
   * 最上面一排是不跟著捲動的工具（外觀、下載 App、GitHub），下面是章節目錄（開啟時捲到並聚焦目前這一頁）。
   */
  const atDownload = $derived(normalizePath(page.url.pathname) === SITE.download);

  function close() {
    ui.menuOpen = false;
  }
</script>

<Sheet bind:open={ui.menuOpen} title="目錄">
  {#snippet top()}
    <div class="menu-tools">
      <ThemeSwitch />
      <div class="menu-links">
        <a class="btn" href={SITE.download} aria-current={atDownload ? 'page' : undefined} onclick={close}>
          <Icon name="download" />下載 App
        </a>
        <a class="btn" href={SITE.github} target="_blank" rel="noopener">
          <ExtraIcon name="github" />GitHub<span class="sr-only">（在新分頁開啟）</span>
        </a>
      </div>
    </div>
  {/snippet}

  <div class="menu-nav">
    <NavTree focusCurrent onNavigate={close} />
  </div>
</Sheet>

<style>
  .menu-tools {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .menu-tools :global(.theme-switch) {
    width: 100%;
  }

  .menu-links {
    display: flex;
    gap: var(--space-2);
  }

  .menu-links .btn {
    flex: 1 1 0;
    min-width: 0;
    font-size: var(--fs-md);
  }

  .menu-nav {
    padding: var(--space-1) 0 var(--space-3);
  }
</style>
