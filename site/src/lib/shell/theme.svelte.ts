/**
 * 文件站的外觀：跟隨系統／深色（App 的 classic，不設 data-palette）／淺色（studio）。
 * 存在 localStorage `maimotion.docs.theme`；第一次繪製前由 src/app.html 的內嵌腳本先套用，
 * 這裡只負責切換與跟隨系統變化。兩邊的規則要一致。
 */
export type ThemeMode = 'system' | 'dark' | 'light';

const KEY = 'maimotion.docs.theme';
const BG = { dark: '#0b0d11', light: '#e9e9e6' } as const;

function read(): ThemeMode {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === 'dark' || saved === 'light') return saved;
  } catch {
    // 無法讀取時跟隨系統。
  }
  return 'system';
}

function systemLight(): boolean {
  return typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: light)').matches;
}

function apply(mode: ThemeMode) {
  const light = mode === 'light' || (mode === 'system' && systemLight());
  const root = document.documentElement;
  if (light) root.setAttribute('data-palette', 'studio');
  else root.removeAttribute('data-palette');
  root.setAttribute('data-scheme', light ? 'light' : 'dark');
  // 手動選了明暗時，兩個 theme-color 都改成該底色；跟隨系統時還原成各自的媒體條件。
  const metas = document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]');
  for (const meta of metas) {
    const forScheme = meta.media.includes('light') ? 'light' : 'dark';
    meta.content = mode === 'system' ? BG[forScheme] : BG[light ? 'light' : 'dark'];
  }
}

class Theme {
  mode = $state<ThemeMode>('system');
  #started = false;

  /** 在瀏覽器掛載後呼叫一次：讀回存檔並跟隨系統變化。 */
  start() {
    if (this.#started) return;
    this.#started = true;
    this.mode = read();
    matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
      if (this.mode === 'system') apply('system');
    });
  }

  set(mode: ThemeMode) {
    this.mode = mode;
    try {
      if (mode === 'system') localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, mode);
    } catch {
      // 無法寫入時只是下次不記得。
    }
    apply(mode);
  }
}

export const theme = new Theme();
