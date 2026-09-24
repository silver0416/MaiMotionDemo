<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import type { VideoLibrary } from '../state/videoLibrary.svelte';

  interface Props {
    library: VideoLibrary;
    id: string;
  }

  let { library, id }: Props = $props();

  const message = $derived(library.downloadErrors[id]);
  /** 沒有 JS 執行環境時，YouTube 下載失敗多半是這個原因。 */
  const noRuntime = $derived(!!library.status && !library.status.runtime);
</script>

{#if message}
  <div class="alert alert--error stack-sm" role="alert">
    {#if noRuntime}
      <p>下載失敗：這台電腦沒有 JS 執行環境（Deno 或 Node.js），YouTube 需要它才能下載。下載 Deno 後再試一次。</p>
      <div class="row row-wrap">
        <button class="btn" onclick={() => void library.install('deno')} disabled={!!library.installing.deno}>
          <Icon name={library.installing.deno ? 'loader' : 'download'} spin={!!library.installing.deno} size={14} />
          {library.installing.deno ? '正在下載 Deno' : '下載 Deno（約 45 MB）'}
        </button>
      </div>
      <p class="xsmall muted">yt-dlp 訊息：{message}</p>
    {:else}
      <p>{message}</p>
    {/if}
  </div>
{/if}
