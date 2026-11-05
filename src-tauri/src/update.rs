use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::Duration;

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/silver0416/MaiMotionDemo/releases/latest";
const RESPONSE_LIMIT_BYTES: usize = 512 * 1024;
const RELEASES_LIST_URL: &str = "https://api.github.com/repos/silver0416/MaiMotionDemo/releases";
/// 更新內容：一頁 50 版，最多翻 4 頁。
const RELEASE_PAGE_SIZE: usize = 50;
const RELEASE_PAGES: usize = 4;
const RELEASE_LIST_LIMIT_BYTES: usize = 4 * 1024 * 1024;
/// Release 頁的網址一律在這底下，前端才會用瀏覽器開啟。
const RELEASE_PAGE_PREFIX: &str = "https://github.com/silver0416/MaiMotionDemo/releases";

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

/// 一版的發布說明，內容與 GitHub Release 頁相同（Markdown）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNote {
    /// `1.1.2`（沒有前導 v）
    pub version: String,
    /// Release 標題；沒有標題時用 tag。
    pub name: String,
    pub body: String,
    pub url: String,
    pub published_at: String,
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
        Some(UpdateAsset {
            name,
            url,
            size,
            sha256,
        })
    };
    let candidates: Vec<UpdateAsset> = assets.iter().filter_map(parse).collect();
    candidates
        .iter()
        .find(|asset| asset.name == expected)
        .or_else(|| {
            candidates
                .iter()
                .find(|asset| asset.name.ends_with("-windows-x64.exe"))
        })
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
        .map_err(|error| map_request_error(error, "檢查更新"))?;
    let response = require_success(response, "檢查更新").await?;
    let bytes = read_limited(response, RESPONSE_LIMIT_BYTES).await?;
    let release: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("GitHub 版本資訊格式無法解析：{error}"))?;
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

/// 從 `from`（不含）到 `to`（含）每一版的發布說明，新版在前。
/// 不知道 `from`、或 `from` 不比 `to` 舊時，只回傳 `to` 這一版。
pub async fn release_notes(
    client: &Client,
    from: Option<&str>,
    to: &str,
) -> Result<Vec<ReleaseNote>, String> {
    const ACTION: &str = "讀取更新內容";
    let lowest = release_range(from, to).0.unwrap_or_else(|| normalize_version(to));
    let mut releases: Vec<serde_json::Value> = Vec::new();
    for page in 1..=RELEASE_PAGES {
        let response = client
            .get(format!(
                "{RELEASES_LIST_URL}?per_page={RELEASE_PAGE_SIZE}&page={page}"
            ))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|error| map_request_error(error, ACTION))?;
        let response = require_success(response, ACTION).await?;
        let bytes = read_limited(response, RELEASE_LIST_LIMIT_BYTES).await?;
        let list: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
            .map_err(|error| format!("GitHub 版本清單格式無法解析：{error}"))?;
        let last_page = list.len() < RELEASE_PAGE_SIZE;
        // 清單由新到舊；這一頁已經有範圍下緣（或更舊）的版本，就不用再往後翻。
        let reached = list.iter().any(|release| {
            release_version(release)
                .is_some_and(|version| version_order(&version, &lowest) != Ordering::Greater)
        });
        releases.extend(list);
        if last_page || reached {
            break;
        }
    }
    let notes = pick_release_notes(&releases, from, to);
    if notes.is_empty() {
        return Err(format!(
            "GitHub 上還沒有 v{} 的發布說明",
            normalize_version(to)
        ));
    }
    Ok(notes)
}

/// 範圍的兩端（都已去掉前導 v）：舊版不知道、無法辨識或不比目前舊時是 None。
fn release_range(from: Option<&str>, to: &str) -> (Option<String>, String) {
    let to = normalize_version(to);
    let from = from
        .map(normalize_version)
        .filter(|from| is_version(from) && version_order(from, &to) == Ordering::Less);
    (from, to)
}

/// 正式版的版本號；草稿、預先發布與無法辨識的 tag 是 None。
fn release_version(release: &serde_json::Value) -> Option<String> {
    let flag = |key: &str| {
        release
            .get(key)
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    };
    if flag("draft") || flag("prerelease") {
        return None;
    }
    let version = normalize_version(release.get("tag_name")?.as_str()?);
    is_version(&version).then_some(version)
}

/// `1`、`1.2`、`1.2.3` 這種每段都是數字的版本號（`nightly`、`1.2.x` 不算）。
fn is_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// 從 GitHub 的 Release 清單挑出範圍內的版本，新版在前。
fn pick_release_notes(
    releases: &[serde_json::Value],
    from: Option<&str>,
    to: &str,
) -> Vec<ReleaseNote> {
    let (from, to) = release_range(from, to);
    let mut notes: Vec<ReleaseNote> = releases
        .iter()
        .filter_map(|release| {
            let version = release_version(release)?;
            let in_range = version_order(&version, &to) != Ordering::Greater
                && match &from {
                    Some(from) => version_order(&version, from) == Ordering::Greater,
                    None => version_order(&version, &to) == Ordering::Equal,
                };
            if !in_range {
                return None;
            }
            let text = |key: &str| {
                release
                    .get(key)
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let name = text("name");
            let url = text("html_url");
            Some(ReleaseNote {
                name: if name.trim().is_empty() {
                    text("tag_name")
                } else {
                    name
                },
                body: text("body"),
                url: if url.starts_with(&format!("{RELEASE_PAGE_PREFIX}/")) {
                    url
                } else {
                    RELEASE_PAGE_PREFIX.to_string()
                },
                published_at: text("published_at"),
                version,
            })
        })
        .collect();
    notes.sort_by(|a, b| version_order(&b.version, &a.version));
    notes.dedup_by(|a, b| version_order(&a.version, &b.version) == Ordering::Equal);
    notes
}

/// 數字分段逐段比較，缺的段當 0（`1.1` 與 `1.1.0` 相同）。
fn version_order(a: &str, b: &str) -> Ordering {
    let a = parse_parts(a);
    let b = parse_parts(b);
    (0..a.len().max(b.len()))
        .map(|index| {
            let left = a.get(index).copied().unwrap_or(0);
            let right = b.get(index).copied().unwrap_or(0);
            left.cmp(&right)
        })
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

/// 去掉前導 `v`、空白與 `-beta`／`+build` 後綴，只留 `x.y.z` 核心。
fn normalize_version(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix('v')
        .or_else(|| raw.strip_prefix('V'))
        .unwrap_or(raw);
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
            let digits: String = part
                .chars()
                .take_while(|char| char.is_ascii_digit())
                .collect();
            digits.parse::<u64>().unwrap_or(0)
        })
        .collect()
}

/// `action`：錯誤訊息開頭的動作，例如「檢查更新」「讀取更新內容」。
fn map_request_error(error: reqwest::Error, action: &str) -> String {
    if error.is_timeout() {
        format!("{action}逾時，請稍後再試")
    } else if error.is_connect() {
        "無法連線 GitHub，請檢查網路後再試".into()
    } else {
        format!("{action}的網路請求失敗：{error}")
    }
}

async fn require_success(
    response: reqwest::Response,
    action: &str,
) -> Result<reqwest::Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(match status {
        StatusCode::NOT_FOUND => "GitHub 上還沒有任何正式版本（404），請稍後再試".into(),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            "GitHub API 請求過於頻繁（限流），請稍後再試".into()
        }
        _ if status.is_server_error() => {
            format!("GitHub 服務暫時異常（HTTP {status}），請稍後再試")
        }
        _ => format!("{action}失敗（HTTP {status}）"),
    })
}

async fn read_limited(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    let too_large = || format!("GitHub 版本資訊超過 {} KiB 上限", limit / 1024);
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("讀取 GitHub 版本資訊失敗：{error}"))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(too_large());
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
            assert_eq!(
                asset.sha256.as_deref(),
                Some("8ac1afe1a1f0a593be08e2e2fa310f0b7b8238edaf0ea5ee5445ffed5663ed8c")
            );
            // 其他網域的附件一律不接受。
            let only_evil = serde_json::json!({"assets": [release["assets"][2].clone()]});
            assert!(pick_asset(&only_evil, "0.4.1").is_none());
        } else {
            assert!(asset.is_none());
        }
    }

    fn release(tag: &str) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "name": format!("MaiMotionDemo {tag}"),
            "body": format!("## 新功能

- {tag}"),
            "html_url": format!("https://github.com/silver0416/MaiMotionDemo/releases/tag/{tag}"),
            "published_at": "2026-11-06T00:00:00Z",
            "draft": false,
            "prerelease": false,
        })
    }

    fn versions(notes: &[ReleaseNote]) -> Vec<&str> {
        notes.iter().map(|note| note.version.as_str()).collect()
    }

    #[test]
    fn release_notes_cover_the_update_span() {
        let mut beta = release("v1.1.2-beta.1");
        beta["prerelease"] = true.into();
        beta["body"] = "beta".into();
        let mut draft = release("v1.2.0");
        draft["draft"] = true.into();
        // GitHub 是由新到舊，但挑選不靠順序。
        let releases = vec![
            draft,
            release("v1.1.1"),
            beta,
            release("v1.1.2"),
            release("v1.1.0"),
            release("nightly"),
            release("v1.0.9"),
        ];
        let pick = |from: Option<&str>| pick_release_notes(&releases, from, "1.1.2");
        assert_eq!(versions(&pick(Some("1.1.0"))), ["1.1.2", "1.1.1"]);
        assert_eq!(versions(&pick(Some("v1.0.9"))), ["1.1.2", "1.1.1", "1.1.0"]);
        assert_eq!(versions(&pick(Some("1.1.1"))), ["1.1.2"]);
        assert_eq!(
            versions(&pick(Some("0.1.0"))),
            ["1.1.2", "1.1.1", "1.1.0", "1.0.9"]
        );
        // 不知道舊版、同一版、降版或看不懂的版本號：只列目前這一版。
        for from in [None, Some("1.1.2"), Some("1.3.0"), Some("unknown"), Some("")] {
            assert_eq!(versions(&pick(from)), ["1.1.2"], "{from:?}");
        }
        let notes = pick(Some("1.1.1"));
        assert_eq!(notes[0].name, "MaiMotionDemo v1.1.2");
        assert_eq!(notes[0].body, "## 新功能

- v1.1.2");
        assert_eq!(
            notes[0].url,
            "https://github.com/silver0416/MaiMotionDemo/releases/tag/v1.1.2"
        );
        // 目前這一版還沒發布：只有中間的版本；只要這一版時是空的。
        assert_eq!(
            versions(&pick_release_notes(&releases, Some("1.1.0"), "1.1.3")),
            ["1.1.2", "1.1.1"]
        );
        assert!(pick_release_notes(&releases, None, "1.1.3").is_empty());
    }

    #[test]
    fn release_notes_keep_links_on_github() {
        let mut odd = release("v1.1.2");
        odd["html_url"] = "https://example.com/releases/tag/v1.1.2".into();
        odd["name"] = "  ".into();
        let notes = pick_release_notes(&[odd], None, "1.1.2");
        assert_eq!(notes[0].url, "https://github.com/silver0416/MaiMotionDemo/releases");
        assert_eq!(notes[0].name, "v1.1.2");
    }

    #[test]
    fn orders_versions_by_number() {
        assert_eq!(version_order("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(version_order("1.1", "1.1.0"), Ordering::Equal);
        assert_eq!(version_order("0.9", "1.0"), Ordering::Less);
        assert!(is_version("1.1.2") && is_version("2"));
        assert!(!is_version("1.2.x") && !is_version("1..2") && !is_version(""));
    }

    /// 實際連網：`cargo test -- --ignored live_release_notes`。
    #[test]
    #[ignore]
    fn live_release_notes() {
        let client = build_client().unwrap();
        let run = |from: Option<&str>| {
            tauri::async_runtime::block_on(release_notes(&client, from, "1.1.1")).unwrap()
        };
        let span = run(Some("1.0.9"));
        assert_eq!(versions(&span), ["1.1.1", "1.1.0"]);
        assert!(span.iter().all(|note| note.body.contains("## 下載")));
        assert_eq!(versions(&run(None)), ["1.1.1"]);
        let all = run(Some("0.0.1"));
        assert!(all.len() >= 30, "{}", all.len());
        assert_eq!(all.last().map(|note| note.version.as_str()), Some("0.1.0"));
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
