//! simai Wiki HTTP client：只讀取公開頁面，不使用編輯 API。
//!
//! - User-Agent 為 `MaiMotionDemo/<版本>`，不偽裝瀏覽器。
//! - 429/5xx/timeout 最多 retry 2 次，不無限重試。
//! - 一次只抓一個頁面；正常使用只有「Standard index＋DX index＋選中的歌曲頁」。

use super::model::WikiError;
use reqwest::{Client, StatusCode};
use std::time::Duration;

pub const STANDARD_INDEX_URL: &str = "https://w.atwiki.jp/simai/pages/32.html";
pub const DELUXE_INDEX_URL: &str = "https://w.atwiki.jp/simai/pages/808.html";

const PAGE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MAX_ATTEMPTS: u32 = 3;
const MAX_PAGE_ID: u32 = 9_999_999;

pub fn build_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .https_only(true)
        .user_agent(concat!("MaiMotionDemo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("無法建立 simai Wiki 網路連線：{error}"))
}

pub async fn fetch_standard_index_html(client: &Client) -> Result<String, WikiError> {
    get_text(client, STANDARD_INDEX_URL).await
}

pub async fn fetch_deluxe_index_html(client: &Client) -> Result<String, WikiError> {
    get_text(client, DELUXE_INDEX_URL).await
}

/// 下載單一歌曲頁。page_id 範圍外或 URL 不在 allowlist 直接拒絕，
/// 不追 editx/list/站外連結。
pub async fn fetch_song_html(client: &Client, page_id: u32) -> Result<String, WikiError> {
    if page_id == 0 || page_id > MAX_PAGE_ID {
        return Err(WikiError::InvalidParams("Wiki page id 格式無效".into()));
    }
    let url = format!("https://w.atwiki.jp/simai/pages/{page_id}.html");
    get_text(client, &url).await
}

async fn get_text(client: &Client, url: &str) -> Result<String, WikiError> {
    if !is_allowed_url(url) {
        return Err(WikiError::InvalidParams(
            "只允許讀取 simai Wiki 的公開譜面頁".into(),
        ));
    }
    let mut attempt = 0;
    loop {
        attempt += 1;
        match request_once(client, url).await {
            Ok(text) => return Ok(text),
            Err(error) => {
                if attempt >= MAX_ATTEMPTS || !is_retryable(&error) {
                    return Err(error);
                }
            }
        }
    }
}

fn is_allowed_url(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://w.atwiki.jp/simai/pages/") {
        if let Some((id_part, _)) = rest.split_once(".html") {
            let after = &rest[id_part.len() + ".html".len()..];
            return !id_part.is_empty()
                && id_part.bytes().all(|b| b.is_ascii_digit())
                && (after.is_empty() || after.starts_with('#') || after.starts_with('?'));
        }
    }
    false
}

fn is_retryable(error: &WikiError) -> bool {
    matches!(error, WikiError::Network(message) if message.contains("重試"))
}

async fn request_once(client: &Client, url: &str) -> Result<String, WikiError> {
    let response = client.get(url).send().await.map_err(|error| {
        if error.is_timeout() {
            WikiError::Network("連線 simai Wiki 逾時，請重試或稍後再試".into())
        } else if error.is_connect() {
            WikiError::Network("無法連線 simai Wiki，請重試或檢查網路後再試".into())
        } else {
            WikiError::Network(format!("simai Wiki 網路請求失敗，請重試：{error}"))
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status {
            StatusCode::NOT_FOUND => {
                WikiError::Network("simai Wiki 找不到這個頁面，可能已被移動".into())
            }
            StatusCode::TOO_MANY_REQUESTS => {
                WikiError::Network("simai Wiki 請求過於頻繁，請重試或稍後再試".into())
            }
            _ if status.is_server_error() => WikiError::Network(format!(
                "simai Wiki 服務暫時異常（HTTP {status}），請重試或稍後再試"
            )),
            _ => WikiError::Network(format!("simai Wiki 請求失敗（HTTP {status}）")),
        });
    }
    let bytes = read_limited(response).await?;
    String::from_utf8(bytes)
        .map(|text| text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
        .map_err(|_| WikiError::Network("simai Wiki 頁面編碼無法解析".into()))
}

async fn read_limited(mut response: reqwest::Response) -> Result<Vec<u8>, WikiError> {
    if response
        .content_length()
        .is_some_and(|length| length > PAGE_LIMIT_BYTES as u64)
    {
        return Err(WikiError::Network("simai Wiki 頁面超過 4 MiB 上限".into()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| WikiError::Network(format!("讀取 simai Wiki 頁面失敗：{error}")))?
    {
        if bytes.len().saturating_add(chunk.len()) > PAGE_LIMIT_BYTES {
            return Err(WikiError::Network("simai Wiki 頁面超過 4 MiB 上限".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
