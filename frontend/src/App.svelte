<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { fade, fly } from 'svelte/transition';
  import DiscStage from './components/DiscStage.svelte';
  import Transport from './components/Transport.svelte';
  import SolutionPanel from './components/SolutionPanel.svelte';
  import NoteInspector from './components/NoteInspector.svelte';
  import ConfigPanel from './components/ConfigPanel.svelte';
  import ViewPanel from './components/ViewPanel.svelte';
  import AnnotatePanel from './components/AnnotatePanel.svelte';
  import RecordsPanel from './components/RecordsPanel.svelte';
  import NewChartDialog from './components/NewChartDialog.svelte';
  import SearchDialog from './components/SearchDialog.svelte';
  import SettingsDialog from './components/SettingsDialog.svelte';
  import ResizeHandle from './components/ResizeHandle.svelte';
  import Toaster from './components/Toaster.svelte';
  import Icon, { type IconName } from './components/Icon.svelte';
  import { reducedMotion, squash } from './lib/press';
  import { playback } from './state/playback.svelte';
  import { errorLog } from './state/errorLog.svelte';
  import { markers } from './state/markers.svelte';
  import { annotation } from './state/annotation.svelte';
  import { records } from './state/records.svelte';
  import type { ChartRecord } from './state/records.svelte';
  import { session } from './state/session.svelte';
  import { toasts } from './state/toasts.svelte';
  import { updateState } from './state/update.svelte';
  import { videoSync } from './state/videoSync.svelte';

  type Tab = 'solution' | 'note' | 'config' | 'view' | 'annotate';

  const TABS: { id: Tab; label: string; icon: IconName }[] = [
    { id: 'solution', label: '方案', icon: 'compare' },
    { id: 'note', label: '音符', icon: 'circle-dot' },
    { id: 'config', label: '參數', icon: 'sliders' },
    { id: 'view', label: '顯示', icon: 'eye' },
    { id: 'annotate', label: '標註', icon: 'hand' },
  ];

  const LEFT_DEFAULT = 248;
  const RIGHT_DEFAULT = 360;
  const LEFT_MIN = 180;
  const RIGHT_MIN = 280;
  /** 盤面與播放列至少要留下的寬度。 */
  const CENTER_MIN = 420;
  /** 收合後左欄只留直立膠囊的寬度。 */
  const CAPSULE_WIDTH = 40;
  const PANE_ANIMATION_MS = 260;
  const PANE_STORAGE_KEY = 'maimotion.panes.v1';

  function loadPanes(): { left: number; right: number; collapsed: boolean } {
    try {
      const raw = JSON.parse(localStorage.getItem(PANE_STORAGE_KEY) ?? 'null') as unknown;
      if (raw && typeof raw === 'object') {
        const { left, right, collapsed } = raw as Record<string, unknown>;
        if (typeof left === 'number' && typeof right === 'number') {
          return { left, right, collapsed: collapsed === true };
        }
      }
    } catch {
      // 讀不到就用預設寬度。
    }
    return { left: LEFT_DEFAULT, right: RIGHT_DEFAULT, collapsed: false };
  }

  const savedPanes = loadPanes();

  let tab = $state<Tab>('solution');
  let dialogOpen = $state(false);
  let searchOpen = $state(false);
  let settingsOpen = $state(false);
  /** 指定時新增視窗以編輯模式打開這筆紀錄。 */
  let editingRecord = $state<ChartRecord | null>(null);

  function editRecord(record: ChartRecord) {
    editingRecord = record;
    dialogOpen = true;
  }

  function createRecord() {
    editingRecord = null;
    dialogOpen = true;
  }
  let innerWidth = $state(1280);
  let leftWidth = $state(savedPanes.left);
  let rightWidth = $state(savedPanes.right);
  let collapsed = $state(savedPanes.collapsed);
  /** 只有收合／展開的那一小段時間讓欄寬轉場，拖曳調寬時不能有延遲。 */
  let animatingPane = $state(false);
  let animationTimer: ReturnType<typeof setTimeout> | undefined;

  // 視窗變窄時，欄寬上限跟著收，盤面永遠留得下來。
  const leftMax = $derived(Math.max(LEFT_MIN, Math.min(480, innerWidth - rightWidth - CENTER_MIN)));
  const rightMax = $derived(
    Math.max(
      RIGHT_MIN,
      Math.min(640, innerWidth - (collapsed ? CAPSULE_WIDTH : leftWidth) - CENTER_MIN),
    ),
  );
  const shownLeft = $derived(Math.min(leftWidth, leftMax));
  const shownRight = $derived(Math.min(rightWidth, rightMax));
  const columns = $derived(
    `${collapsed ? CAPSULE_WIDTH : shownLeft}px minmax(0, 1fr) ${shownRight}px`,
  );
  const motion = (duration: number) => (reducedMotion() ? 0 : duration);

  $effect(() => {
    const panes = JSON.stringify({ left: leftWidth, right: rightWidth, collapsed });
    try {
      localStorage.setItem(PANE_STORAGE_KEY, panes);
    } catch {
      // 無法保存時只影響下次啟動的欄寬。
    }
  });

  async function setCollapsed(next: boolean) {
    if (collapsed === next) return;
    clearTimeout(animationTimer);
    animatingPane = !reducedMotion();
    collapsed = next;
    animationTimer = setTimeout(() => (animatingPane = false), PANE_ANIMATION_MS + 40);
    // 按下的按鈕會被換掉，焦點移到另一個狀態的切換鈕，鍵盤使用者不會掉回頁首。
    await tick();
    document.getElementById(next ? 'records-expand' : 'records-collapse')?.focus();
  }

  // 單一播放時鐘：整個應用只有這一個 requestAnimationFrame 迴圈。
  $effect(() => {
    let frame = requestAnimationFrame(function loop(now: number) {
      playback.frame(now);
      frame = requestAnimationFrame(loop);
    });
    return () => cancelAnimationFrame(frame);
  });

  // 真人標註跟著目前開啟的紀錄；換紀錄時先存檔再載入另一份（不論開著哪個分頁）。
  $effect(() => {
    annotation.bind(records.active);
  });

  // 影片同步視窗：譜面狀態、選到的音符與播放狀態都送過去。
  $effect(() => videoSync.start());

  $effect(() => {
    void videoSync.chartState;
    untrack(() => videoSync.sendState());
  });

  // 標註步驟換了（Slide 的起點與滑行是兩步）：影片跳到那一步的時間。
  $effect(() => {
    const step = annotation.currentStep;
    if (step) untrack(() => videoSync.cue(step.note.id, step.time));
  });

  $effect(() => {
    const playing = playback.playing;
    const rate = playback.rate;
    untrack(() => videoSync.playbackChanged(playing, rate));
  });

  $effect(() => {
    const time = playback.time;
    untrack(() => videoSync.timeChanged(time));
  });

  // 在盤面點到音符時，右側自動切到音符明細；正在標註時留在標註分頁。
  $effect(() => {
    if (session.selectedNoteId && untrack(() => tab) !== 'annotate') tab = 'note';
  });

  // 參數改過但還沒重新生成：右下角常駐提醒，直到重新生成或還原參數。
  $effect(() => {
    const show = session.stale && session.phase !== 'analyzing';
    untrack(() => {
      if (show) {
        toasts.show({
          id: 'stale',
          tone: 'info',
          title: '結果已過期',
          body: '參數已變更，盤面仍是舊參數的分析結果。',
          sticky: true,
          action: { label: '重新生成', run: () => void session.analyze() },
        });
      } else {
        toasts.dismiss('stale');
      }
    });
  });

  // 每次開啟都自動檢查一次更新；有新版才提示，失敗靜默忽略。從舊版更新過來時先問要不要刪舊版。
  $effect(() => {
    let cancelled = false;
    (async () => {
      await updateState.init();
      await updateState.askAboutPrevious();
      if (cancelled || !updateState.shouldAutoCheck()) return;
      const info = await updateState.check();
      if (cancelled || !info?.hasUpdate) return;
      if (updateState.canSelfUpdate) {
        toasts.show({
          id: 'update-available',
          tone: 'info',
          title: `有新版本 v${info.latest}`,
          body: '可以直接下載更新，完成後重新啟動就換成新版。',
          sticky: true,
          action: { label: '下載更新', icon: 'download', run: () => void updateState.download() },
        });
        return;
      }
      toasts.show({
        id: 'update-available',
        tone: 'info',
        title: `有新版本 v${info.latest}`,
        body:
          updateState.distribution === 'installed'
            ? '請到 GitHub 下載新版安裝。'
            : 'Portable 版請到 GitHub 下載新版 exe 取代舊檔。',
        action: { label: '前往下載', icon: 'download', run: () => void updateState.openDownload() },
      });
    })();
    return () => {
      cancelled = true;
    };
  });

  function isTyping(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    const name = target.tagName;
    return (
      name === 'INPUT' || name === 'TEXTAREA' || name === 'SELECT' || target.isContentEditable
    );
  }

  function onKeydown(event: KeyboardEvent) {
    if (dialogOpen || searchOpen || settingsOpen) return;
    if (isTyping(event.target) || event.ctrlKey || event.metaKey || event.altKey) return;
    // 標註分頁的快捷鍵（A／D／S／N／Enter／Delete）優先；按鈕上的 Enter 仍交給按鈕。
    if (
      tab === 'annotate' &&
      !(event.key === 'Enter' && event.target instanceof HTMLButtonElement) &&
      annotation.handleKey(event)
    ) {
      event.preventDefault();
      return;
    }
    switch (event.key) {
      case ' ':
        event.preventDefault();
        playback.toggle();
        break;
      case 'ArrowLeft':
        event.preventDefault();
        playback.nudge(event.shiftKey ? -1 : -0.1);
        break;
      case 'ArrowRight':
        event.preventDefault();
        playback.nudge(event.shiftKey ? 1 : 0.1);
        break;
      case 'Home':
        event.preventDefault();
        playback.reset();
        break;
      case 'l':
      case 'L':
        event.preventDefault();
        playback.setLoop(!playback.loopEnabled);
        break;
      case 'i':
      case 'I':
        if (!session.result) break;
        event.preventDefault();
        playback.setLoopStart(playback.time);
        if (!playback.loopEnabled) playback.setLoop(true);
        break;
      case 'o':
      case 'O':
        if (!session.result) break;
        event.preventDefault();
        playback.setLoopEnd(playback.time);
        if (!playback.loopEnabled) playback.setLoop(true);
        break;
      case 'm':
      case 'M':
        if (!markers.available) break;
        event.preventDefault();
        markers.add(Math.round(playback.time * 1000) / 1000);
        break;
      case '[':
      case ']': {
        const target =
          event.key === '[' ? markers.previous(playback.time) : markers.next(playback.time);
        if (!target) break;
        event.preventDefault();
        playback.seek(target.time);
        break;
      }
      default:
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} bind:innerWidth />

<div class="app">
  <main class="layout" class:is-animating={animatingPane} style={`grid-template-columns:${columns}`}>
    <div class="column records-column" class:is-collapsed={collapsed}>
      {#if collapsed}
        <nav
          class="capsule"
          aria-label="譜面紀錄（已收合）"
          in:fade={{ duration: motion(160), delay: motion(110) }}
          out:fade={{ duration: motion(90) }}
        >
          <button
            id="records-expand"
            class="capsule-btn"
            use:squash
            onclick={() => setCollapsed(false)}
            aria-label="展開譜面紀錄"
            title="展開譜面紀錄"
          >
            <Icon name="chevron-right" size={16} />
          </button>
          <button
            class="capsule-btn"
            onclick={createRecord}
            aria-label="新增譜面"
            title="新增譜面"
          >
            <Icon name="plus" size={16} />
          </button>
          <button
            class="capsule-btn"
            onclick={() => (searchOpen = true)}
            aria-label="搜尋譜面"
            title="搜尋譜面（Majdata／simai Wiki）"
          >
            <Icon name="search" size={16} />
          </button>
          <span class="capsule-rule" aria-hidden="true"></span>
          <button
            class="capsule-btn"
            class:has-errors={errorLog.entries.length > 0}
            onclick={() => (settingsOpen = true)}
            aria-label={errorLog.entries.length > 0 ? `設定（${errorLog.entries.length} 筆錯誤紀錄）` : '設定'}
            title="設定"
          >
            <Icon name="settings" size={16} />
          </button>
        </nav>
      {:else}
        <div
          class="records-slot"
          style={`width:${shownLeft}px`}
          in:fly={{ x: -20, duration: motion(220), delay: motion(60) }}
          out:fly={{ x: -20, duration: motion(140) }}
        >
          <aside class="pane records-pane" aria-label="譜面紀錄">
            <RecordsPanel
              onCreate={createRecord}
              onEdit={editRecord}
              onSettings={() => (settingsOpen = true)}
              onSearch={() => (searchOpen = true)}
              onCollapse={() => setCollapsed(true)}
            />
          </aside>
          <ResizeHandle
            label="調整譜面紀錄欄寬度"
            side="right"
            value={shownLeft}
            min={LEFT_MIN}
            max={leftMax}
            defaultValue={LEFT_DEFAULT}
            direction={1}
            onChange={(value) => (leftWidth = value)}
          />
        </div>
      {/if}
    </div>

    <section class="stage-column">
      <div class="stage-area">
        <DiscStage />
      </div>
      <div class="transport-slot">
        <Transport />
      </div>
    </section>

    <div class="column side-wrap">
      <aside class="pane side-column">
        <div class="tabbar" role="group" aria-label="側欄面板">
          {#each TABS as item (item.id)}
            <button
              class="btn tab"
              class:is-active={tab === item.id}
              aria-pressed={tab === item.id}
              onclick={() => (tab = item.id)}
            >
              <Icon name={item.icon} size={14} />
              {item.label}
            </button>
          {/each}
        </div>
        <div class="side-body scroll">
          {#if tab === 'solution'}
            <SolutionPanel />
          {:else if tab === 'note'}
            <NoteInspector />
          {:else if tab === 'config'}
            <ConfigPanel />
          {:else if tab === 'annotate'}
            <AnnotatePanel />
          {:else}
            <ViewPanel />
          {/if}
        </div>
      </aside>
      <ResizeHandle
        label="調整資訊欄寬度"
        side="left"
        value={shownRight}
        min={RIGHT_MIN}
        max={rightMax}
        defaultValue={RIGHT_DEFAULT}
        direction={-1}
        onChange={(value) => (rightWidth = value)}
      />
    </div>
  </main>

  <NewChartDialog bind:open={dialogOpen} bind:editing={editingRecord} />
  <SearchDialog bind:open={searchOpen} />
  <SettingsDialog bind:open={settingsOpen} />
  <Toaster />
</div>

<style>
  .app {
    height: 100%;
    background: var(--c-bg);
  }

  /* 三欄：譜面紀錄｜盤面與播放列｜資訊分頁。欄寬由面板邊緣上的分隔線拖曳，中間吃剩下的空間。 */
  .layout {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    column-gap: var(--space-3);
    height: 100%;
    padding: var(--space-3);
    min-height: 0;
  }

  .layout.is-animating {
    transition: grid-template-columns 260ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  /* 欄位外框：分隔線掛在面板邊緣，跨出面板一半，不能被面板的 overflow 裁掉。 */
  .column {
    position: relative;
    min-height: 0;
    min-width: 0;
  }

  /* 轉場期間面板維持原寬、由欄位裁切，文字不會被擠到換行；
     多留幾像素給跨在邊緣上的分隔線。 */
  .records-column {
    overflow: clip;
    overflow-clip-margin: 8px;
  }

  .records-slot {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
  }

  .records-pane {
    height: 100%;
  }

  .capsule {
    position: absolute;
    top: 0;
    left: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    width: 40px;
    padding: 4px;
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: 999px;
  }

  .capsule-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    color: var(--c-text-dim);
    background: none;
    border: none;
    border-radius: 999px;
    cursor: pointer;
  }

  .capsule-btn:hover {
    color: var(--c-text);
    background: var(--c-control-hover);
  }

  .capsule-rule {
    width: 20px;
    height: 1px;
    background: var(--c-border);
  }

  /* 有未回報的錯誤：圖示右上角一個實心點，另有 aria-label 說明筆數。 */
  .capsule-btn.has-errors {
    position: relative;
  }

  .capsule-btn.has-errors::after {
    content: '';
    position: absolute;
    top: 5px;
    right: 5px;
    width: 7px;
    height: 7px;
    background: var(--c-danger);
    border: 1px solid var(--c-panel);
    border-radius: 999px;
  }

  .pane {
    min-height: 0;
    min-width: 0;
    background: var(--c-panel);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  /* 盤面欄位的高度固定，切換譜面時盤面不會被重新縮放。 */
  .stage-column {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    min-width: 0;
  }

  .transport-slot {
    flex: none;
  }

  /* 盤面所在的儲存格：可用高度扣掉播放列之後剩下的空間，
     DiscStage 以絕對定位置中並等比例縮放，播放列因此永遠留在視窗內。 */
  .stage-area {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
  }

  .side-wrap {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
  }

  .side-column {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
  }

  .tabbar {
    display: flex;
    gap: var(--space-1);
    padding: var(--space-2);
    border-bottom: 1px solid var(--c-border);
  }

  .tab {
    flex: 1 1 0;
    min-width: 0;
    gap: var(--space-1);
    padding: 0 var(--space-2);
  }

  .side-body {
    min-height: 0;
  }

  /* 只有真的很窄（低於 900px）才改成上下排列並允許整頁捲動，分隔線隱藏。 */
  @media (max-width: 899px) {
    .layout {
      display: flex;
      flex-direction: column;
      gap: var(--space-3);
      overflow: auto;
    }

    .layout :global(.handle) {
      display: none;
    }

    .records-column {
      flex: none;
      height: 240px;
    }

    .records-column.is-collapsed {
      height: 40px;
    }

    .records-slot {
      width: 100% !important;
    }

    .capsule {
      flex-direction: row;
      width: auto;
    }

    .capsule-rule {
      width: 1px;
      height: 20px;
    }

    .stage-column {
      flex: none;
    }

    .stage-area {
      flex: 0 0 auto;
      height: min(52vh, 460px);
      min-height: 260px;
    }

    .side-wrap {
      flex: none;
    }

    .side-column {
      min-height: 360px;
      height: min(70vh, 620px);
    }
  }
</style>
