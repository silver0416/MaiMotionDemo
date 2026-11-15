<script lang="ts" module>
  /**
   * 安裝引導用的線條圖示：App 的 Icon.svelte 與 $lib/shell/ExtraIcon.svelte 都沒有的才放這裡。
   * 外形取自 Lucide（ISC 授權）、24×24、線寬 2、跟隨文字顏色，和其他圖示同一套。
   * 之後若要移進 ExtraIcon.svelte，名稱與路徑照搬即可。
   */
  const ICONS = {
    /** iOS 的「分享」：方框加向上的箭頭（Lucide share）。 */
    share: ['M12 2v13', 'm16 6-4-4-4 4', 'M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8'],
    /** 「加入主畫面」：方框裡一個加號（Lucide square-plus）。 */
    'square-plus': ['M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z', 'M8 12h8', 'M12 8v8'],
    /** 「以網頁 App 打開」的開關（Lucide toggle-right）。 */
    'toggle-right': ['M9 5h6a7 7 0 0 1 0 14H9A7 7 0 0 1 9 5z', 'M12 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0'],
    /** 安裝到主畫面（Lucide smartphone）。 */
    smartphone: ['M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z', 'M12 18h.01'],
  } as const;

  export type PwaIconName = keyof typeof ICONS;
</script>

<script lang="ts">
  interface Props {
    name: PwaIconName;
    size?: number;
  }

  let { name, size = 15 }: Props = $props();
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#each ICONS[name] as d (d)}
    <path {d} />
  {/each}
</svg>

<style>
  .icon {
    flex: none;
    display: block;
  }
</style>
