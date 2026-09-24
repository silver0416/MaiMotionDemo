use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/silver0416/MaiMotionDemo/releases/latest";
const RESPONSE_LIMIT_BYTES: usize = 512 * 1024;

/// 前端更新提示用的版本資訊（camelCase，與其他 command 一致）。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub url: String,
    pub notes: String,
    pub published_at: String,
    pub distribution: String,
    /// 這個平台可以在程式內下載的執行檔；沒有就只能到 GitHub 手動下載。
    pub asset: Option<UpdateAsset>,
}

/// Release 附件：Portable 單一執行檔。
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// GitHub 提供的 SHA-256（十六進位）；舊版 Release 可能沒有。
    pub sha256: Option<String>,
}

/// 只接受本專案 Release 的下載網址，避免被導去別處下載執行檔。
pub const ASSET_URL_PREFIX: &str = "https://github.com/silver0416/MaiMotionDemo/releases/download/";

/// 從 Release 的附件中找出這一版的 Windows x64 執行檔。
pub fn pick_asset(release: &serde_json::Value, version: &str) -> Option<UpdateAsset> {
    if !cfg!(windows) {
        return None;
    }
    let assets = release.get("assets")?.as_array()?;
    let expected = format!("MaiMotionDemo-v{version}-windows-x64.exe");
    let parse = |asset: &serde_json::Value| -> Option<UpdateAsset> {
        let name = asset.get("name")?.as_str()?.to_string();
        let url = asset.get("browser_download_url")?.as_str()?.to_string();
        let size = asset.get("size")?.as_u64()?;
        if !url.starts_with(ASSET_URL_PREFIX) || size == 0 || name.contains(['/', '\\']) {
            return None;
        }
        let sha256 = asset
            .get("digest")
            .and_then(|value| value.as_str())
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(|hex| hex.to_ascii_lowercase());
        Some(UpdateAsset { name, url, size, sha256 })
    };
    let candidates: Vec<UpdateAsset> = assets.iter().filter_map(parse).collect();
    candidates
        .iter()
        .find(|asset| asset.name == expected)
        .or_else(|| candidates.iter().find(|asset| asset.name.ends_with("-windows-x64.exe")))
        .cloned()
}

/// 目前這份建置的發佈通路。
/// - `dev`：debug 建置，不做更新提示
/// - `portable`：免安裝單一 exe（目前預設），只能導向 GitHub 下載
/// - `installed`：未來的安裝版，保留給 updater 全自動更新用
pub fn distribution() -> &'static str {
    if cfg!(debug_assertions) {
        return "dev";
    }
    match option_env!("MAIMOTION_DISTRIBUTION") {
        Some("installed") => "installed",
        // 未知旗標一律視為 portable，走手動下載流程，不會誤進 updater。
        _ => "portable",
    }
}

pub fn build_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .https_only(true)
        .user_agent(concat!("MaiMotionDemo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("無法建立更新檢查的網路連線：{error}"))
}

pub async fn check(client: &Client) -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let response = client
        .get(RELEASES_LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(map_request_error)?;
    let response = require_success(response).await?;
    let bytes = read_limited(response).await?;
    let release: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("GitHub 版本資訊格式無法解析：{error}"))?;
    let latest_raw = release
        .get("tag_name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    if latest_raw.is_empty() {
        return Err("GitHub 回傳的版本資訊無效（缺少 tag_name）".into());
    }
    let latest = normalize_version(latest_raw);
    let current_normalized = normalize_version(&current);
    if latest.is_empty() {
        return Err(format!("GitHub 版本標籤無法辨識：{latest_raw}"));
    }
    let url = release
        .get("html_url")
        .and_then(|value| value.as_str())
        .unwrap_or("https://github.com/silver0416/MaiMotionDemo/releases")
        .to_string();
    let mut notes = release
        .get("body")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    if notes.chars().count() > 2000 {
        notes = format!("{}…", notes.chars().take(2000).collect::<String>());
    }
    let published_at = release
        .get("published_at")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let asset = pick_asset(&release, &latest);
    Ok(UpdateInfo {
        asset,
        current: current.clone(),
        latest: latest.clone(),
        has_update: is_newer(&current_normalized, &latest),
        url,
        notes,
        published_at,
        distribution: distribution().to_string(),
    })
}

/// 去掉前導 `v`、空白與 `-beta`／`+build` 後綴，只留 `x.y.z` 核心。
fn normalize_version(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw.strip_prefix('v').or_else(|| raw.strip_prefix('V')).unwrap_or(raw);
    let core = raw.split(['-', '+']).next().unwrap_or(raw);
    core.trim().to_string()
}

/// 只有 latest 的數字分段大於 current 才算有新版；相等或無法比較都不提示。
fn is_newer(current: &str, latest: &str) -> bool {
    let current_parts = parse_parts(current);
    let latest_parts = parse_parts(latest);
    if current_parts.is_empty() || latest_parts.is_empty() {
        return false;
    }
    let width = current_parts.len().max(latest_parts.len());
    for index in 0..width {
        let current_part = current_parts.get(index).copied().unwrap_or(0);
        let latest_part = latest_parts.get(index).copied().unwrap_or(0);
        if latest_part != current_part {
            return latest_part > current_part;
        }
    }
    false
}

/// `1.2.x` 這類非數字段視為 0，不讓版本檢查直接報錯。
fn parse_parts(version: &str) -> Vec<u64> {
    let version = version.trim();
    if version.is_empty() {
        return Vec::new();
    }
    version
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(|char| char.is_ascii_digit()).collect();
            digits.parse::<u64>().unwrap_or(0)
        })
        .collect()
}

fn map_request_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "檢查更新逾時，請稍後再試".into()
    } else if error.is_connect() {
        "無法連線 GitHub，請檢查網路後再試".into()
    } else {
        format!("檢查更新的網路請求失敗：{error}")
    }
}

async fn require_success(response: reqwest::Response) -> Result<reqwest::Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(match status {
        StatusCode::NOT_FOUND => {
            "GitHub 上還沒有任何正式版本（404），請稍後再試".into()
        }
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            "GitHub API 請求過於頻繁（限流），請稍後再試".into()
        }
        _ if status.is_server_error() => {
            format!("GitHub 服務暫時異常（HTTP {status}），請稍後再試")
        }
        _ => format!("檢查更新失敗（HTTP {status}）"),
    })
}

async fn read_limited(response: reqwest::Response) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > RESPONSE_LIMIT_BYTES as u64)
    {
        return Err("GitHub 版本資訊超過 512 KiB 上限".into());
    }
    let mut bytes = Vec::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("讀取 GitHub 版本資訊失敗：{error}"))?
    {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_LIMIT_BYTES {
            return Err("GitHub 版本資訊超過 512 KiB 上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tags() {
        assert_eq!(normalize_version("v0.3.2"), "0.3.2");
        assert_eq!(normalize_version(" V1.0 "), "1.0");
        assert_eq!(normalize_version("v0.4.0-beta.1"), "0.4.0");
        assert_eq!(normalize_version("0.3.2+build.5"), "0.3.2");
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.3.2", "0.3.3"));
        assert!(is_newer("0.3.2", "0.4.0"));
        assert!(is_newer("0.3.2", "1.0.0"));
        assert!(!is_newer("0.3.2", "0.3.2"));
        assert!(!is_newer("0.4.0", "0.3.9"));
        assert!(!is_newer("0.3.2", "0.3"));
        assert!(is_newer("0.3", "0.3.1"));
    }

    #[test]
    fn ignores_unparseable_versions() {
        assert!(!is_newer("", "0.3.3"));
        assert!(!is_newer("0.3.2", ""));
    }

    #[test]
    fn picks_this_versions_windows_asset() {
        let release = serde_json::json!({
            "assets": [
                {"name": "notes.txt", "browser_download_url": "https://github.com/silver0416/MaiMotionDemo/releases/download/v0.4.1/notes.txt", "size": 3},
                {"name": "MaiMotionDemo-v0.4.1-windows-x64.exe", "size": 100,
                 "browser_download_url": "https://github.com/silver0416/MaiMotionDemo/releases/download/v0.4.1/MaiMotionDemo-v0.4.1-windows-x64.exe",
                 "digest": "sha256:8AC1AFE1A1F0A593BE08E2E2FA310F0B7B8238EDAF0EA5EE5445FFED5663ED8C"},
                {"name": "evil-windows-x64.exe", "size": 100, "browser_download_url": "https://example.com/evil-windows-x64.exe"}
            ]
        });
        let asset = pick_asset(&release, "0.4.1");
        if cfg!(windows) {
            let asset = asset.unwrap();
            assert_eq!(asset.name, "MaiMotionDemo-v0.4.1-windows-x64.exe");
            assert_eq!(asset.size, 100);
            assert_eq!(asset.sha256.as_deref(), Some("8ac1afe1a1f0a593be08e2e2fa310f0b7b8238edaf0ea5ee5445ffed5663ed8c"));
            // 其他網域的附件一律不接受。
            let only_evil = serde_json::json!({"assets": [release["assets"][2].clone()]});
            assert!(pick_asset(&only_evil, "0.4.1").is_none());
        } else {
            assert!(asset.is_none());
        }
    }

    #[test]
    fn parses_release_shape() {
        let json = br#"{"tag_name":"v0.3.3","html_url":"https://github.com/silver0416/MaiMotionDemo/releases/tag/v0.3.3","body":"notes","published_at":"2026-09-18T00:00:00Z"}"#;
        let release: serde_json::Value = serde_json::from_slice(json).unwrap();
        assert_eq!(
            release.get("tag_name").and_then(|value| value.as_str()),
            Some("v0.3.3")
        );
        assert_eq!(normalize_version("v0.3.3"), "0.3.3");
        assert!(is_newer("0.3.2", "0.3.3"));
    }
}
