//! simai Wiki（https://w.atwiki.jp/simai/）官方譜面文字譜的取得層。
//!
//! 職責邊界：
//! - Wiki HTML → raw simai 文字（本模組）。
//! - simai 文字 → Chart → 左右手分析（既有的 `mai-motion-core`，由 provider 在回傳前呼叫驗證）。
//!
//! 資料一律來自公開頁面 `https://w.atwiki.jp/simai/pages/*.html`，不使用編輯 API，
//! 不執行頁面 JavaScript，不把 HTML 送進 WebView，只做文字抽取。

mod cache;
mod client;
mod html;
mod index_parser;
mod model;
mod provider;
mod search;
mod song_parser;

// 只 re-export 命令層（main.rs）需要的符號；其餘模組經 `wiki::xxx` 路徑使用。
// 單元測試直接放在各模組內的 `#[cfg(test)]`，不經由這裡。
pub use client::build_client;
pub use model::{ChartType, Difficulty, WikiSong};
pub use provider::{
    cache_info, clear_cache, fetch_chart, refresh_index, search_cached, WikiCacheInfo,
    WikiChartPayload, WikiIndexPayload, WikiState,
};
