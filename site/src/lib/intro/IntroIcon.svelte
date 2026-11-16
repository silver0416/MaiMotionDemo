<script lang="ts" module>
  import type { IconName } from '$app/components/Icon.svelte';
  import type { ExtraIconName } from '$lib/shell/ExtraIcon.svelte';

  /** 導覽章節可用的圖示：App 的 Icon 加上文件站補充的 ExtraIcon。 */
  export type IntroIconName = IconName | ExtraIconName;
</script>

<script lang="ts">
  /**
   * 「認識 MaiMotionDemo」各元件共用的圖示：名字在 ExtraIcon 裡就用它，否則用 App 的 Icon。
   * 兩者同一套畫法（Lucide、24×24、線寬 2、跟隨文字顏色、aria-hidden），只是來源不同。
   */
  import Icon from '$app/components/Icon.svelte';
  import ExtraIcon, { isExtraIcon } from '$lib/shell/ExtraIcon.svelte';

  interface Props {
    name: IntroIconName;
    size?: number;
  }

  let { name, size = 20 }: Props = $props();
</script>

{#if isExtraIcon(name)}
  <ExtraIcon {name} {size} />
{:else}
  <Icon name={name as IconName} {size} />
{/if}
