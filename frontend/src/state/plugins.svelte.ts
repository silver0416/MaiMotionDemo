// 選用外掛的狀態：已安裝清單、各外掛要顯示的內容、正在執行的動作與進度、等使用者勾選的項目。
// 主程式不知道外掛做什麼，只照外掛回報的摘要與動作顯示；這裡不寫死任何特定外掛。
// 設定頁關掉後元件仍在（App 一直掛著），執行到一半關掉再打開也能接著看進度。

import { CANCELLED, isDesktop, pluginApi } from '../lib/api';
import { listenEvent } from '../lib/video';
import type {
  PluginAction,
  PluginInfo,
  PluginPanel,
  PluginProgress,
  PluginSelectRequest,
} from '../lib/types';
import { toasts } from './toasts.svelte';

/** 一個外掛正在執行的動作。 */
export interface PluginRun {
  done: number;
  /** null：還不知道總數（不定進度）。 */
  total: number | null;
  message: string | null;
  /** 不是這次開啟後按下的（重新載入後從清單接回），拿不到結果，只能顯示進度與取消。 */
  attached: boolean;
  cancelling: boolean;
}

/** 等使用者在勾選對話框確認的項目。 */
export interface PendingSelect {
  pluginId: string;
  pluginName: string;
  request: PluginSelectRequest;
}

/** 接回的執行多久問一次是否已結束。 */
const ATTACHED_POLL_MS = 2000;

function message(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : String(error);
}

export class PluginManager {
  readonly desktop = isDesktop();
  list = $state<PluginInfo[]>([]);
  /** 讀過一次清單（含失敗）；之前顯示骨架。 */
  loaded = $state(false);
  listError = $state<string | null>(null);
  panels = $state<Record<string, PluginPanel>>({});
  panelErrors = $state<Record<string, string>>({});
  runs = $state<Record<string, PluginRun>>({});
  installing = $state(false);
  /** 正在移除的外掛 id。 */
  removing = $state<string | null>(null);
  select = $state<PendingSelect | null>(null);

  #started = false;
  #poll: ReturnType<typeof setInterval> | undefined;

  get anyRunning(): boolean {
    return Object.keys(this.runs).length > 0;
  }

  /** 開始監聽進度事件；重複呼叫不會重複監聽。 */
  async init(): Promise<void> {
    if (!this.desktop || this.#started) return;
    this.#started = true;
    await listenEvent<PluginProgress>('plugin-progress', (event) => {
      // 只收正在執行的；動作結束後才到的舊事件不會把它標回執行中。
      const run = this.runs[event.id];
      if (!run) return;
      run.done = event.done;
      run.total = event.total;
      run.message = event.message;
    });
  }

  /** 重讀清單與每個外掛的內容（執行中的外掛保留原本的內容，不另外叫醒它）。 */
  async refresh(): Promise<void> {
    if (!this.desktop) return;
    await this.init();
    await this.#loadList();
    await Promise.all(this.list.filter((plugin) => !this.runs[plugin.id]).map((plugin) => this.loadPanel(plugin.id)));
  }

  async loadPanel(id: string): Promise<void> {
    try {
      this.panels[id] = await pluginApi.panel(id);
      delete this.panelErrors[id];
    } catch (error) {
      this.panelErrors[id] = message(error);
    }
  }

  /** 讀清單；清單說在執行的接回進度，接回的執行結束了就收掉並重讀內容。 */
  async #loadList(): Promise<void> {
    try {
      const list = await pluginApi.list();
      this.list = list;
      this.listError = null;
      const ids = new Set(list.map((plugin) => plugin.id));
      for (const plugin of list) {
        if (plugin.running && !this.runs[plugin.id]) {
          this.runs[plugin.id] = { done: 0, total: null, message: null, attached: true, cancelling: false };
        }
      }
      const finished: string[] = [];
      for (const [id, run] of Object.entries(this.runs)) {
        if (run.attached && !list.some((plugin) => plugin.id === id && plugin.running)) {
          delete this.runs[id];
          finished.push(id);
        }
      }
      for (const id of Object.keys(this.panels)) if (!ids.has(id)) delete this.panels[id];
      for (const id of Object.keys(this.panelErrors)) if (!ids.has(id)) delete this.panelErrors[id];
      this.#syncPoll();
      await Promise.all(finished.filter((id) => ids.has(id)).map((id) => this.loadPanel(id)));
    } catch (error) {
      this.listError = message(error);
    } finally {
      this.loaded = true;
    }
  }

  #syncPoll(): void {
    const attached = Object.values(this.runs).some((run) => run.attached);
    if (attached && this.#poll === undefined) {
      this.#poll = setInterval(() => void this.#loadList(), ATTACHED_POLL_MS);
    } else if (!attached && this.#poll !== undefined) {
      clearInterval(this.#poll);
      this.#poll = undefined;
    }
  }

  /** 選 .mmplugin 檔並安裝；同代號的舊版會被換掉。 */
  async install(): Promise<void> {
    if (!this.desktop || this.installing) return;
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({
      multiple: false,
      directory: false,
      title: '選擇外掛檔',
      filters: [{ name: '外掛', extensions: ['mmplugin'] }],
    });
    if (typeof picked !== 'string') return;
    this.installing = true;
    try {
      const info = await pluginApi.install(picked);
      toasts.show({ id: 'settings-plugin', tone: 'ok', title: `已安裝「${info.name}」${info.version}` });
      await this.#loadList();
      await this.loadPanel(info.id);
    } catch (error) {
      toasts.show({ id: 'settings-plugin', tone: 'error', title: '無法安裝外掛', body: message(error) });
    } finally {
      this.installing = false;
    }
  }

  /** 移除外掛；成功回傳 true。 */
  async remove(plugin: PluginInfo, deleteData: boolean): Promise<boolean> {
    if (this.removing || this.runs[plugin.id]) return false;
    this.removing = plugin.id;
    try {
      await pluginApi.remove(plugin.id, deleteData);
      toasts.show({
        id: 'settings-plugin',
        tone: 'ok',
        title: deleteData ? `已移除「${plugin.name}」和它的資料` : `已移除「${plugin.name}」`,
      });
      await this.#loadList();
      return true;
    } catch (error) {
      toasts.show({ id: 'settings-plugin', tone: 'error', title: '無法移除外掛', body: message(error) });
      return false;
    } finally {
      this.removing = null;
    }
  }

  /** 按下外掛的動作（確認已在畫面上做過）：要存檔的先選位置，取消就不做。 */
  async runAction(plugin: PluginInfo, action: PluginAction): Promise<void> {
    if (this.runs[plugin.id]) return;
    let savePath: string | undefined;
    if (action.save) {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const extensions = action.save.extensions.filter((item) => item.length > 0);
      const path = await save({
        title: action.label,
        defaultPath: action.save.defaultName,
        ...(extensions.length > 0
          ? { filters: [{ name: action.save.filterName ?? extensions.join('、'), extensions }] }
          : {}),
      });
      if (!path) return;
      savePath = path;
    }
    await this.#run(plugin.id, plugin.name, action.id, action.label, { savePath });
  }

  /** 勾選對話框確認：以外掛指定的動作再執行一次。 */
  async confirmSelect(selection: string[]): Promise<void> {
    const pending = this.select;
    if (!pending || selection.length === 0) return;
    this.select = null;
    await this.#run(pending.pluginId, pending.pluginName, pending.request.action, pending.request.title, { selection });
  }

  async cancel(id: string): Promise<void> {
    const run = this.runs[id];
    if (!run || run.cancelling) return;
    run.cancelling = true;
    const ok = await pluginApi.cancel(id).catch(() => false);
    // 已經結束（或還沒開始）時取消不了，按鈕回復。
    if (!ok && this.runs[id]) this.runs[id].cancelling = false;
  }

  async #run(
    id: string,
    name: string,
    action: string,
    label: string,
    options: { savePath?: string; selection?: string[] },
  ): Promise<void> {
    if (this.runs[id]) return;
    this.runs[id] = { done: 0, total: null, message: null, attached: false, cancelling: false };
    const toastId = `plugin-${id}`;
    try {
      const result = await pluginApi.run(id, action, options);
      if (result.message) {
        toasts.show({ id: toastId, tone: 'ok', title: `${name}：${label}完成`, body: result.message, keepBody: true });
      } else if (!result.select) {
        toasts.show({ id: toastId, tone: 'ok', title: `${name}：${label}完成` });
      }
      if (result.select) {
        this.select = { pluginId: id, pluginName: name, request: result.select };
      }
    } catch (error) {
      const text = message(error);
      // 使用者按了取消：和分析取消一樣不另外通知。
      if (text !== CANCELLED) {
        toasts.show({ id: toastId, tone: 'error', title: `${name}：${label}失敗`, body: text });
      }
    } finally {
      delete this.runs[id];
      await this.#loadList();
      if (this.list.some((plugin) => plugin.id === id)) await this.loadPanel(id);
    }
  }
}

export const plugins = new PluginManager();
