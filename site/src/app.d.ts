// SvelteKit 的全域型別與文件站的建置常數。
declare global {
  /** 手冊對應的 App 版本（vite.config.ts 的 define，讀 repo 根目錄 package.json）。 */
  const __APP_VERSION__: string;
  /** 最新版本的建置時快照（vite.config.ts 的 releaseSnapshot；抓不到 GitHub 時是 package.json 版本配檔名規則）。 */
  const __RELEASE_SNAPSHOT__: import('./lib/release/core').ReleaseInfo | null;

  namespace App {
    // interface Error {}
    // interface PageData {}
  }
}

export {};
