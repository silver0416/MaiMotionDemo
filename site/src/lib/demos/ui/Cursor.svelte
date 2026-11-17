<!-- css="injected"：理由同 Stage.svelte（實際畫面導覽在伺服器端就算繪游標，沒有樣式時會露出來）。 -->
<svelte:options css="injected" />

<script lang="ts">
  /**
   * 示意動畫的小游標：自動示範時表示「現在按了哪裡」。
   * 位置是相對於演示內容區的像素，移動 600ms（和 player.svelte.ts 的 TRAVEL 一致）；
   * press 每加一，游標尖端放出一圈琥珀色的「按下」回饋（只播一次，不循環）。
   * 外形取自 Lucide mouse-pointer-2（ISC 授權）。
   */
  interface Props {
    x: number;
    y: number;
    show: boolean;
    press: number;
  }

  let { x, y, show, press }: Props = $props();
</script>

<div class="cursor" class:is-shown={show} style:transform={`translate(${x}px, ${y}px)`} aria-hidden="true">
  {#key press}
    <span class="ring" class:is-on={press > 0}></span>
  {/key}
  <svg width="22" height="22" viewBox="0 0 24 24">
    <path
      d="M4.037 4.688a.495.495 0 0 1 .651-.651l16 6.5a.5.5 0 0 1-.063.947l-6.124 1.58a2 2 0 0 0-1.438 1.435l-1.579 6.126a.5.5 0 0 1-.947.063z"
    />
  </svg>
</div>

<style>
  .cursor {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 5;
    width: 0;
    height: 0;
    pointer-events: none;
    opacity: 0;
    transition:
      transform 600ms var(--ease-out),
      opacity 160ms ease;
  }

  .cursor.is-shown {
    opacity: 1;
  }

  /* 尖端（4, 4.7）對準座標點。 */
  svg {
    position: absolute;
    top: -4px;
    left: -4px;
    display: block;
    fill: var(--c-text-strong);
    stroke: var(--c-bg);
    stroke-width: 1.5;
    stroke-linejoin: round;
  }

  .ring {
    position: absolute;
    top: -14px;
    left: -14px;
    width: 28px;
    height: 28px;
    border: 2px solid var(--c-accent);
    border-radius: 50%;
    opacity: 0;
  }

  .ring.is-on {
    animation: cursor-press 420ms var(--ease-out);
  }

  @keyframes cursor-press {
    from {
      opacity: 1;
      transform: scale(0.4);
    }
    to {
      opacity: 0;
      transform: scale(1.3);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .cursor {
      transition: none;
    }

    .ring.is-on {
      animation: none;
    }
  }
</style>
