<script lang="ts">
  /**
   * <Features> 裡的一格（<li>）：一塊面板，上面一顆圓形圖示（裝飾，aria-hidden），下面是內文（children）。
   * 單層容器，裡面不要再放卡片。
   */
  import type { Snippet } from 'svelte';
  import IntroIcon, { type IntroIconName } from './IntroIcon.svelte';

  interface Props {
    icon: IntroIconName;
    children?: Snippet;
  }

  let { icon, children }: Props = $props();
</script>

<li class="feature">
  <span class="feature-tile" aria-hidden="true"><IntroIcon name={icon} size={20} /></span>
  <div class="feature-body">
    {@render children?.()}
  </div>
</li>

<style>
  .feature {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
    padding: var(--space-4);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-item);
    background: var(--c-panel);
  }

  .feature-tile {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-pill);
    background: var(--c-control-hover);
    color: var(--c-text-strong);
  }

  .feature-body {
    min-width: 0;
  }

  .feature-body > :global(p) {
    margin: 0;
  }

  .feature-body > :global(p + p) {
    margin-top: 0.5em;
  }

  /* 只排得下一欄時（手機）：圖示改放左邊，不要為了一兩行字多佔一列高度。 */
  @container (width < 28rem) {
    .feature {
      flex-direction: row;
      align-items: flex-start;
    }

    .feature-body {
      flex: 1;
      padding-top: var(--space-2);
    }
  }
</style>
