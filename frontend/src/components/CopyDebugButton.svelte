<script lang="ts">
  import Icon from './Icon.svelte';
  import { appVersion } from '../lib/api';
  import { buildDebugReport, copyText, type DebugInput } from '../lib/debug';
  import { toasts } from '../state/toasts.svelte';

  interface Props {
    /** 按下時才組資料，確保複製到的是當下的原文與診斷。 */
    input?: () => Omit<DebugInput, 'appVersion'>;
    /** 另外組好的整段文字（例如比對回報），取代 input；回傳 null 表示目前沒有可複製的內容。 */
    report?: () => Promise<string | null>;
    label?: string;
    title?: string;
    iconSize?: number;
  }

  let {
    input,
    report,
    label = '複製除錯資訊',
    title = '包含完整譜面、錯誤位置與問題點，可直接貼給開發者',
    iconSize,
  }: Props = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function build(): Promise<string | null> {
    if (report) return report();
    if (input) return buildDebugReport({ ...input(), appVersion: await appVersion() });
    return null;
  }

  async function copy() {
    const text = await build();
    if (text === null) {
      toasts.show({ id: 'debug-copy', tone: 'error', title: '沒有可以複製的內容', body: '譜面還沒載入完成，請稍後再試。' });
      return;
    }
    const ok = await copyText(text);
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

<button class="btn" onclick={copy} {title}>
  <Icon name={copied ? 'check' : 'copy'} size={iconSize} />
  <span aria-live="polite">{copied ? '已複製' : label}</span>
</button>
