<script lang="ts">
  /**
   * 站名旁的標誌：App icon 的前景（一個環裡粉、藍兩個圓點），不帶底色。
   * 環跟著文字顏色（深色是白、淺色是黑），圓點用左右手的顏色。
   * 比例取自 App icon（以 100 為邊長：環半徑 32、環寬 9、圓點半徑 8.5、圓點離中心 12）。
   */
  interface Props {
    size?: number;
  }

  let { size = 24 }: Props = $props();
</script>

<svg class="logo" width={size} height={size} viewBox="0 0 100 100" aria-hidden="true">
  <circle class="logo-ring" cx="50" cy="50" r="38" />
  <circle class="logo-left" cx="36" cy="50" r="10" />
  <circle class="logo-right" cx="64" cy="50" r="10" />
</svg>

<style>
  .logo {
    flex: none;
    display: block;
  }

  .logo-ring {
    fill: none;
    stroke: currentColor;
    stroke-width: 10.5;
  }

  .logo-left {
    fill: var(--c-left);
  }

  .logo-right {
    fill: var(--c-right);
  }
</style>
