<script lang="ts">
  import type { Snippet } from 'svelte';
  import { fade } from 'svelte/transition';
  import Icon, { type IconName } from '$app/components/Icon.svelte';
  import { reducedMotion } from '$app/lib/press';
  import ExtraIcon from '$lib/shell/ExtraIcon.svelte';
  import MiniDisc from './MiniDisc.svelte';
  import Stage from './Stage.svelte';

  /**
   * 示意：App 主視窗的縮圖（互動地圖，不自動播放）。
   * 點縮圖裡的一區（或用方向鍵換區），該區加亮，旁邊寫名稱、一句說明與「看詳細」連結；
   * 「窄視窗」開關示範 900px 以下改成上下排列。文字對照 start/interface.md 與 App.svelte 的分頁名稱。
   */

  type RegionId = 'records' | 'disc' | 'transport' | 'solution' | 'note' | 'config' | 'view' | 'annotate';

  interface Region {
    id: RegionId;
    where: string;
    name: string;
    desc: string;
    href: string;
    icon?: IconName;
  }

  const REGIONS: Region[] = [
    {
      id: 'records',
      where: '左欄',
      name: '譜面紀錄',
      desc: '上方有「收合側欄」「新增」「搜尋譜面」，中間是資料夾、標籤、篩選與紀錄清單，最下方是「設定」。點一筆紀錄就開啟那份譜面。',
      href: '/charts/records',
    },
    {
      id: 'disc',
      where: '中間',
      name: '盤面',
      desc: '兩隻手怎麼移動都畫在這裡。在盤面點一顆音符，右側會自動切到「音符」分頁。',
      href: '/start/disc',
    },
    {
      id: 'transport',
      where: '中間下方',
      name: '播放列',
      desc: '播放、倍率、時間軸、循環片段與時間軸標籤。',
      href: '/play/transport',
    },
    {
      id: 'solution',
      where: '右側分頁',
      name: '方案',
      desc: '比較候選方案、看兩手分工、換手與模型略過的地方。',
      href: '/play/solutions',
      icon: 'compare',
    },
    {
      id: 'note',
      where: '右側分頁',
      name: '音符',
      desc: '看選取音符的時間、分給哪隻手、滑行區段。',
      href: '/play/notes',
      icon: 'circle-dot',
    },
    {
      id: 'config',
      where: '右側分頁',
      name: '參數',
      desc: '選評分方式、調整打法偏好，改完要重新生成。',
      href: '/model/params',
      icon: 'sliders',
    },
    {
      id: 'view',
      where: '右側分頁',
      name: '顯示',
      desc: '軌跡、標籤、手掌覆蓋區、Touch 落點標記、縮放與盤面校準。',
      href: '/play/view',
      icon: 'eye',
    },
    {
      id: 'annotate',
      where: '右側分頁',
      name: '標註',
      desc: '記錄真人實際的左右手，和模型比對，或開影片對照。',
      href: '/annotate/basics',
      icon: 'hand',
    },
  ];

  const TABS = REGIONS.filter((region) => region.icon);

  let selected = $state<RegionId | null>(null);
  let narrow = $state(false);
  let root = $state<HTMLElement | null>(null);

  const current = $derived(REGIONS.find((region) => region.id === selected) ?? null);
  /** 右側分頁區塊顯示哪一頁：選了某個分頁就是它，否則是「方案」。 */
  const shownTab = $derived(TABS.find((tab) => tab.id === selected) ?? TABS[0]);
  /** 方向鍵的焦點落點：目前選的那區，沒有就是第一區。 */
  const focusable = $derived(selected ?? REGIONS[0].id);

  function pick(id: RegionId) {
    selected = id;
  }

  function move(delta: number) {
    const index = REGIONS.findIndex((region) => region.id === focusable);
    const next = REGIONS[(index + delta + REGIONS.length) % REGIONS.length];
    selected = next.id;
    requestAnimationFrame(() => root?.querySelector<HTMLElement>(`.frame [data-region="${next.id}"]`)?.focus());
  }

  function onkey(key: string): boolean {
    if (key === 'ArrowRight' || key === 'ArrowDown') {
      move(1);
      return true;
    }
    if (key === 'ArrowLeft' || key === 'ArrowUp') {
      move(-1);
      return true;
    }
    if (key === 'Escape' && selected) {
      selected = null;
      return true;
    }
    return false;
  }

  const fadeIn = () => ({ duration: reducedMotion() ? 0 : 220 });
</script>

{#snippet region(id: RegionId, extra: string, body: Snippet)}
  {@const item = REGIONS.find((entry) => entry.id === id)!}
  <button
    class={`region ${extra}`}
    class:is-selected={selected === id}
    data-region={id}
    tabindex={focusable === id ? 0 : -1}
    aria-pressed={selected === id}
    aria-label={`${item.where}：${item.name}`}
    onclick={() => pick(id)}
  >
    {@render body()}
  </button>
{/snippet}

{#snippet recordsBody()}
  <span class="pane-head" aria-hidden="true">
    <span class="mini-title">譜面紀錄</span>
    <span class="mini-icons"><Icon name="plus" size={10} /><Icon name="search" size={10} /></span>
  </span>
  <span class="bars" aria-hidden="true">
    <span class="bar"></span><span class="bar bar--short"></span><span class="bar"></span><span class="bar bar--short"></span>
  </span>
  <span class="pane-foot" aria-hidden="true"><Icon name="settings" size={10} /><span>設定</span></span>
{/snippet}

{#snippet discBody()}
  <span class="disc-wrap" aria-hidden="true">
    <MiniDisc label="盤面" numbers={false} fit />
  </span>
{/snippet}

{#snippet transportBody()}
  <span class="transport-mini" aria-hidden="true">
    <span class="mini-play"><Icon name="play" size={8} /></span>
    <span class="mini-track"><span class="mini-head"></span></span>
  </span>
  <span class="mini-title" aria-hidden="true">播放列</span>
{/snippet}

<Stage label="App 版面導覽" surface="bg" say="點縮圖裡的任一區看說明；焦點在縮圖上時，方向鍵可以換區。" {onkey}>
  <div class="tour" bind:this={root}>
    <div class="tour-top">
      <label class="check">
        <input type="checkbox" bind:checked={narrow} />
        <span>窄視窗（900px 以下）</span>
      </label>
    </div>

    <div class="tour-grid">
      <div class="window" role="group" aria-label={narrow ? 'App 主視窗縮圖：窄視窗，上下排列' : 'App 主視窗縮圖：三欄'}>
        {#key narrow}
          <div class="frame" class:is-narrow={narrow} in:fade={fadeIn()}>
            {@render region('records', 'pane records', recordsBody)}
            <div class="center">
              {@render region('disc', 'disc', discBody)}
              {@render region('transport', 'pane transport', transportBody)}
            </div>
            <div class="pane side">
              <div class="tabbar" role="group" aria-label="右側資訊分頁">
                {#each TABS as tab (tab.id)}
                  <button
                    class="tab region"
                    class:is-selected={selected === tab.id}
                    data-region={tab.id}
                    tabindex={focusable === tab.id ? 0 : -1}
                    aria-pressed={selected === tab.id}
                    aria-label={`右側分頁：${tab.name}`}
                    title={tab.name}
                    onclick={() => pick(tab.id)}
                  >
                    <Icon name={tab.icon!} size={11} />
                    <span class="tab-text">{tab.name}</span>
                  </button>
                {/each}
              </div>
              <span class="side-body" aria-hidden="true">
                <span class="mini-title">{shownTab.name}</span>
                <span class="bars">
                  <span class="bar"></span><span class="bar bar--short"></span><span class="bar"></span>
                </span>
              </span>
            </div>
          </div>
        {/key}
      </div>

      <div class="info" aria-live="polite">
        {#if current}
          <span class="xsmall muted">{current.where}</span>
          <span class="info-name">{current.name}</span>
          <p class="info-desc">{current.desc}</p>
          <a class="linkish info-link" href={current.href}>看詳細<ExtraIcon name="arrow-right" size={13} /></a>
        {:else}
          <span class="info-name">選一區看看</span>
          <p class="info-desc muted">
            {narrow
              ? '視窗不到 900px 寬時，三欄改成上下排列，整頁可以捲動，所有操作仍然可以用。'
              : '左邊是譜面紀錄，中間是盤面與播放列，右邊是五個資訊分頁。'}
          </p>
        {/if}
      </div>
    </div>

    <!-- 觸控螢幕上縮圖的分頁太小，另外列一排夠大的按鈕。 -->
    <div class="chips" role="group" aria-label="選擇區域">
      {#each REGIONS as item (item.id)}
        <button class="btn chip" aria-pressed={selected === item.id} onclick={() => pick(item.id)}>{item.name}</button>
      {/each}
    </div>
  </div>
</Stage>

<style>
  .tour {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .tour-top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .tour-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 13rem;
    gap: var(--space-4);
    align-items: start;
  }

  @container demo (width < 600px) {
    .tour-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .window {
    min-width: 0;
  }

  /* ---- 三欄縮圖：比例照 App 的預設欄寬（左 240、右 360 左右），中間吃剩下的 ---- */

  .frame {
    display: grid;
    grid-template-columns: 24% minmax(0, 1fr) 32%;
    gap: 6px;
    aspect-ratio: 16 / 10;
    padding: 6px;
    background: var(--c-bg);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-md);
  }

  .center {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: 6px;
    min-height: 0;
  }

  /* 可點的一區：清單列的樣子（透明框、hover 出底），選中時一圈琥珀框。 */
  .region {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    min-height: 0;
    padding: 6px;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-panel);
    color: var(--c-text);
    font-size: var(--fs-xs);
    text-align: left;
    cursor: pointer;
    transition:
      background-color 120ms ease,
      box-shadow 120ms ease,
      transform 90ms ease-out;
  }

  .region:active {
    transform: scale(0.98);
  }

  .region.is-selected {
    box-shadow: inset 0 0 0 2px var(--c-accent);
  }

  .region:focus-visible {
    outline-offset: 1px;
  }

  @media (hover: hover) {
    .region:hover {
      background: var(--c-control);
    }
  }

  .pane {
    min-width: 0;
    min-height: 0;
  }

  .pane-head,
  .pane-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
    color: var(--c-text-dim);
  }

  .pane-foot {
    justify-content: flex-start;
    margin-top: auto;
  }

  .mini-title {
    overflow: hidden;
    font-weight: 600;
    color: var(--c-text-strong);
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .mini-icons {
    display: inline-flex;
    gap: 2px;
  }

  .bars {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .bar {
    display: block;
    height: 6px;
    border-radius: var(--radius-sm);
    background: var(--c-control);
  }

  .bar--short {
    width: 64%;
  }

  .disc {
    align-items: center;
    justify-content: center;
    padding: 4px;
    background: var(--c-bg);
    border-color: transparent;
  }

  .disc-wrap {
    position: relative;
    display: block;
    flex: 1 1 auto;
    align-self: stretch;
    min-height: 0;
  }

  .transport {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }

  .transport-mini {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .mini-play {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 16px;
    height: 12px;
    border-radius: var(--radius-pill);
    background: var(--c-primary);
    color: var(--c-on-primary);
  }

  .mini-track {
    position: relative;
    flex: 1 1 auto;
    height: 6px;
    border-radius: var(--radius-sm);
    background: var(--c-control);
  }

  .mini-head {
    position: absolute;
    top: -2px;
    bottom: -2px;
    left: 35%;
    width: 2px;
    background: var(--c-text);
  }

  .transport .mini-title {
    display: none;
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px;
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
  }

  /* 分頁列：和 App 一樣是一條凹槽，選中的分頁反白。 */
  .tabbar {
    display: flex;
    gap: 1px;
    padding: 2px;
    border-radius: var(--radius-pill);
    background: var(--c-control);
  }

  .tab {
    flex: 1 1 0;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 3px 0;
    border: none;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--c-text-dim);
  }

  .tab.is-selected {
    background: var(--c-invert-bg);
    color: var(--c-on-invert);
    box-shadow: none;
  }

  .tab-text {
    display: none;
  }

  .side-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 2px 2px 0;
  }

  /* ---- 窄視窗：上下排列 ---- */

  .frame.is-narrow {
    display: flex;
    flex-direction: column;
    width: min(100%, 15rem);
    margin: 0 auto;
    aspect-ratio: auto;
  }

  .frame.is-narrow .records {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }

  .frame.is-narrow .records .bars,
  .frame.is-narrow .records .pane-foot {
    display: none;
  }

  .frame.is-narrow .center {
    grid-template-rows: auto auto;
  }

  .frame.is-narrow .disc-wrap {
    flex: none;
    align-self: center;
    width: 72%;
    aspect-ratio: 1;
  }

  .frame.is-narrow .transport .mini-title {
    display: none;
  }

  .frame.is-narrow .side {
    min-height: 7.5rem;
  }

  .frame.is-narrow .tab {
    padding: 3px 4px;
  }

  .frame.is-narrow .tab-text {
    display: inline;
  }

  /* ---- 說明 ---- */

  .info {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  .info-name {
    color: var(--c-text-strong);
    font-size: var(--fs-lg);
    font-weight: 600;
    line-height: var(--lh-tight);
  }

  .info-desc {
    font-size: var(--fs-md);
  }

  .info-link {
    align-self: flex-start;
    margin-top: var(--space-1);
    font-size: var(--fs-md);
  }

  .chips {
    display: none;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  @media (pointer: coarse) {
    .chips {
      display: flex;
    }
  }

  .chip {
    padding: 0 var(--space-3);
  }

  @media (prefers-reduced-motion: reduce) {
    .region {
      transition: none;
    }
  }
</style>
