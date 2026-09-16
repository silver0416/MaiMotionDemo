//! 兩層快取：index JSON 與歌曲頁 HTML。
//!
//! - Index TTL 24 小時；過期＋Wiki 請求失敗時用 stale 快取，不讓功能直接死亡。
//! - 歌曲頁以 page_id 快取；Standard/DX 共用同一 URL 就共用同一份檔案，
//!   不下載兩次。
//! - 異常小的 index（單邊 < 50 首）不覆蓋舊快取。

use super::model::{WikiError, WikiSong};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Index 快取有效期：24 小時。
pub const INDEX_TTL_SECONDS: u64 = 24 * 3600;
/// 單邊 index 解析結果低於此數量視為版面異常，不覆蓋快取。
pub const MIN_SONGS_PER_INDEX: usize = 50;
const INDEX_CACHE_FILE: &str = "simai_wiki_index.json";
const SONG_HTML_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexCacheFile {
    pub fetched_at: u64,
    pub songs: Vec<WikiSong>,
}

pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn is_fresh(fetched_at: u64, now: u64) -> bool {
    now.saturating_sub(fetched_at) <= INDEX_TTL_SECONDS
}

pub fn index_cache_path(base: &Path) -> PathBuf {
    base.join("wiki").join(INDEX_CACHE_FILE)
}

pub fn song_cache_path(base: &Path, page_id: u32) -> PathBuf {
    base.join("wiki")
        .join("pages")
        .join(format!("{page_id}.html"))
}

/// 損壞的快取視為不存在（回傳 None），不讓整個功能掛掉。
pub fn load_index_cache(path: &Path) -> Option<IndexCacheFile> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_index_cache(path: &Path, songs: &[WikiSong], fetched_at: u64) -> Result<(), WikiError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| WikiError::Cache(format!("無法建立 Wiki 快取目錄：{error}")))?;
    }
    let file = IndexCacheFile {
        fetched_at,
        songs: songs.to_vec(),
    };
    let bytes = serde_json::to_vec(&file)
        .map_err(|error| WikiError::Cache(format!("Wiki 索引快取寫入失敗：{error}")))?;
    std::fs::write(path, bytes)
        .map_err(|error| WikiError::Cache(format!("Wiki 索引快取寫入失敗：{error}")))?;
    Ok(())
}

pub fn load_song_html(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > SONG_HTML_LIMIT_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

pub fn save_song_html(path: &Path, html: &str) -> Result<(), WikiError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| WikiError::Cache(format!("無法建立 Wiki 快取目錄：{error}")))?;
    }
    std::fs::write(path, html)
        .map_err(|error| WikiError::Cache(format!("Wiki 歌曲快取寫入失敗：{error}")))?;
    Ok(())
}
