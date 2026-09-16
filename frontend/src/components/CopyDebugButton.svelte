<script lang="ts">
  import Icon from './Icon.svelte';
  import { appVersion } from '../lib/api';
  import { buildDebugReport, copyText, type DebugInput } from '../lib/debug';
  import { toasts } from '../state/toasts.svelte';

  interface Props {
    /** 按下時才組資料，確保複製到的是當下的原文與診斷。 */
    input: () => Omit<DebugInput, 'appVersion'>;
    label?: string;
  }

  let { input, label = '複製除錯資訊' }: Props = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy() {
    const report = buildDebugReport({ ...input(), appVersion: await appVersion() });
    const ok = await copyText(report);
    if (!ok) {
      toasts.show({
        id: 'debug-copy',
        tone: 'error',
        title: '無法寫入剪貼簿',
        body: '系統拒絕了剪貼簿存取，請再試一次。',
      });
      return;
    }
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), 2000);
  }
</script>

<button class="btn" onclick={copy} title="包含完整譜面、錯誤位置與問題點，可直接貼給開發者">
  <Icon name={copied ? 'check' : 'copy'} />
  <span aria-live="polite">{copied ? '已複製' : label}</span>
</button>
