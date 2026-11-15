<script lang="ts">
  import NavTree from '$lib/shell/NavTree.svelte';
  import Sheet from '$lib/shell/Sheet.svelte';
  import { ui } from '$lib/shell/ui.svelte';
  import { PANEL_IDS } from './panel-ids';

  /**
   * App 外框底部導覽列「目錄」打開的抽屜：章節目錄（和一般外框的側欄、選單抽屜同一個 NavTree）。
   * 開啟時捲到並聚焦目前這一頁；點了連結就關。
   */
  function close() {
    ui.tocOpen = false;
  }
</script>

<Sheet bind:open={ui.tocOpen} title="目錄" id={PANEL_IDS.toc}>
  <div class="toc-nav">
    <NavTree focusCurrent onNavigate={close} />
  </div>
</Sheet>

<style>
  .toc-nav {
    padding: var(--space-1) 0 var(--space-3);
  }
</style>
