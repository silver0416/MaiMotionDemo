/**
 * 前端建置資訊，由 vite.config.ts 在啟動或建置時注入。
 * 桌面版的 App 版本以 Tauri 設定為準，見 api.ts appVersion()。
 * 設定檔改動前就已啟動的開發伺服器沒有這些常數，退回 unknown 而不是整頁當掉。
 */
export const FRONTEND_VERSION: string = typeof __APP_VERSION__ === 'string' ? __APP_VERSION__ : 'unknown';
export const BUILD_ID: string = typeof __BUILD_ID__ === 'string' ? __BUILD_ID__ : 'unknown';
export const BUILD_TIME: string = typeof __BUILD_TIME__ === 'string' ? __BUILD_TIME__ : 'unknown';
