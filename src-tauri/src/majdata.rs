use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const API_ROOT: &str = "https://majdata.net/api3/api";
const SEARCH_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const CHART_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MAX_QUERY_CHARS: usize = 100;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MajdataChartSummary {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub designer: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub levels: Vec<Option<String>>,
    #[serde(default)]
    pub uploader: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub public_tags: Vec<String>,
}

pub fn build_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .https_only(true)
        .user_agent(concat!("MaiMotionDemo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("無法建立 Majdata 網路連線：{error}"))
}

pub async fn search(client: &Client, query: String) -> Result<Vec<MajdataChartSummary>, String> {
    let query = validate_query(&query)?;
    let mut url = Url::parse(&format!("{API_ROOT}/maichart/list"))
        .map_err(|error| format!("Majdata 搜尋網址無效：{error}"))?;
    url.query_pairs_mut().append_pair("search", query);

    let response = client.get(url).send().await.map_err(map_request_error)?;
    let response = require_success(response).await?;
    let bytes = read_limited(response, SEARCH_LIMIT_BYTES, "搜尋結果").await?;
    serde_json::from_slice(&bytes).map_err(|error| format!("Majdata 搜尋結果格式無法解析：{error}"))
}

pub async fn fetch_chart(client: &Client, song_id: String) -> Result<String, String> {
    let song_id = validate_song_id(&song_id)?;
    let url = Url::parse(&format!("{API_ROOT}/maichart/{song_id}/chart"))
        .map_err(|error| format!("Majdata 譜面網址無效：{error}"))?;

    let response = client.get(url).send().await.map_err(map_request_error)?;
    let response = require_success(response).await?;
    let bytes = read_limited(response, CHART_LIMIT_BYTES, "譜面檔案").await?;
    let source = String::from_utf8(bytes)
        .map_err(|_| "Majdata 譜面不是有效的 UTF-8 文字，無法匯入".to_string())?;
    let source = source
        .strip_prefix('\u{feff}')
        .unwrap_or(&source)
        .to_string();
    if source.trim().is_empty() {
        return Err("Majdata 回傳了空白譜面，無法匯入".into());
    }
    Ok(source)
}

fn validate_query(query: &str) -> Result<&str, String> {
    let query = query.trim();
    let count = query.chars().count();
    if count == 0 {
        return Err("請輸入要搜尋的歌曲名稱、作者或譜師".into());
    }
    if count > MAX_QUERY_CHARS {
        return Err(format!("搜尋文字不可超過 {MAX_QUERY_CHARS} 個字元"));
    }
    Ok(query)
}

fn validate_song_id(song_id: &str) -> Result<&str, String> {
    let song_id = song_id.trim();
    if song_id.is_empty()
        || song_id.len() > 128
        || !song_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("Majdata song id 格式無效".into());
    }
    Ok(song_id)
}

fn map_request_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "連線 Majdata 逾時，請稍後再試".into()
    } else if error.is_connect() {
        "無法連線 Majdata，請檢查網路後再試".into()
    } else {
        format!("Majdata 網路請求失敗：{error}")
    }
}

async fn require_success(response: reqwest::Response) -> Result<reqwest::Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(match status {
        StatusCode::NOT_FOUND => "Majdata 找不到這份譜面，可能已被移除".into(),
        StatusCode::TOO_MANY_REQUESTS => "Majdata 請求過於頻繁，請稍後再試".into(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            "Majdata 拒絕了這次請求，公開存取規則可能已變更".into()
        }
        _ if status.is_server_error() => {
            format!("Majdata 服務暫時異常（HTTP {status}），請稍後再試")
        }
        _ => format!("Majdata 請求失敗（HTTP {status}）"),
    })
}

async fn read_limited(
    mut response: reqwest::Response,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(format!(
            "Majdata {label}超過 {} MiB 上限",
            limit / 1024 / 1024
        ));
    }
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .map_or(0, |length| length as usize),
    );
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("讀取 Majdata {label}失敗：{error}"))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(format!(
                "Majdata {label}超過 {} MiB 上限",
                limit / 1024 / 1024
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_query_without_changing_unicode() {
        assert_eq!(validate_query("  削除  "), Ok("削除"));
        assert!(validate_query("   ").is_err());
        assert!(validate_query(&"譜".repeat(MAX_QUERY_CHARS + 1)).is_err());
    }

    #[test]
    fn rejects_song_id_path_injection() {
        assert_eq!(
            validate_song_id("0544d740-ee9c-45a8-b345-ef489decdcd4"),
            Ok("0544d740-ee9c-45a8-b345-ef489decdcd4")
        );
        assert!(validate_song_id("../account/info").is_err());
        assert!(validate_song_id("id?fullImage=true").is_err());
    }

    #[test]
    fn parses_current_search_shape_and_tolerates_null_levels() {
        let json = br#"[{"id":"chart-1","title":"Song","artist":"Artist","designer":"Designer","levels":[null,"","13+"],"uploader":"User","timestamp":"2026-09-16T00:00:00Z","tags":["tag"],"publicTags":[]}]"#;
        let charts: Vec<MajdataChartSummary> = serde_json::from_slice(json).unwrap();
        assert_eq!(charts[0].id, "chart-1");
        assert_eq!(
            charts[0].levels,
            vec![None, Some(String::new()), Some("13+".into())]
        );
        assert!(charts[0].description.is_empty());
    }
}
