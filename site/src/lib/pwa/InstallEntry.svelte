<script lang="ts">
  /**
   * 安裝入口：一顆 .btn，放在手機首頁（原本「下載」的位置，文字「安裝手冊到手機」）與選單抽屜（「加到主畫面」）。
   * 點了之後：Android 有安裝事件就直接叫出系統的安裝視窗，沒有就打開「從瀏覽器選單安裝」的步驟；
   * iOS 打開步驟；App 內建瀏覽器打開「請用瀏覽器開啟」。
   * 不可安裝時（電腦、已安裝成 App）自己不顯示。
   *
   * 用法：<InstallEntry onselect={() => (ui.menuOpen = false)} />
   * `onselect` 在流程開始前呼叫，用來先關掉抽屜（提示框在抽屜開著時會先藏起來）。
   */
  import { openInstall } from './install';
  import PwaIcon from './PwaIcon.svelte';
  import { installKind } from './state.svelte';

  interface Props {
    /** 點了之後、開始安裝流程前呼叫（例如關掉選單抽屜）。 */
    onselect?: () => void;
    /** 額外的 class（例如讓它和旁邊的按鈕等寬）。 */
    class?: string;
    /** 按鈕上的字。 */
    label?: string;
  }

  let { onselect, class: extra = '', label = '加到主畫面' }: Props = $props();

  const kind = $derived(installKind());

  function select() {
    onselect?.();
    openInstall();
  }
</script>

{#if kind !== 'none'}
  <button class="btn {extra}" onclick={select}>
    <PwaIcon name="smartphone" />{label}
  </button>
{/if}
