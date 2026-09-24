//! Portable 版的程式內更新：
//! 1. 把新版執行檔下載到目前程式所在的資料夾（沒有寫入權限就改放「下載」資料夾），
//!    驗證大小與 SHA-256 後才放到正式檔名。
//! 2. 重新啟動：開啟新版並帶上 `--updated-from <舊版路徑>`，舊版自己結束。
//! 3. 新版啟動後詢問是否刪除舊版；只刪啟動參數指定、而且不是自己的那個執行檔。

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
        return Err(format!("下載更新失敗（HTTP {}）", response.status().as_u16()));
    }
    let mut file = std::fs::File::create(&temp).map_err(|error| format!("無法寫入更新檔：{error}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut reported: u64 = 0;
    let mut response = response;
    on_progress(DownloadProgress { downloaded: 0, total: asset.size });
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
            file.write_all(&chunk).map_err(|error| format!("無法寫入更新檔：{error}"))?;
            if downloaded - reported >= 256 * 1024 {
                reported = downloaded;
                on_progress(DownloadProgress { downloaded, total: asset.size });
            }
        }
        file.flush().map_err(|error| format!("無法寫入更新檔：{error}"))?;
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
    on_progress(DownloadProgress { downloaded, total: asset.size });
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
                download(&state.client, &asset, &exe, fallback.as_deref(), move |progress| {
                    let _ = emitter.emit("update-download-progress", progress);
                })
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
    pub fn update_restart(app: AppHandle, state: tauri::State<'_, SelfUpdateState>) -> Result<(), String> {
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

    /// 這次是從哪個舊版更新過來的（還在、可以刪的才回傳）。
    #[tauri::command]
    pub fn update_previous(state: tauri::State<'_, SelfUpdateState>) -> Option<String> {
        let previous = state.previous.lock().ok()?.clone()?;
        let exe = current_exe().ok()?;
        removable_previous(&previous, &exe).then(|| previous.to_string_lossy().into_owned())
    }

    /// 刪除舊版；成功或使用者選擇保留後就不再詢問。
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
        let dir = std::env::temp_dir().join(format!("maimotion-self-update-{name}-{}", std::process::id()));
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
        assert_eq!(previous_from_args(args(&["app.exe", "--updated-from"])), None);
    }

    #[test]
    fn only_other_existing_exe_is_removable() {
        let dir = temp_dir("removable");
        let old = dir.join("old.exe");
        let current = dir.join("new.exe");
        let text = dir.join("notes.txt");
        std::fs::write(&old, b"x").unwrap();
        std::fs::write(&current, b"x").unwrap();
        std::fs::write(&text, b"x").unwrap();
        assert!(removable_previous(&old, &current));
        assert!(!removable_previous(&current, &current));
        assert!(!removable_previous(&text, &current));
        assert!(!removable_previous(&dir.join("missing.exe"), &current));
        remove_previous(&old).unwrap();
        assert!(!old.exists());
        let _ = std::fs::remove_dir_all(&dir);
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
        assert!(run(asset(&format!("{ASSET_URL_PREFIX}v1/x.exe"), "../x.exe", 10)).is_err());
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
        let done = tauri::async_runtime::block_on(download(&client, &good, &exe, None, |p| last = p.downloaded)).unwrap();
        assert_eq!(last, good.size);
        assert!(!done.fallback);
        assert_eq!(std::fs::metadata(&done.path).unwrap().len(), good.size);
        let _ = std::fs::remove_file(&done.path);
        let bad = UpdateAsset { sha256: Some("0".repeat(64)), ..good };
        let error = tauri::async_runtime::block_on(download(&client, &bad, &exe, None, |_| {})).unwrap_err();
        assert!(error.contains("SHA-256"), "{error}");
        assert!(!dir.join(&bad.name).exists());
        assert!(!dir.join(format!(".{}.download", bad.name)).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
