//! 對 UI 暴露的高階 API：目錄載入（含磁碟快取）、譜面下載與驗證。
//!
//! 流程與 simai Wiki 相同：
//! ```text
//! /charts 目錄頁 → 本機目錄（24 小時快取）→ 搜尋只查本機
//! 選中難度 → /adxcs/{slug}/maidata.txt → 只留該難度 → 既有 parser 驗證
//! ```

use super::catalog::{
    inote_slot, is_valid_slug, parse_catalog_html, AdxSong, MIN_CATALOG_SONGS, SITE_ROOT,
};
use super::maidata;
use crate::wiki::{cache_base, ChartType, Difficulty};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

const CATALOG_TTL_SECONDS: u64 = 24 * 3600;
/// 目錄頁約 2.5 MB（2026-10）；留足空間但仍設上限。
const CATALOG_LIMIT_BYTES: usize = 24 * 1024 * 1024;
const CHART_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ATTEMPTS: u32 = 3;
const CHART_REQUEST_SPACING: Duration = Duration::from_millis(800);
const CATALOG_FILE: &str = "catalog.json";

pub fn build_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        // 目錄頁較大，慢速網路要多給一點時間。
        .timeout(Duration::from_secs(40))
        .https_only(true)
        .user_agent(concat!("MaiMotionDemo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("無法建立 adxdls 網路連線：{error}"))
}

pub struct AdxState {
    client: Client,
    songs: RwLock<Vec<AdxSong>>,
    fetched_at: RwLock<Option<u64>>,
    last_chart_fetch: Mutex<Option<Instant>>,
}

impl AdxState {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            songs: RwLock::new(Vec::new()),
            fetched_at: RwLock::new(None),
            last_chart_fetch: Mutex::new(None),
        }
    }
}

/// 目錄載入結果。歌曲本身留在 Rust，前端只需要筆數與時間（搜尋走 chart_search）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdxIndexPayload {
    pub count: usize,
    pub fetched_at: u64,
    pub from_cache: bool,
    /// adxdls 抓取失敗，回傳的是過期快取。
    pub stale: bool,
}

/// 譜面載入結果：chart_text 只含選定難度，已通過既有 parser 驗證。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdxChartPayload {
    pub chart_text: String,
    pub title: String,
    pub artist: Option<String>,
    pub bpm: Option<f64>,
    pub level: Option<String>,
    pub designer: Option<String>,
    pub page_url: String,
    pub slug: String,
    pub difficulty: Difficulty,
    pub chart_type: ChartType,
}

#[derive(Serialize, Deserialize)]
struct CatalogCacheFile {
    fetched_at: u64,
    songs: Vec<AdxSong>,
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 快取放在 Wiki 快取旁的 `adx/`；設定頁的「清除快取」會一起清。
pub fn cache_dir(base: &Path) -> PathBuf {
    base.join("adx")
}

fn catalog_path(base: &Path) -> PathBuf {
    cache_dir(base).join(CATALOG_FILE)
}

fn chart_path(base: &Path, slug: &str) -> PathBuf {
    cache_dir(base).join("charts").join(format!("{slug}.txt"))
}

fn load_catalog(path: &Path) -> Option<CatalogCacheFile> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("無法建立 adxdls 快取目錄：{error}"))?;
    }
    std::fs::write(path, bytes).map_err(|error| format!("adxdls 快取寫入失敗：{error}"))
}

fn store_memory(state: &AdxState, songs: Vec<AdxSong>, fetched_at: u64) {
    if let Ok(mut memory) = state.songs.write() {
        *memory = songs;
    }
    if let Ok(mut at) = state.fetched_at.write() {
        *at = Some(fetched_at);
    }
}

fn memory_payload(state: &AdxState) -> Option<AdxIndexPayload> {
    let songs = state.songs.read().ok()?;
    if songs.is_empty() {
        return None;
    }
    Some(AdxIndexPayload {
        count: songs.len(),
        fetched_at: state.fetched_at.read().ok()?.unwrap_or(0),
        from_cache: false,
        stale: false,
    })
}

/// 清掉記憶體中的目錄（設定頁清除快取時呼叫；磁碟由呼叫端一起刪）。
pub fn clear_memory(state: &AdxState) {
    store_memory(state, Vec::new(), 0);
    if let Ok(mut at) = state.fetched_at.write() {
        *at = None;
    }
}

/// 載入目錄：記憶體 → 新鮮快取 → 網路 → 過期快取。
pub async fn refresh_index(
    app: &AppHandle,
    state: &AdxState,
    force: bool,
) -> Result<AdxIndexPayload, String> {
    if !force {
        if let Some(payload) = memory_payload(state) {
            return Ok(payload);
        }
    }
    let path = catalog_path(&cache_base(app));
    if !force {
        if let Some(cached) = load_catalog(&path) {
            if now_unix_seconds().saturating_sub(cached.fetched_at) <= CATALOG_TTL_SECONDS
                && !cached.songs.is_empty()
            {
                let payload = AdxIndexPayload {
                    count: cached.songs.len(),
                    fetched_at: cached.fetched_at,
                    from_cache: true,
                    stale: false,
                };
                store_memory(state, cached.songs, cached.fetched_at);
                return Ok(payload);
            }
        }
    }
    let fresh = async {
        let html = get_text(
            &state.client,
            &format!("{SITE_ROOT}/charts"),
            CATALOG_LIMIT_BYTES,
        )
        .await?;
        let songs = parse_catalog_html(&html)?;
        if songs.len() < MIN_CATALOG_SONGS {
            return Err("adxdls 目錄歌曲數異常偏低，版面可能已變更，已保留舊快取".to_string());
        }
        Ok(songs)
    };
    match fresh.await {
        Ok(songs) => {
            let fetched_at = now_unix_seconds();
            let file = CatalogCacheFile { fetched_at, songs };
            match serde_json::to_vec(&file) {
                Ok(bytes) => {
                    if let Err(error) = write_file(&path, &bytes) {
                        eprintln!("{error}");
                    }
                }
                Err(error) => eprintln!("adxdls catalog cache encode failed: {error}"),
            }
            let payload = AdxIndexPayload {
                count: file.songs.len(),
                fetched_at,
                from_cache: false,
                stale: false,
            };
            store_memory(state, file.songs, fetched_at);
            Ok(payload)
        }
        Err(error) => match load_catalog(&path).filter(|cached| !cached.songs.is_empty()) {
            // 網站掛掉時用舊目錄保底。
            Some(cached) => {
                let payload = AdxIndexPayload {
                    count: cached.songs.len(),
                    fetched_at: cached.fetched_at,
                    from_cache: true,
                    stale: true,
                };
                store_memory(state, cached.songs, cached.fetched_at);
                Ok(payload)
            }
            None => Err(error),
        },
    }
}

/// 合併搜尋用：在已載入的目錄上執行 `read`；記憶體沒有就讀磁碟快取（過期也可），仍沒有回傳 None。
pub fn with_songs<R>(
    app: &AppHandle,
    state: &AdxState,
    read: impl FnOnce(&[AdxSong]) -> R,
) -> Option<R> {
    let empty = state
        .songs
        .read()
        .map(|songs| songs.is_empty())
        .unwrap_or(true);
    if empty {
        let cached = load_catalog(&catalog_path(&cache_base(app)))?;
        if cached.songs.is_empty() {
            return None;
        }
        store_memory(state, cached.songs, cached.fetched_at);
    }
    let songs = state.songs.read().ok()?;
    Some(read(&songs))
}

/// 下載 maidata.txt，取出指定難度並用既有 parser 驗證。
pub async fn fetch_chart(
    app: &AppHandle,
    state: &AdxState,
    slug: &str,
    difficulty: Difficulty,
) -> Result<AdxChartPayload, String> {
    if !is_valid_slug(slug) {
        return Err("adxdls 譜面代號格式無效".into());
    }
    let song = with_songs(app, state, |songs| {
        songs.iter().find(|song| song.slug == slug).cloned()
    })
    .flatten()
    .ok_or_else(|| "adxdls 目錄裡找不到這首歌，請重新整理索引".to_string())?;
    let base = cache_base(app);
    let path = chart_path(&base, slug);
    let text = match std::fs::read(&path)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        Some(cached) => cached,
        None => {
            space_chart_requests(state);
            let fresh = get_text(
                &state.client,
                &format!("{SITE_ROOT}/adxcs/{slug}/maidata.txt"),
                CHART_LIMIT_BYTES,
            )
            .await?;
            if let Err(error) = write_file(&path, fresh.as_bytes()) {
                eprintln!("{error}");
            }
            fresh
        }
    };
    let slot = inote_slot(difficulty);
    let chart_text = maidata::single_chart(&text, slot)
        .ok_or_else(|| format!("adxdls 這首歌沒有 {} 的譜面", difficulty.section_heading()))?;
    if let Err(diagnostic) = mai_motion_core::parse_chart(&chart_text, 0.0) {
        // 位置換回原始 maidata.txt 的行號，對照網站上的檔案比較好找。
        let place = diagnostic
            .source_span
            .as_deref()
            .and_then(|span| {
                let shift = maidata::inote_line(&text, slot)?;
                let line = span
                    .line
                    .checked_sub(maidata::inote_line(&chart_text, slot)?)?
                    + shift;
                Some(format!("maidata.txt 第 {line} 行第 {} 字", span.column))
            })
            .unwrap_or_else(|| diagnostic.code.clone());
        return Err(format!(
            "adxdls 譜面未通過 simai 解析（{place}）：{}",
            diagnostic.message
        ));
    }
    let fields = maidata::fields(&text);
    let info = song.difficulties.get(&difficulty);
    Ok(AdxChartPayload {
        chart_text,
        title: song.title.clone(),
        artist: Some(song.artist.clone()).filter(|artist| !artist.is_empty()),
        bpm: song
            .bpm
            .or_else(|| maidata::field(&fields, "wholebpm").and_then(|bpm| bpm.parse().ok())),
        level: info
            .and_then(|info| info.level.clone())
            .or_else(|| maidata::field(&fields, &format!("lv_{slot}")).map(String::from)),
        designer: info.and_then(|info| info.designer.clone()),
        page_url: song.page_url.clone(),
        slug: song.slug.clone(),
        difficulty,
        chart_type: song.chart_type,
    })
}

/// 未快取的譜面請求之間保留基本間隔；跑在 Tauri 背景工作，不阻塞 UI。
fn space_chart_requests(state: &AdxState) {
    let wait = match state.last_chart_fetch.lock() {
        Ok(mut last) => {
            let now = Instant::now();
            let wait = last.and_then(|previous| CHART_REQUEST_SPACING.checked_sub(now - previous));
            *last = Some(now + wait.unwrap_or(Duration::ZERO));
            wait
        }
        Err(_) => None,
    };
    if let Some(wait) = wait {
        std::thread::sleep(wait);
    }
}

/// 只允許 adxdls 的目錄頁與譜面檔。
fn is_allowed_url(url: &str) -> bool {
    let Some(path) = url.strip_prefix(SITE_ROOT) else {
        return false;
    };
    if path == "/charts" {
        return true;
    }
    path.strip_prefix("/adxcs/")
        .and_then(|rest| rest.strip_suffix("/maidata.txt"))
        .is_some_and(is_valid_slug)
}

/// GET 文字；429／5xx／逾時最多重試 2 次。
async fn get_text(client: &Client, url: &str, limit: usize) -> Result<String, String> {
    if !is_allowed_url(url) {
        return Err("只允許讀取 adxdls 的目錄與譜面檔".into());
    }
    let mut attempt = 0;
    loop {
        attempt += 1;
        match request_once(client, url, limit).await {
            Ok(text) => return Ok(text),
            Err((message, retryable)) => {
                if attempt >= MAX_ATTEMPTS || !retryable {
                    return Err(message);
                }
            }
        }
    }
}

async fn request_once(client: &Client, url: &str, limit: usize) -> Result<String, (String, bool)> {
    let mut response = client.get(url).send().await.map_err(|error| {
        if error.is_timeout() {
            ("連線 adxdls 逾時，請重試或稍後再試".to_string(), true)
        } else if error.is_connect() {
            (
                "無法連線 adxdls，請檢查網路；防毒軟體若攔截此網站，需要先放行".to_string(),
                true,
            )
        } else {
            (format!("adxdls 網路請求失敗：{error}"), false)
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status {
            StatusCode::NOT_FOUND => ("adxdls 找不到這個檔案，可能已被移除".to_string(), false),
            StatusCode::TOO_MANY_REQUESTS => ("adxdls 請求過於頻繁，請稍後再試".to_string(), true),
            _ if status.is_server_error() => (
                format!("adxdls 服務暫時異常（HTTP {status}），請稍後再試"),
                true,
            ),
            _ => (format!("adxdls 請求失敗（HTTP {status}）"), false),
        });
    }
    let too_large = || {
        (
            format!("adxdls 回應超過 {} MiB 上限", limit / 1024 / 1024),
            false,
        )
    };
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| (format!("讀取 adxdls 回應失敗：{error}"), true))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    let text =
        String::from_utf8(bytes).map_err(|_| ("adxdls 回應的編碼無法解析".to_string(), false))?;
    Ok(text
        .strip_prefix('\u{feff}')
        .map(String::from)
        .unwrap_or(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_allowlist() {
        assert!(is_allowed_url("https://adxdls.saop.cc/charts"));
        assert!(is_allowed_url(
            "https://adxdls.saop.cc/adxcs/12061/maidata.txt"
        ));
        assert!(!is_allowed_url(
            "https://adxdls.saop.cc/adxcs/../maidata.txt"
        ));
        assert!(!is_allowed_url(
            "https://adxdls.saop.cc/adxcs/a/b/maidata.txt"
        ));
        assert!(!is_allowed_url("https://adxdls.saop.cc/api/charts"));
        assert!(!is_allowed_url(
            "https://adxdls.saop.cc.evil.example/charts"
        ));
        assert!(!is_allowed_url("http://adxdls.saop.cc/charts"));
    }
}
