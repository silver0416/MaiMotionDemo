<script lang="ts">
  import { goto } from '$app/navigation';
  import Icon from '$app/components/Icon.svelte';
  import Sheet from '$lib/shell/Sheet.svelte';
  import { createSearch, ensureIndex, type IndexStatus, type SearchHit, type SearchState } from './engine';

  /**
   * 搜尋手冊：桌面是靠上置中的對話框，手機是全螢幕。
   * 上下鍵移動、Enter 前往、Esc 關閉；輸入法組字中不觸發搜尋。
   * 結果來自 engine.ts（目前是字面比對，之後會合併語意結果，這裡不用改）。
   */
  interface Props {
    open: boolean;
    /** 打開時預填的字。 */
    seed?: string;
  }

  let { open = $bindable(), seed = '' }: Props = $props();

  const EXAMPLES = ['兩隻手為什麼會互換', '影片和譜面對不起來', '怎麼只看某個資料夾', 'Slide 要怎麼寫'];

  let input = $state<HTMLInputElement | null>(null);
  let query = $state('');
  let hits = $state<SearchHit[]>([]);
  let searchState = $state<SearchState>('idle');
  let indexState = $state<IndexStatus>('loading');
  let activeIndex = $state(0);
  let composing = false;

  const search = createSearch((next, state) => {
    hits = next;
    searchState = state;
    activeIndex = 0;
  });

  // 打開時：帶入預填字、讀索引。
  $effect(() => {
    if (!open) return;
    query = seed;
    void ensureIndex().then((status) => {
      indexState = status;
      if (query) search.query(query);
    });
  });

  function run() {
    if (composing) return;
    search.query(query);
  }

  function onClose() {
    search.cancel();
    query = '';
    hits = [];
    searchState = 'idle';
  }

  function pick(example: string) {
    query = example;
    search.query(query);
    input?.focus();
  }

  async function go(hit: SearchHit) {
    open = false;
    await goto(hit.url);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.isComposing || composing) return;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      // 結果還沒出來時也攔下，免得後面的頁面跟著捲動。
      event.preventDefault();
      if (hits.length === 0) return;
      const step = event.key === 'ArrowDown' ? 1 : -1;
      activeIndex = (activeIndex + step + hits.length) % hits.length;
      document.getElementById(`search-hit-${activeIndex}`)?.scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Enter') {
      const hit = hits[activeIndex];
      if (hit) {
        event.preventDefault();
        void go(hit);
      }
    }
  }

  /** 把摘要切成一般字與標示字。 */
  function segments(hit: SearchHit): { text: string; mark: boolean }[] {
    const parts: { text: string; mark: boolean }[] = [];
    let at = 0;
    for (const [start, end] of hit.ranges) {
      if (start > at) parts.push({ text: hit.snippet.slice(at, start), mark: false });
      parts.push({ text: hit.snippet.slice(start, end), mark: true });
      at = end;
    }
    if (at < hit.snippet.length) parts.push({ text: hit.snippet.slice(at), mark: false });
    return parts;
  }

  const trimmed = $derived(query.trim());
</script>

<Sheet bind:open title="搜尋手冊" hideTitle size="lg" placement="top" mobile="full" {onClose}>
  {#snippet head()}
    <label class="search-field">
      <span class="search-icon"><Icon name="search" size={18} /></span>
      <span class="sr-only">搜尋手冊</span>
      <input
        bind:this={input}
        bind:value={query}
        class="input search-input"
        type="search"
        placeholder="想知道什麼？直接打問題或關鍵字"
        autocomplete="off"
        spellcheck="false"
        enterkeyhint="go"
        role="combobox"
        aria-expanded={hits.length > 0}
        aria-controls="search-results"
        aria-autocomplete="list"
        aria-activedescendant={hits.length > 0 ? `search-hit-${activeIndex}` : undefined}
        data-autofocus
        oninput={run}
        oncompositionstart={() => (composing = true)}
        oncompositionend={() => {
          composing = false;
          run();
        }}
        onkeydown={onKeydown}
      />
    </label>
  {/snippet}

  <div class="search-body" aria-live="polite">
    {#if indexState === 'missing'}
      <div class="alert alert--warn" role="status">
        <p class="alert-title">索引還沒建立</p>
        <p>暫時沒辦法搜尋。可以先從目錄找，或稍後再試。</p>
      </div>
    {:else if !trimmed}
      <p class="search-hint">可以這樣問：</p>
      <ul class="examples">
        {#each EXAMPLES as example (example)}
          <li>
            <button type="button" class="example" onclick={() => pick(example)}>
              <Icon name="search" />
              <span>{example}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else if indexState === 'loading' || (searchState === 'idle' && trimmed)}
      <div class="search-loading" role="status">
        <span class="skeleton skeleton--line" style="width: 40%"></span>
        <span class="skeleton skeleton--line" style="width: 85%"></span>
        <span class="skeleton skeleton--line" style="width: 70%"></span>
        <span class="sr-only">搜尋中</span>
      </div>
    {:else if hits.length === 0}
      <div class="search-empty" role="status">
        <p class="search-empty-title">找不到「{trimmed}」相關的段落</p>
        <p class="muted">換個說法試試：用畫面上按鈕的名字，或少打幾個字。</p>
      </div>
    {:else}
      <ul class="hits" id="search-results" role="listbox" aria-label="搜尋結果">
        {#each hits as hit, i (hit.id)}
          <li id={`search-hit-${i}`} role="option" aria-selected={i === activeIndex}>
            <a
              class={['hit', i === activeIndex && 'is-active']}
              href={hit.url}
              tabindex="-1"
              onclick={(event) => {
                event.preventDefault();
                void go(hit);
              }}
              onpointermove={() => (activeIndex = i)}
            >
              <span class="hit-path">
                <span class="hit-page">{hit.page}</span>
                {#each hit.headings as heading (heading)}
                  <span class="hit-sep" aria-hidden="true">›</span>
                  <span class="hit-heading">{heading}</span>
                {/each}
              </span>
              {#if hit.snippet}
                <span class="hit-snippet">
                  {#each segments(hit) as part, p (p)}
                    {#if part.mark}<mark>{part.text}</mark>{:else}{part.text}{/if}
                  {/each}
                </span>
              {/if}
            </a>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#snippet foot()}
    <span class="keys">
      <span><kbd class="kbd">上</kbd><kbd class="kbd">下</kbd> 移動</span>
      <span><kbd class="kbd">Enter</kbd> 前往</span>
      <span><kbd class="kbd">Esc</kbd> 關閉</span>
    </span>
  {/snippet}
</Sheet>

<style>
  .search-field {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    min-width: 0;
  }

  .search-icon {
    position: absolute;
    left: 14px;
    display: inline-flex;
    color: var(--c-text-dim);
    pointer-events: none;
  }

  .search-input {
    min-height: 40px;
    padding-left: 42px;
    font-size: var(--fs-lg);
  }

  /* 原生 type=search 的清除鈕在各瀏覽器長得不一樣，關掉。 */
  .search-input::-webkit-search-cancel-button {
    appearance: none;
  }

  .search-body {
    padding: var(--space-1);
  }

  .search-hint {
    padding: var(--space-2) var(--space-3);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
    font-weight: 600;
    letter-spacing: 0.04em;
  }

  .examples,
  .hits {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .example {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    min-height: 40px;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    background: transparent;
    color: var(--c-text);
    font-size: var(--fs-lg);
    text-align: left;
    cursor: pointer;
    transition:
      background-color 120ms ease,
      transform 90ms ease-out;
  }

  .example :global(.icon) {
    color: var(--c-text-dim);
  }

  @media (hover: hover) {
    .example:hover {
      background: var(--c-control);
    }
  }

  .example:active {
    transform: scale(0.99);
  }

  .example:focus-visible {
    outline-offset: -2px;
  }

  .hit {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: 10px 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-item);
    color: var(--c-text);
    text-decoration: none;
  }

  .hit.is-active {
    background: var(--c-control-hover);
    border-color: var(--c-border-strong);
  }

  .hit-path {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px var(--space-2);
    color: var(--c-text-strong);
    font-size: var(--fs-lg);
    font-weight: 600;
    line-height: var(--lh-tight);
  }

  .hit-sep,
  .hit-page {
    color: var(--c-text-dim);
    font-weight: 500;
  }

  .hit-page:only-child {
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .hit-snippet {
    color: var(--c-text-dim);
    font-size: var(--fs-md);
    line-height: var(--lh-normal);
  }

  .hit-snippet mark {
    padding: 0 1px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--c-accent) 26%, var(--c-panel));
    color: var(--c-text-strong);
  }

  .search-loading {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-3);
  }

  .search-empty {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-5) var(--space-3);
    font-size: var(--fs-lg);
  }

  .search-empty-title {
    color: var(--c-text-strong);
    font-weight: 600;
  }

  .alert {
    margin: var(--space-2);
    font-size: var(--fs-md);
  }

  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    color: var(--c-text-dim);
    font-size: var(--fs-sm);
  }

  .keys > span {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .keys :global(.kbd) {
    min-height: 20px;
    font-size: var(--fs-xs);
  }

  @media (pointer: coarse) {
    .search-input {
      min-height: 44px;
    }

    .example {
      min-height: 48px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .example {
      transition-duration: 1ms;
    }
  }
</style>
