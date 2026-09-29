// 匯出標記檔的存檔位置：每次詢問，或直接存到設定的預設資料夾。
import { isDesktop } from '../lib/api';
import { STORE_SETTINGS, dbGet, dbPut } from '../lib/db';
import { toasts } from './toasts.svelte';

const PREFS_KEY = 'export';

interface StoredPrefs {
  dir: string;
  skipAsk: boolean;
}

class ExportPrefs {
  /** 預設資料夾；空字串表示沒有指定。 */
  dir = $state('');
  /** 有預設資料夾時不再跳出另存新檔，直接存過去。 */
  skipAsk = $state(false);

  constructor() {
    void dbGet<StoredPrefs>(STORE_SETTINGS, PREFS_KEY).then((stored) => {
      if (!stored) return;
      if (typeof stored.dir === 'string') this.dir = stored.dir;
      if (typeof stored.skipAsk === 'boolean') this.skipAsk = stored.skipAsk;
    });
  }

  #save(): void {
    void dbPut(STORE_SETTINGS, { dir: this.dir, skipAsk: this.skipAsk }, PREFS_KEY);
  }

  setDir(dir: string): void {
    this.dir = dir;
    if (!dir) this.skipAsk = false;
    this.#save();
  }

  setSkipAsk(on: boolean): void {
    this.skipAsk = on && this.dir !== '';
    this.#save();
  }

  /** 清除使用者資料後呼叫。 */
  reset(): void {
    this.dir = '';
    this.skipAsk = false;
  }

  /** 以資料夾選擇對話框挑預設資料夾；取消時不改。 */
  async pickDir(): Promise<void> {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ directory: true, multiple: false, defaultPath: this.dir || undefined, title: '選擇預設匯出資料夾' });
    if (typeof picked === 'string' && picked) this.setDir(picked);
  }
}

export const exportPrefs = new ExportPrefs();

function download(text: string, fileName: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

async function joinPath(dir: string, name: string): Promise<string> {
  const { join } = await import('@tauri-apps/api/path');
  return await join(dir, name);
}

/**
 * 存匯出的標記檔。桌面版：有預設資料夾且勾選不再詢問就直接存（同名加編號），否則跳出另存新檔；
 * 瀏覽器預覽改為下載。回傳實際路徑；取消回傳 null；瀏覽器下載回傳檔名。
 */
export async function saveExport(text: string, fileName: string): Promise<string | null> {
  if (!isDesktop()) {
    download(text, fileName);
    return fileName;
  }
  const { invoke } = await import('@tauri-apps/api/core');
  if (exportPrefs.dir && exportPrefs.skipAsk) {
    return await invoke<string>('save_text_in_dir', { dir: exportPrefs.dir, name: fileName, contents: text });
  }
  const { save } = await import('@tauri-apps/plugin-dialog');
  const path = await save({
    title: '匯出標記檔',
    defaultPath: exportPrefs.dir ? await joinPath(exportPrefs.dir, fileName) : fileName,
    filters: [{ name: '標記檔', extensions: ['json'] }],
  });
  if (!path) return null;
  return await invoke<string>('save_text_file', { path: path.toLowerCase().endsWith('.json') ? path : `${path}.json`, contents: text });
}

/** 存檔後的提示：桌面版附「在資料夾中顯示」。 */
export function showSaved(id: string, title: string, path: string, note?: string): void {
  toasts.show({
    id,
    tone: 'ok',
    title,
    body: note ? `${note}\n${path}` : path,
    ...(isDesktop()
      ? {
          action: {
            label: '在資料夾中顯示',
            icon: 'folder-open' as const,
            run: () => {
              void import('@tauri-apps/plugin-opener').then(({ revealItemInDir }) => revealItemInDir(path));
            },
          },
        }
      : {}),
  });
}
