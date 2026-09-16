//! 對 UI 暴露的高階 API：index 載入、本地搜尋、歌曲頁下載與譜面抽取。
//!
//! 流程：
//! ```text
//! Wiki HTML → provider → raw simai text → 既有 mai-motion-core parser → Chart
//! ```
//! 搜尋只查本地 index，不在每次按鍵時打 Wiki；選中歌曲才 GET 該頁。

use super::cache::{
    index_cache_path, is_fresh, load_index_cache, load_song_html, now_unix_seconds,
    save_index_cache, save_song_html, song_cache_path, MIN_SONGS_PER_INDEX,
};
use super::client::{fetch_deluxe_index_html, fetch_song_html, fetch_standard_index_html};
use super::index_parser::parse_index_html;
use super::model::{ChartType, Difficulty, WikiError, WikiSong};
use super::search::search_songs_filtered;
use super::song_parser::parse_song_page;
use reqwest::Client;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// 未快取的歌曲請求之間至少間隔，避免短時間連打 Wiki。
const SONG_REQUEST_SPACING: Duration = Duration::from_millis(800);
const MAX_QUERY_CHARS: usize = 100;

pub struct WikiState {
    client: Client,
    index: RwLock<Vec<WikiSong>>,
    fetched_at: RwLock<Option<u64>>,
    last_song_fetch: Mutex<Option<Instant>>,
}

impl WikiState {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            index: RwLock::new(Vec::new()),
            fetched_at: RwLock::new(None),
            last_song_fetch: Mutex::new(None),
        }
    }
}

/// Index 載入結果；`stale` 表示 Wiki 失敗、退回使用過期快取。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiIndexPayload {
    pub songs: Vec<WikiSong>,
    pub fetched_at: u64,
    pub from_cache: bool,
    pub stale: bool,
}

/// 譜面載入結果：raw simai 文字已通過既有 parser 驗證，
/// 可直接送 `analyze_chart` 跑左右手分析。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiChartPayload {
    pub chart_text: String,
    pub title: String,
    pub artist: Option<String>,
    pub bpm: Option<f64>,
    pub level: Option<String>,
    pub page_url: String,
    pub difficulty: Difficulty,
    pub chart_type: ChartType,
}

/// 快取根目錄：Tauri 快取目錄；取不到時退回系統暫存（測試與例外狀況）。
pub fn cache_base(app: &AppHandle) -> PathBuf {
    app.path()
        .app_cache_dir()
        .map(|dir| dir.join("MaiMotionDemo"))
        .unwrap_or_else(|_| std::env::temp_dir().join("MaiMotionDemo"))
}

/// 載入 Standard＋DX index。記憶體已有資料且非強制時直接回傳；
/// 否則讀新鮮快取，否則抓 Wiki，失敗則退回 stale 快取。
pub async fn refresh_index(
    app: &AppHandle,
    state: &WikiState,
    force: bool,
) -> Result<WikiIndexPayload, String> {
    if !force {
        if let Some(payload) = memory_payload(state) {
            return Ok(payload);
        }
    }
    let base = cache_base(app);
    let cache_path = index_cache_path(&base);
    if !force {
        if let Some(cached) = load_index_cache(&cache_path) {
            if is_fresh(cached.fetched_at, now_unix_seconds()) {
                store_memory(state, &cached.songs, cached.fetched_at);
                return Ok(WikiIndexPayload {
                    songs: cached.songs,
                    fetched_at: cached.fetched_at,
                    from_cache: true,
                    stale: false,
                });
            }
        }
    }
    match fetch_fresh_index(&state.client).await {
        Ok(songs) => {
            let fetched_at = now_unix_seconds();
            if let Err(error) = save_index_cache(&cache_path, &songs, fetched_at) {
                // 快取寫入失敗不擋回傳，只記錄。
                eprintln!("simai Wiki index cache save failed: {error}");
            }
            store_memory(state, &songs, fetched_at);
            Ok(WikiIndexPayload {
                songs,
                fetched_at,
                from_cache: false,
                stale: false,
            })
        }
        Err(error) => {
            // Wiki 掛掉時用 stale 快取保底，不讓功能直接死亡。
            if let Some(cached) = load_index_cache(&cache_path) {
                store_memory(state, &cached.songs, cached.fetched_at);
                return Ok(WikiIndexPayload {
                    songs: cached.songs,
                    fetched_at: cached.fetched_at,
                    from_cache: true,
                    stale: true,
                });
            }
            Err(error.to_string())
        }
    }
}

async fn fetch_fresh_index(client: &Client) -> Result<Vec<WikiSong>, WikiError> {
    let standard_html = fetch_standard_index_html(client).await?;
    let standard = parse_index_html(&standard_html, ChartType::Standard).map_err(|_| {
        WikiError::InvalidIndexPage("Standard 索引頁解析失敗，版面可能已變更".into())
    })?;
    if standard.len() < MIN_SONGS_PER_INDEX {
        return Err(WikiError::InvalidIndexPage(
            "Standard 索引歌曲數異常偏低，版面可能已變更，已保留舊快取".into(),
        ));
    }
    let deluxe_html = fetch_deluxe_index_html(client).await?;
    let deluxe = parse_index_html(&deluxe_html, ChartType::Deluxe)
        .map_err(|_| WikiError::InvalidIndexPage("DX 索引頁解析失敗，版面可能已變更".into()))?;
    if deluxe.len() < MIN_SONGS_PER_INDEX {
        return Err(WikiError::InvalidIndexPage(
            "DX 索引歌曲數異常偏低，版面可能已變更，已保留舊快取".into(),
        ));
    }
    let mut songs = standard;
    songs.extend(deluxe);
    songs.sort_by_key(|song| (song.chart_type, song.page_id));
    Ok(songs)
}

fn memory_payload(state: &WikiState) -> Option<WikiIndexPayload> {
    let index = state.index.read().ok()?;
    if index.is_empty() {
        return None;
    }
    let fetched_at = state.fetched_at.read().ok()?.unwrap_or(0);
    Some(WikiIndexPayload {
        songs: index.clone(),
        fetched_at,
        from_cache: false,
        stale: false,
    })
}

fn store_memory(state: &WikiState, songs: &[WikiSong], fetched_at: u64) {
    if let Ok(mut index) = state.index.write() {
        *index = songs.to_vec();
    }
    if let Ok(mut at) = state.fetched_at.write() {
        *at = Some(fetched_at);
    }
}

/// 本地搜尋：只查記憶體／磁碟快取，不打 Wiki。
pub fn search_cached(
    app: &AppHandle,
    state: &WikiState,
    query: &str,
    chart_type: Option<&str>,
) -> Result<Vec<WikiSong>, String> {
    let query = query.trim();
    let count = query.chars().count();
    if count == 0 {
        return Err("請輸入要搜尋的歌曲名稱".into());
    }
    if count > MAX_QUERY_CHARS {
        return Err(format!("搜尋文字不可超過 {MAX_QUERY_CHARS} 個字元"));
    }
    let filter = match chart_type.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("all") => None,
        Some(name) => match ChartType::from_param(name) {
            Some(chart_type) => Some(chart_type),
            None => return Err("譜面種類篩選無效".into()),
        },
    };
    ensure_memory(app, state)?;
    let index = state
        .index
        .read()
        .map_err(|_| "Wiki 索引讀取失敗，請重試".to_string())?;
    Ok(search_songs_filtered(&index, query, filter))
}

/// 記憶體沒有 index 時，從磁碟快取補（新鮮或過期皆可），仍沒有才報錯。
fn ensure_memory(app: &AppHandle, state: &WikiState) -> Result<(), String> {
    let has_data = state
        .index
        .read()
        .map(|index| !index.is_empty())
        .unwrap_or(false);
    if has_data {
        return Ok(());
    }
    let cache_path = index_cache_path(&cache_base(app));
    if let Some(cached) = load_index_cache(&cache_path) {
        store_memory(state, &cached.songs, cached.fetched_at);
        return Ok(());
    }
    Err("尚未載入 simai Wiki 索引，請先重新整理".into())
}

/// 下載歌曲頁並抽取指定難度的 raw simai 文字，最後用既有 parser 驗證。
pub async fn fetch_chart(
    app: &AppHandle,
    state: &WikiState,
    page_id: u32,
    chart_type: ChartType,
    difficulty: Difficulty,
) -> Result<WikiChartPayload, String> {
    if page_id == 0 || page_id > 9_999_999 {
        return Err(WikiError::InvalidParams("Wiki page id 格式無效".into()).to_string());
    }
    let base = cache_base(app);
    let song_path = song_cache_path(&base, page_id);
    let html = match load_song_html(&song_path) {
        Some(cached) => cached,
        None => {
            space_song_requests(state);
            let fresh = fetch_song_html(&state.client, page_id)
                .await
                .map_err(|error| error.to_string())?;
            if let Err(error) = save_song_html(&song_path, &fresh) {
                eprintln!("simai Wiki song cache save failed: {error}");
            }
            fresh
        }
    };
    let page = parse_song_page(&html, page_id).map_err(|error| error.to_string())?;
    let section = page
        .charts
        .get(&difficulty)
        .ok_or_else(|| WikiError::ChartNotFound(difficulty).to_string())?;
    if !section.available {
        return Err(WikiError::ChartUnavailable(difficulty).to_string());
    }
    let chart_text = section.raw_text.clone();
    // 交給既有 simai parser 驗證：Wiki 層不解讀語法，只傳遞。
    if let Err(diagnostic) = mai_motion_core::parse_chart(&chart_text, 0.0) {
        return Err(WikiError::SimaiParseError(format!(
            "Wiki 譜面未通過 simai 解析（{}）：{}",
            diagnostic.code, diagnostic.message
        ))
        .to_string());
    }
    let level = state.index.read().ok().and_then(|index| {
        index.iter().find_map(|song| {
            if song.chart_type == chart_type && song.page_id == page_id {
                song.difficulties
                    .get(&difficulty)
                    .and_then(|info| info.level.clone())
            } else {
                None
            }
        })
    });
    let title = if page.title.trim().is_empty() {
        state
            .index
            .read()
            .ok()
            .and_then(|index| {
                index.iter().find_map(|song| {
                    if song.chart_type == chart_type && song.page_id == page_id {
                        Some(song.title.clone())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default()
    } else {
        page.title.clone()
    };
    Ok(WikiChartPayload {
        chart_text,
        title,
        artist: page.artist.clone(),
        bpm: page.bpm,
        level,
        page_url: format!("https://w.atwiki.jp/simai/pages/{page_id}.html"),
        difficulty,
        chart_type,
    })
}

/// 未快取的歌曲請求之間保留基本間隔。只在需要等待時才 sleep，
/// 且這裡跑在 Tauri 背景工作，不阻塞 UI。
fn space_song_requests(state: &WikiState) {
    let wait = match state.last_song_fetch.lock() {
        Ok(mut last) => {
            let now = Instant::now();
            let wait = match *last {
                Some(previous) => SONG_REQUEST_SPACING.checked_sub(now - previous),
                None => None,
            };
            *last = Some(now + wait.unwrap_or(Duration::ZERO));
            wait
        }
        Err(_) => None,
    };
    if let Some(wait) = wait {
        std::thread::sleep(wait);
    }
}
