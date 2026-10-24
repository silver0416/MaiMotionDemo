//! adxdls（https://adxdls.saop.cc/charts）AstroDX 譜面目錄的取得層。
//!
//! - 目錄：`/charts` 頁內嵌的 Next.js 資料（catalog），24 小時磁碟快取。
//! - 譜面：`/adxcs/{slug}/maidata.txt`，只取選定難度後交給既有 parser 驗證。
//! - 目錄資料以 CC BY 4.0 授權（需標示 adxdls）；譜面、曲繪、音訊版權屬原權利人。
//!
//! 和 simai Wiki 一樣只讀公開頁面，不執行頁面 JavaScript，不把 HTML 送進 WebView。

mod catalog;
mod maidata;
mod provider;

pub use catalog::AdxSong;
pub use provider::{
    build_client, cache_dir, clear_memory, fetch_chart, refresh_index, with_songs, AdxChartPayload,
    AdxIndexPayload, AdxState,
};
