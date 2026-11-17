<script lang="ts">
  /**
   * 示意畫面：下載好的 App 檔案（作業系統的畫面拍不到，畫一個最簡單的樣子）。
   * 只有資料夾名稱、App 圖示與檔名；版面在 ../sketch.ts（1280×800 空間），這裡換成百分比，字級用 cqw 跟著舞台寬度縮放。
   * 「示意」的標示是 QuickTour 圖說的徽章（跟著畫面切換），不寫在這裡。
   */
  import Icon from '$app/components/Icon.svelte';
  import Logo from '$lib/shell/Logo.svelte';
  import { SITE } from '$lib/nav';
  import type { Rect } from '../flows';
  import { EXE_SKETCH, SKETCH_VIEWPORT } from '../sketch';

  const { width: W, height: H } = SKETCH_VIEWPORT;
  const fileName = `MaiMotionDemo-v${SITE.appVersion}-windows-x64.exe`;

  function box([x, y, w, h]: Rect) {
    return `left:${(x / W) * 100}%;top:${(y / H) * 100}%;width:${(w / W) * 100}%;height:${(h / H) * 100}%`;
  }
</script>

<div class="sketch" role="img" aria-label={`示意：「下載」資料夾裡的 ${fileName}`}>
  <span class="folder" style={box(EXE_SKETCH.folder)}><Icon name="folder" size={18} />下載</span>
  <span class="icon" style={box(EXE_SKETCH.icon)}><Logo size={64} /></span>
  <span class="name mono" style={box(EXE_SKETCH.name)}>{fileName}</span>
</div>

<style>
  /* 1cqw＝舞台寬度的 1%＝1280 空間的 12.8px。 */
  .sketch {
    container-type: inline-size;
    position: absolute;
    inset: 0;
    background: var(--c-bg);
    color: var(--c-text);
  }

  .sketch > span {
    position: absolute;
    display: flex;
  }

  .folder {
    align-items: center;
    gap: 0.6cqw;
    color: var(--c-text-dim);
    font-size: 1.4cqw;
  }

  .folder :global(svg) {
    width: 1.6cqw;
    height: 1.6cqw;
  }

  .icon {
    align-items: center;
    justify-content: center;
    border: 1px solid var(--c-border);
    border-radius: 2cqw;
    background: var(--c-panel);
    color: var(--c-text-strong);
  }

  .icon :global(svg) {
    width: 62%;
    height: 62%;
  }

  .name {
    justify-content: center;
    font-size: 1.25cqw;
    line-height: 1.5;
    text-align: center;
    overflow-wrap: anywhere;
  }
</style>
