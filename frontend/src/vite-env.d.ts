/// <reference types="svelte" />
/// <reference types="vite/client" />

// fixtures/*.json 由 Rust 產生且體積大；不開 resolveJsonModule，改以 unknown 匯入後再驗證。
declare module '*.json' {
  const value: unknown;
  export default value;
}

/** 由 vite.config.ts 在建置時注入。 */
declare const __APP_VERSION__: string;
declare const __BUILD_ID__: string;
declare const __BUILD_TIME__: string;
