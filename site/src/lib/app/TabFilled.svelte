<script lang="ts">
  /**
   * 底部導覽列「選中」時用的實心圖示（Android 的 Material 3 慣例：選中換成填滿的版本）。
   * 外形沿用 Lucide 的線條圖示（同一套路徑），內部填上和外框一樣的顏色；該留空的地方用遮罩挖掉：
   * - house：門挖空，一路開到底（像 Material 的實心首頁圖示）。
   * - book-open：中間的書脊挖成一條縫，分出左右兩頁。
   * 放大鏡的鏡片本來就該是空的、「更多」的三個點本來就是實心，不需要實心版本。
   * 預設不顯示，由 TabBar 的樣式在選中時切換（和線條版同時存在，第一次繪製就決定，不閃）。
   */
  interface Props {
    name: 'house' | 'book-open';
    size?: number;
  }

  let { name, size = 24 }: Props = $props();
</script>

<svg
  class="icon tab-filled"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="currentColor"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#if name === 'house'}
    <mask id="tab-filled-house" maskUnits="userSpaceOnUse" x="0" y="0" width="24" height="24">
      <rect width="24" height="24" fill="#fff" stroke="none" />
      <path d="M9.5 24v-9.5a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1V24z" fill="#000" stroke="none" />
    </mask>
    <path
      mask="url(#tab-filled-house)"
      d="M3 10a2 2 0 0 1 .709-1.528l7-5.999a2 2 0 0 1 2.582 0l7 5.999A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"
    />
  {:else}
    <mask id="tab-filled-book" maskUnits="userSpaceOnUse" x="0" y="0" width="24" height="24">
      <rect width="24" height="24" fill="#fff" stroke="none" />
      <path d="M12 6.5V24" fill="none" stroke="#000" stroke-width="1.6" stroke-linecap="butt" />
    </mask>
    <path
      mask="url(#tab-filled-book)"
      d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z"
    />
  {/if}
</svg>
