//! Portable 版的程式內更新：
//! 1. 把新版執行檔下載到目前程式所在的資料夾（沒有寫入權限就改放「下載」資料夾），
//!    驗證大小與 SHA-256 後才放到正式檔名。
//! 2. 重新啟動：開啟新版並帶上 `--updated-from <舊版路徑>`，舊版自己結束。
//! 3. 新版啟動後自動刪除舊版。啟動參數誰都能帶，所以只刪同一個資料夾裡、
//!    內含本程式識別碼、而且不是自己的那個執行檔。

use crate::update::{UpdateAsset, ASSET_URL_PREFIX};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

pub const UPDATED_FROM_ARG: &str = "--updated-from";
const DOWNLOAD_LIMIT_BYTES: u64 = 200 * 1024 * 1024;
/// 本程式的識別碼（tauri.conf.json 的 identifier），每一版執行檔裡都有這串。
const APP_MARKER: &[u8] = b"dev.silver0416.maimotiondemo";

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Downloaded {
    pub path: String,
    pub name: String,
    /// true：程式所在資料夾不能寫入，改放到「下載」資料夾。
    pub fallback: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

/// 這次是從哪個舊版更新過來的。
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviousVersion {
    pub path: String,
    /// 舊版的版本號（`1.1.0`）；讀不到時是 null，更新內容就只列目前這一版。
    pub version: Option<String>,
}

pub struct SelfUpdateState {
    client: reqwest::Client,
    busy: AtomicBool,
    /// 這次下載好的新版；重新啟動只會開這個檔案。
    downloaded: Mutex<Option<PathBuf>>,
    /// 這次啟動是從哪個舊版更新過來的。
    previous: Mutex<Option<PathBuf>>,
}

impl SelfUpdateState {
    pub fn new(client: reqwest::Client, previous: Option<PathBuf>) -> Self {
        Self {
            client,
            busy: AtomicBool::new(false),
            downloaded: Mutex::new(None),
            previous: Mutex::new(previous),
        }
    }
}

pub fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .https_only(true)
        .user_agent(concat!("MaiMotionDemo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("無法建立更新下載的網路連線：{error}"))
}

/// 啟動參數裡的 `--updated-from <路徑>`。
pub fn previous_from_args(args: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == UPDATED_FROM_ARG {
            return args.next().map(PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix(&format!("{UPDATED_FROM_ARG}=")) {
            return Some(PathBuf::from(value));
        }
    }
    None
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// 可以寫入就回傳 true（實際建立再刪除一個暫存檔）。
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".maimotion-write-test-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 下載新版到 `dir`（不能寫入就用 `fallback_dir`），驗證後放到正式檔名。
pub async fn download(
    client: &reqwest::Client,
    asset: &UpdateAsset,
    current_exe: &Path,
    fallback_dir: Option<&Path>,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<Downloaded, String> {
    if !asset.url.starts_with(ASSET_URL_PREFIX)
        || asset.name.contains(['/', '\\'])
        || !asset.name.to_ascii_lowercase().ends_with(".exe")
    {
        return Err("更新檔的來源或檔名不正確，已停止下載".into());
    }
    if asset.size == 0 || asset.size > DOWNLOAD_LIMIT_BYTES {
        return Err("更新檔大小不正確".into());
    }
    let home = current_exe
        .parent()
        .ok_or_else(|| "找不到目前程式所在的資料夾".to_string())?;
    let (dir, fallback) = if writable(home) {
        (home.to_path_buf(), false)
    } else {
        let dir = fallback_dir
            .filter(|dir| writable(dir))
            .ok_or_else(|| "目前程式所在的資料夾不能寫入，也找不到可用的下載資料夾".to_string())?;
        (dir.to_path_buf(), true)
    };
    let target = dir.join(&asset.name);
    if same_file(&target, current_exe) {
        return Err("目前執行的就是這一版，不需要更新".into());
    }
    let temp = dir.join(format!(".{}.download", asset.name));

    let response = client.get(&asset.url).send().await.map_err(|error| {
        if error.is_timeout() {
            "下載更新逾時，請稍後再試".to_string()
        } else if error.is_connect() {
            "無法連線 GitHub，請檢查網路後再試".to_string()
        } else {
            format!("下載更新失敗：{error}")
        }
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "下載更新失敗（HTTP {}）",
            response.status().as_u16()
        ));
    }
    let mut file =
        std::fs::File::create(&temp).map_err(|error| format!("無法寫入更新檔：{error}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut reported: u64 = 0;
    let mut response = response;
    on_progress(DownloadProgress {
        downloaded: 0,
        total: asset.size,
    });
    let result: Result<(), String> = async {
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("下載中斷：{error}"))?
        {
            downloaded += chunk.len() as u64;
            if downloaded > asset.size {
                return Err("下載的檔案比預期大，已停止".into());
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .map_err(|error| format!("無法寫入更新檔：{error}"))?;
            if downloaded - reported >= 256 * 1024 {
                reported = downloaded;
                on_progress(DownloadProgress {
                    downloaded,
                    total: asset.size,
                });
            }
        }
        file.flush()
            .map_err(|error| format!("無法寫入更新檔：{error}"))?;
        if downloaded != asset.size {
            return Err("下載的檔案不完整，請再試一次".into());
        }
        if let Some(expected) = &asset.sha256 {
            if hex(&hasher.finalize_reset()) != *expected {
                return Err("下載的檔案驗證失敗（SHA-256 不符），已刪除".into());
            }
        }
        Ok(())
    }
    .await;
    drop(file);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    on_progress(DownloadProgress {
        downloaded,
        total: asset.size,
    });
    if target.exists() {
        std::fs::remove_file(&target).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            format!("資料夾裡已有同名檔案而且無法取代（可能正在執行）：{error}")
        })?;
    }
    std::fs::rename(&temp, &target).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        format!("無法放置更新檔：{error}")
    })?;
    Ok(Downloaded {
        path: target.to_string_lossy().into_owned(),
        name: asset.name.clone(),
        fallback,
    })
}

/// 開啟新版，並告訴它舊版在哪裡。
pub fn launch(new_exe: &Path, current_exe: &Path) -> Result<(), String> {
    if !new_exe.is_file() {
        return Err("找不到下載好的新版，請重新下載".into());
    }
    Command::new(new_exe)
        .arg(UPDATED_FROM_ARG)
        .arg(current_exe)
        .current_dir(new_exe.parent().unwrap_or(Path::new(".")))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("無法開啟新版：{error}"))
}

/// 可以刪除的舊版：存在、是 exe，而且不是目前執行的程式。
pub fn removable_previous(previous: &Path, current_exe: &Path) -> bool {
    previous.is_file()
        && previous
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        && !same_file(previous, current_exe)
        && same_dir(previous, current_exe)
        && is_this_app(previous)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.parent(), b.parent()) {
        (Some(a), Some(b)) => same_file(a, b),
        _ => false,
    }
}

/// 執行檔裡找得到本程式的識別碼，才確定是舊版的 MaiMotionDemo。
fn is_this_app(path: &Path) -> bool {
    let size_ok = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= DOWNLOAD_LIMIT_BYTES);
    size_ok
        && std::fs::read(path).is_ok_and(|bytes| {
            bytes
                .windows(APP_MARKER.len())
                .any(|window| window == APP_MARKER)
        })
}

/// 舊版的版本號：先看執行檔的版本資源，讀不到再看檔名（`MaiMotionDemo-v1.1.0-windows-x64.exe`）。
/// 舊版會在新版啟動後刪掉，所以要在刪除前讀。
pub fn previous_version(path: &Path) -> Option<String> {
    let from_resource = std::fs::metadata(path)
        .is_ok_and(|meta| meta.len() <= DOWNLOAD_LIMIT_BYTES)
        .then(|| std::fs::read(path).ok())
        .flatten()
        .and_then(|bytes| version_resource(&bytes));
    from_resource.or_else(|| version_in_name(path))
}

/// Windows 執行檔的版本資源：鍵名「VS_VERSION_INFO」（UTF-16）後面補零對齊，
/// 接著是 VS_FIXEDFILEINFO（簽章 0xFEEF04BD、結構版本 0x00010000）。
/// 每一版建置時都用 Cargo 的版本號填這裡，取產品版本的前三段。
fn version_resource(bytes: &[u8]) -> Option<String> {
    const KEY: &[u8] = b"V\0S\0_\0V\0E\0R\0S\0I\0O\0N\0_\0I\0N\0F\0O\0\0\0";
    let dword = |at: usize| -> Option<u32> {
        let slice = bytes.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    };
    let mut from = 0;
    while let Some(found) = bytes
        .get(from..)?
        .windows(KEY.len())
        .position(|window| window == KEY)
    {
        let end = from + found + KEY.len();
        for start in end..end + 4 {
            if dword(start) == Some(0xFEEF_04BD) && dword(start + 4) == Some(0x0001_0000) {
                let major_minor = dword(start + 16)?;
                let patch_build = dword(start + 20)?;
                return Some(format!(
                    "{}.{}.{}",
                    major_minor >> 16,
                    major_minor & 0xFFFF,
                    patch_build >> 16
                ));
            }
        }
        from = end;
    }
    None
}

/// 檔名裡的 `v1.2.3`。
fn version_in_name(path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_str()?;
    name.match_indices(['v', 'V']).find_map(|(at, _)| {
        let digits: String = name[at + 1..]
            .chars()
            .take_while(|char| char.is_ascii_digit() || *char == '.')
            .collect();
        let parts: Vec<&str> = digits.split('.').take(3).collect();
        (parts.len() == 3 && parts.iter().all(|part| !part.is_empty())).then(|| parts.join("."))
    })
}

/// 刪除舊版；舊程式可能還在結束中，最多等幾秒。
pub fn remove_previous(previous: &Path) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..40 {
        match std::fs::remove_file(previous) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!("無法刪除舊版（可能還在執行）：{last}"))
}

// ---------------------------------------------------------------------------
// 與 Tauri 的接點

pub mod commands {
    use super::*;
    use tauri::{AppHandle, Emitter, Manager};

    fn current_exe() -> Result<PathBuf, String> {
        std::env::current_exe().map_err(|error| format!("找不到目前的程式：{error}"))
    }

    /// 下載新版。只有 Portable 版可用；安裝版與開發版回錯誤。
    #[tauri::command]
    pub async fn update_download(
        app: AppHandle,
        state: tauri::State<'_, SelfUpdateState>,
        asset: UpdateAsset,
    ) -> Result<Downloaded, String> {
        if crate::update::distribution() != "portable" {
            return Err("只有 Portable 版可以在程式內更新".into());
        }
        if state
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("正在下載更新".into());
        }
        let exe = current_exe();
        let fallback = app.path().download_dir().ok();
        let emitter = app.clone();
        let result = match exe {
            Ok(exe) => {
                download(
                    &state.client,
                    &asset,
                    &exe,
                    fallback.as_deref(),
                    move |progress| {
                        let _ = emitter.emit("update-download-progress", progress);
                    },
                )
                .await
            }
            Err(error) => Err(error),
        };
        state.busy.store(false, Ordering::Release);
        let downloaded = result?;
        if let Ok(mut slot) = state.downloaded.lock() {
            *slot = Some(PathBuf::from(&downloaded.path));
        }
        Ok(downloaded)
    }

    /// 開啟下載好的新版並結束目前的程式。
    #[tauri::command]
    pub fn update_restart(
        app: AppHandle,
        state: tauri::State<'_, SelfUpdateState>,
    ) -> Result<(), String> {
        let target = state
            .downloaded
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| "還沒有下載好的新版".to_string())?;
        launch(&target, &current_exe()?)?;
        app.exit(0);
        Ok(())
    }

    /// 這次是從哪個舊版更新過來的（還在、可以刪的才回傳），連同舊版的版本號。
    /// 要讀整個舊版執行檔，放到背景執行緒，不卡住畫面。
    #[tauri::command]
    pub async fn update_previous(
        state: tauri::State<'_, SelfUpdateState>,
    ) -> Result<Option<PreviousVersion>, String> {
        let Some(previous) = state.previous.lock().ok().and_then(|slot| slot.clone()) else {
            return Ok(None);
        };
        let exe = current_exe()?;
        tauri::async_runtime::spawn_blocking(move || {
            removable_previous(&previous, &exe).then(|| PreviousVersion {
                version: previous_version(&previous),
                path: previous.to_string_lossy().into_owned(),
            })
        })
        .await
        .map_err(|error| format!("讀取舊版資訊失敗：{error}"))
    }

    /// 刪除舊版（新版啟動後自動呼叫）；remove 為 false 時只放棄這次的舊版記錄。
    #[tauri::command]
    pub async fn update_remove_previous(
        state: tauri::State<'_, SelfUpdateState>,
        remove: bool,
    ) -> Result<(), String> {
        let previous = state.previous.lock().ok().and_then(|mut slot| slot.take());
        let Some(previous) = previous else {
            return Ok(());
        };
        if !remove {
            return Ok(());
        }
        let exe = current_exe()?;
        if !removable_previous(&previous, &exe) {
            return Ok(());
        }
        tauri::async_runtime::spawn_blocking(move || remove_previous(&previous))
            .await
            .map_err(|error| format!("刪除舊版失敗：{error}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "maimotion-self-update-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_updated_from_argument() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            previous_from_args(args(&["app.exe", "--updated-from", "C:/old.exe"])),
            Some(PathBuf::from("C:/old.exe"))
        );
        assert_eq!(
            previous_from_args(args(&["app.exe", "--updated-from=C:/old.exe"])),
            Some(PathBuf::from("C:/old.exe"))
        );
        assert_eq!(previous_from_args(args(&["app.exe"])), None);
        assert_eq!(
            previous_from_args(args(&["app.exe", "--updated-from"])),
            None
        );
    }

    fn app_bytes() -> Vec<u8> {
        [b"MZ...".as_slice(), APP_MARKER, b"...".as_slice()].concat()
    }

    #[test]
    fn only_this_apps_exe_beside_us_is_removable() {
        let dir = temp_dir("removable");
        let old = dir.join("old.exe");
        let current = dir.join("new.exe");
        let text = dir.join("notes.txt");
        let foreign = dir.join("other-program.exe");
        std::fs::write(&old, app_bytes()).unwrap();
        std::fs::write(&current, app_bytes()).unwrap();
        std::fs::write(&text, app_bytes()).unwrap();
        std::fs::write(&foreign, b"MZ other program").unwrap();
        assert!(removable_previous(&old, &current));
        assert!(!removable_previous(&current, &current));
        assert!(!removable_previous(&text, &current));
        assert!(!removable_previous(&foreign, &current));
        assert!(!removable_previous(&dir.join("missing.exe"), &current));
        // 別的資料夾裡的舊版不刪，即使內容是本程式。
        let elsewhere = temp_dir("removable-elsewhere");
        let away = elsewhere.join("old.exe");
        std::fs::write(&away, app_bytes()).unwrap();
        assert!(!removable_previous(&away, &current));
        remove_previous(&old).unwrap();
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    /// 模擬版本資源：VS_VERSIONINFO 開頭（長度、值長度、型別）、鍵名、對齊補零、VS_FIXEDFILEINFO。
    fn version_info(major: u16, minor: u16, patch: u16, padding: usize) -> Vec<u8> {
        let mut bytes = b"MZ\x90\0garbage \xBD\x04\xEF\xFE not the resource".to_vec();
        bytes.extend_from_slice(&[0x5C, 0x03, 0x34, 0x00, 0x00, 0x00]);
        bytes.extend_from_slice(
            &"VS_VERSION_INFO\0"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<u8>>(),
        );
        bytes.extend(std::iter::repeat_n(0u8, padding));
        let ms = (u32::from(major) << 16) | u32::from(minor);
        let ls = u32::from(patch) << 16;
        for dword in [0xFEEF_04BDu32, 0x0001_0000, ms, ls, ms, ls] {
            bytes.extend_from_slice(&dword.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 28]);
        bytes
    }

    #[test]
    fn reads_the_version_resource() {
        assert_eq!(
            version_resource(&version_info(1, 1, 0, 0)).as_deref(),
            Some("1.1.0")
        );
        assert_eq!(
            version_resource(&version_info(0, 12, 3, 2)).as_deref(),
            Some("0.12.3")
        );
        // 只有簽章、沒有鍵名的位元組不算。
        assert_eq!(version_resource(b"\xBD\x04\xEF\xFE\0\0\x01\0"), None);
        let mut cut = version_info(1, 1, 0, 0);
        cut.truncate(cut.len() - 40);
        assert_eq!(version_resource(&cut), None);
    }

    #[test]
    fn reads_the_version_in_the_file_name() {
        let name = |name: &str| version_in_name(Path::new(name));
        assert_eq!(
            name("C:/apps/MaiMotionDemo-v1.1.0-windows-x64.exe").as_deref(),
            Some("1.1.0")
        );
        assert_eq!(name("MaiMotionDemo-V0.4.12.exe").as_deref(), Some("0.4.12"));
        assert_eq!(name("mai-motion-demo.exe"), None);
        assert_eq!(name("MaiMotionDemo-v1.1.exe"), None);
    }

    #[test]
    fn previous_version_prefers_the_resource() {
        let dir = temp_dir("previous-version");
        let named = dir.join("MaiMotionDemo-v1.0.9-windows-x64.exe");
        std::fs::write(&named, version_info(1, 1, 0, 0)).unwrap();
        assert_eq!(previous_version(&named).as_deref(), Some("1.1.0"));
        std::fs::write(&named, app_bytes()).unwrap();
        assert_eq!(previous_version(&named).as_deref(), Some("1.0.9"));
        let plain = dir.join("old.exe");
        std::fs::write(&plain, app_bytes()).unwrap();
        assert_eq!(previous_version(&plain), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 讀實際建置的執行檔：`cargo build --release` 之後
    /// `cargo test -- --ignored reads_the_built_exe_version`。
    #[test]
    #[ignore]
    fn reads_the_built_exe_version() {
        let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/mai-motion-demo.exe");
        assert_eq!(
            previous_version(&exe).as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn marker_matches_the_app_identifier() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            config["identifier"].as_str().unwrap().as_bytes(),
            APP_MARKER
        );
    }

    #[test]
    fn rejects_foreign_or_odd_assets() {
        let dir = temp_dir("reject");
        let exe = dir.join("app.exe");
        let client = build_client().unwrap();
        let asset = |url: &str, name: &str, size: u64| UpdateAsset {
            name: name.into(),
            url: url.into(),
            size,
            sha256: None,
        };
        let run = |asset: UpdateAsset| {
            tauri::async_runtime::block_on(download(&client, &asset, &exe, None, |_| {}))
        };
        assert!(run(asset("https://example.com/x.exe", "x.exe", 10)).is_err());
        assert!(run(asset(&format!("{ASSET_URL_PREFIX}v1/x.zip"), "x.zip", 10)).is_err());
        assert!(run(asset(
            &format!("{ASSET_URL_PREFIX}v1/x.exe"),
            "../x.exe",
            10
        ))
        .is_err());
        assert!(run(asset(&format!("{ASSET_URL_PREFIX}v1/x.exe"), "x.exe", 0)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 實際連網：下載 v0.4.0 的執行檔並驗證 SHA-256，也測雜湊不符會被拒絕。
    /// `cargo test -- --ignored live_download_release`。
    #[test]
    #[ignore]
    fn live_download_release() {
        let dir = temp_dir("live");
        let exe = dir.join("old.exe");
        std::fs::write(&exe, b"old").unwrap();
        let client = build_client().unwrap();
        let good = UpdateAsset {
            name: "MaiMotionDemo-v0.4.0-windows-x64.exe".into(),
            url: format!("{ASSET_URL_PREFIX}v0.4.0/MaiMotionDemo-v0.4.0-windows-x64.exe"),
            size: 13_737_472,
            sha256: Some("8ac1afe1a1f0a593be08e2e2fa310f0b7b8238edaf0ea5ee5445ffed5663ed8c".into()),
        };
        let mut last = 0;
        let done = tauri::async_runtime::block_on(download(&client, &good, &exe, None, |p| {
            last = p.downloaded
        }))
        .unwrap();
        assert_eq!(last, good.size);
        assert!(!done.fallback);
        assert_eq!(std::fs::metadata(&done.path).unwrap().len(), good.size);
        let _ = std::fs::remove_file(&done.path);
        let bad = UpdateAsset {
            sha256: Some("0".repeat(64)),
            ..good
        };
        let error = tauri::async_runtime::block_on(download(&client, &bad, &exe, None, |_| {}))
            .unwrap_err();
        assert!(error.contains("SHA-256"), "{error}");
        assert!(!dir.join(&bad.name).exists());
        assert!(!dir.join(format!(".{}.download", bad.name)).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
