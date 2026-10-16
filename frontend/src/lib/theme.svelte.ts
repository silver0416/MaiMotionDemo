/**
 * 外觀：深淺模式與配色。依 Claude Design「配色與字體組合」定案保留四組配色，
 * 三組深色（現狀、機台霓虹、暖墨）可選，淺色固定用「淺色工作室」。
 *
 * 存在 localStorage：主視窗與影片同步視窗同源，啟動時同步讀取（避免先閃一次預設配色），
 * 並透過 storage 事件在兩個視窗之間同步。
 */

export type Appearance = 'system' | 'dark' | 'light';
export type DarkPalette = 'classic' | 'neon' | 'ink';
export type Palette = DarkPalette | 'studio';

export interface PaletteInfo<Id extends Palette = Palette> {
  id: Id;
  label: string;
  hint: string;
  /** 設定頁的色票：背景、面板、左手、右手、換手。 */
  swatches: readonly string[];
}

export const DARK_PALETTES: readonly PaletteInfo<DarkPalette>[] = [
  {
    id: 'neon',
    label: '機台霓虹',
    hint: '偏藍紫的黑，洋紅與青更飽和，數字用 Chakra Petch。',
    swatches: ['#0a0a14', '#13131f', '#ff5fb3', '#3fd6f0', '#ffd23f'],
  },
  {
    id: 'classic',
    label: '現狀',
    hint: '改版前的冷灰配色。',
    swatches: ['#0b0d11', '#14181e', '#f2679d', '#4fb8f5', '#f0b429'],
  },
  {
    id: 'ink',
    label: '暖墨',
    hint: '暖灰底，珊瑚與青綠取代粉藍，標題用楷體。',
    swatches: ['#100e0c', '#1a1714', '#f47a6b', '#4cc3b0', '#f2b350'],
  },
];

export const LIGHT_PALETTE: PaletteInfo<'studio'> = {
  id: 'studio',
  label: '淺色工作室',
  hint: '淺底適合白天與投影，盤面保持黑色。',
  swatches: ['#e9e9e6', '#f7f7f5', '#d63c7a', '#1f72c9', '#d9861a'],
};

export const APPEARANCES: readonly { id: Appearance; label: string }[] = [
  { id: 'system', label: '跟隨系統' },
  { id: 'dark', label: '深色' },
  { id: 'light', label: '淺色' },
];

const STORAGE_KEY = 'maimotion.theme.v1';
const DEFAULTS = { appearance: 'system' as Appearance, dark: 'neon' as DarkPalette };

function load(): { appearance: Appearance; dark: DarkPalette } {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null') as unknown;
    if (raw && typeof raw === 'object') {
      const { appearance, dark } = raw as Record<string, unknown>;
      return {
        appearance: APPEARANCES.some((item) => item.id === appearance)
          ? (appearance as Appearance)
          : DEFAULTS.appearance,
        dark: DARK_PALETTES.some((item) => item.id === dark) ? (dark as DarkPalette) : DEFAULTS.dark,
      };
    }
  } catch {
    // 讀不到就用預設外觀。
  }
  return { ...DEFAULTS };
}

const systemQuery =
  typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: light)') : null;

class ThemeState {
  appearance = $state<Appearance>(DEFAULTS.appearance);
  dark = $state<DarkPalette>(DEFAULTS.dark);
  /** 系統目前是不是淺色；跟隨系統時用。 */
  systemLight = $state(systemQuery?.matches ?? false);

  readonly scheme = $derived<'dark' | 'light'>(
    this.appearance === 'light' || (this.appearance === 'system' && this.systemLight) ? 'light' : 'dark',
  );
  readonly palette = $derived<Palette>(this.scheme === 'light' ? 'studio' : this.dark);

  constructor() {
    const stored = load();
    this.appearance = stored.appearance;
    this.dark = stored.dark;
  }

  setAppearance(next: Appearance) {
    this.appearance = next;
    this.save();
  }

  setDark(next: DarkPalette) {
    this.dark = next;
    this.save();
  }

  private save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ appearance: this.appearance, dark: this.dark }));
    } catch {
      // 無法保存時只影響下次啟動。
    }
  }

  /** 套到 <html>：data-palette 決定 tokens，color-scheme 讓原生捲軸與輸入元件跟著深淺。 */
  apply() {
    const root = document.documentElement;
    root.dataset.palette = this.palette;
    root.dataset.scheme = this.scheme;
    root.style.colorScheme = this.scheme;
  }

  /** 啟動時呼叫一次：立即套用，之後跟著設定、系統深淺與另一個視窗的變更更新。 */
  start() {
    this.apply();
    systemQuery?.addEventListener('change', (event) => (this.systemLight = event.matches));
    window.addEventListener('storage', (event) => {
      if (event.key !== STORAGE_KEY) return;
      const stored = load();
      this.appearance = stored.appearance;
      this.dark = stored.dark;
    });
    $effect.root(() => {
      $effect(() => this.apply());
    });
  }
}

export const theme = new ThemeState();
