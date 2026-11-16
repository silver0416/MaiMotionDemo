<script lang="ts">
  /**
   * App 的畫面截圖：頁面上是一張縮圖，點一下在整個視窗放大看，再點一下、按「關閉」或 Esc 收起來。
   * - 放大用原生 <dialog> 的 showModal（焦點、Esc、背景不可操作都交給瀏覽器）；整面是不透明的底色，不用半透明遮罩。
   * - 寬螢幕：整張圖縮到視窗內。窄螢幕（< 768px）：圖片撐到視窗高，左右拖著看，字才看得清楚。
   * - width／height 是圖片的原始尺寸，先佔好位置，載入時版面不跳。
   * 圖片放在 static/shots/。用法：<Shot src="/shots/….webp" alt="…" width={2000} height={1078} caption="…" />
   */
  import Icon from '$app/components/Icon.svelte';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';

  interface Props {
    src: string;
    alt: string;
    width: number;
    height: number;
    caption?: string;
  }

  let { src, alt, width, height, caption }: Props = $props();

  let dialog: HTMLDialogElement;

  function open() {
    dialog.showModal();
  }

  function close() {
    dialog.close();
  }
</script>

<figure class="shot">
  <button class="shot-open" type="button" onclick={open} aria-label={`放大圖片：${alt}`}>
    <img {src} {alt} {width} {height} loading="lazy" decoding="async" />
    <span class="shot-hint" aria-hidden="true"><ExtraIcon name="maximize" size={14} />點圖放大</span>
  </button>
  {#if caption}<figcaption class="shot-caption">{caption}</figcaption>{/if}
</figure>

<dialog class="shot-dialog" bind:this={dialog} aria-label={alt}>
  <div class="shot-bar">
    <button class="btn" type="button" onclick={close}><Icon name="x" size={16} />關閉</button>
  </div>
  <button class="shot-stage scroll" type="button" onclick={close} aria-label="關閉放大的圖片">
    <img {src} alt="" {width} {height} />
  </button>
</dialog>

<style>
  .shot {
    margin: var(--space-5) 0 0;
  }

  /* 縮圖本身是一顆按鈕：外框與圓角同面板；hover／focus 時框線變明顯。 */
  .shot-open {
    position: relative;
    display: block;
    width: 100%;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-item);
    background: var(--c-panel);
    cursor: zoom-in;
  }

  .shot-open:hover,
  .shot-open:focus-visible {
    border-color: var(--c-border-strong);
  }

  .shot-open img {
    display: block;
    width: 100%;
    height: auto;
  }

  /* 右下角的提示：不透明的膠囊。 */
  .shot-hint {
    position: absolute;
    right: var(--space-3);
    bottom: var(--space-3);
    display: inline-flex;
    gap: var(--space-1);
    align-items: center;
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-pill);
    background: var(--c-control);
    color: var(--c-text-strong);
    font-size: var(--fs-sm);
    font-weight: 600;
    line-height: var(--lh-normal);
  }

  .shot-caption {
    margin: var(--space-3) auto 0;
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
    text-align: center;
  }

  /* 放大：蓋滿整個視窗的不透明底色，上面一列「關閉」，下面是圖。 */
  .shot-dialog {
    width: 100vw;
    max-width: none;
    height: 100dvh;
    max-height: none;
    margin: 0;
    padding: 0;
    border: none;
    background: var(--c-bg);
    color: var(--c-text);
  }

  .shot-dialog[open] {
    display: flex;
    flex-direction: column;
  }

  .shot-dialog::backdrop {
    background: var(--c-bg);
  }

  .shot-bar {
    display: flex;
    flex: none;
    justify-content: flex-end;
    padding: calc(var(--space-3) + env(safe-area-inset-top, 0px)) var(--space-4) var(--space-3);
  }

  .shot-stage {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    justify-content: center;
    min-height: 0;
    padding: 0 var(--space-4) calc(var(--space-4) + env(safe-area-inset-bottom, 0px));
    border: none;
    background: none;
    cursor: zoom-out;
  }

  .shot-stage img {
    display: block;
    max-width: 100%;
    max-height: 100%;
    width: auto;
    height: auto;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-item);
  }

  @media (width < 768px) {
    /* 窄螢幕：圖片撐到可用高度、寬度超出就左右捲，從最左邊開始看。 */
    .shot-stage {
      justify-content: flex-start;
      overflow: auto hidden;
    }

    .shot-stage img {
      flex: none;
      max-width: none;
      height: 100%;
    }
  }
</style>
