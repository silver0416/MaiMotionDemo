<script lang="ts">
  /**
   * 浮在畫面下方的小提示框（安裝提示、「有新版本」共用）。外觀同 App 的通知（Toaster）：
   * 不透明純色、圓角 18、--shadow-pop；不蓋住整個畫面，只佔最下面一小塊。
   *
   * 位置避開底部導覽列（App 外框）：iOS 的浮動膠囊高 62px、離底 21px；Android 的底列高 64px 加手勢列。
   * 外框若要自己決定，在 :root 設 `--app-nav-clearance`（提示框底緣到螢幕底的距離）即可覆蓋。
   * 出現時由下往上 12px 淡入（220ms），收起 140ms；減少動態時直接出現、消失。
   */
  import type { Snippet } from 'svelte';
  import { cubicIn, cubicOut } from 'svelte/easing';

  interface Props {
    /** 給輔助技術的名稱。 */
    label: string;
    /** 低調版（App 內建瀏覽器的引導）：底色和面板一樣、框線較淡。 */
    quiet?: boolean;
    children: Snippet;
  }

  let { label, quiet = false, children }: Props = $props();

  function rise(_node: Element, { out = false }: { out?: boolean } = {}) {
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    return {
      duration: reduce ? 0 : out ? 140 : 220,
      easing: out ? cubicIn : cubicOut,
      css: (t: number) => `opacity: ${t}; transform: translateY(${(1 - t) * 12}px)`,
    };
  }
</script>

<section class="notice" class:notice--quiet={quiet} aria-label={label} in:rise out:rise={{ out: true }}>
  {@render children()}
</section>

<style>
  .notice {
    --clearance: calc(env(safe-area-inset-bottom, 0px) + var(--space-3));
    position: fixed;
    left: var(--gutter);
    right: var(--gutter);
    bottom: var(--app-nav-clearance, var(--clearance));
    z-index: 20;
    max-width: 440px;
    margin-inline: auto;
    padding: 10px 8px 10px 14px;
    background: var(--c-control-hover);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-pop);
    box-shadow: var(--shadow-pop);
    color: var(--c-text-strong);
  }

  .notice--quiet {
    background: var(--c-panel);
    border-color: var(--c-border);
    color: var(--c-text);
  }

  :global(:root[data-app='ios']) .notice {
    --clearance: calc(21px + 62px + var(--space-3));
  }

  :global(:root[data-app='android']) .notice {
    --clearance: calc(64px + env(safe-area-inset-bottom, 0px) + var(--space-3));
  }
</style>
