//! 選用外掛：另外安裝的獨立程式，替主程式加上不隨主程式發布的功能。
//!
//! - 封裝與安裝：`package`（.mmplugin，只收發布者簽章的封裝）。
//! - 執行與通訊：`host`（外掛是獨立執行檔，以參數與 stdout 的 JSON 溝通）。
//! - 程式放在 `<本機資料>/plugins/<id>/`，外掛自己的資料放在 `<本機資料>/plugin-data/<id>/`。
//!
//! 主程式本身不知道外掛做什麼：設定頁顯示外掛回報的摘要與動作；
//! 另外依外掛宣告的能力（`provides`）在固定的地方詢問它，例如 `adx-source`
//! 讓 adxdls 的目錄與譜面在離線時改由外掛提供。

mod host;
mod package;

pub use host::{Panel, Progress, RunResult};
pub use package::PACKAGE_EXTENSION;

use package::Installed;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// 能力：adxdls 的離線來源。外掛要回應 `adx-catalog` 與 `adx-chart <slug>`。
pub const ADX_SOURCE: &str = "adx-source";
/// `adx-chart` 查無此曲時的結束碼。
const NOT_FOUND_EXIT: i32 = 3;
const ADX_CATALOG_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const ADX_CHART_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MAX_SELECTION: usize = 50_000;

#[derive(Clone, Default)]
pub struct PluginState {
    running: host::Running,
    /// 這次執行期間已核對過執行檔雜湊的外掛（`id@version`）。
    verified: Arc<Mutex<HashSet<String>>>,
}

/// 前端看到的一個外掛。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub provides: Vec<String>,
    /// 外掛自己的資料佔用的空間。
    pub data_bytes: u64,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginProgress {
    pub id: String,
    #[serde(flatten)]
    pub progress: Progress,
}

fn base(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|error| format!("找不到本機資料夾：{error}"))
}

fn plugins_dir(base: &Path) -> PathBuf {
    base.join("plugins")
}

fn data_dir(base: &Path, id: &str) -> PathBuf {
    base.join("plugin-data").join(id)
}

fn dir_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn info(base: &Path, state: &PluginState, plugin: &Installed) -> PluginInfo {
    let manifest = &plugin.manifest;
    PluginInfo {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        provides: manifest.provides.clone(),
        data_bytes: dir_bytes(&data_dir(base, &manifest.id)),
        running: state.running.is_running(&manifest.id),
    }
}

fn installed(base: &Path) -> Vec<Installed> {
    package::list(
        &plugins_dir(base),
        &package::publisher_key(),
        env!("CARGO_PKG_VERSION"),
    )
}

/// 找到外掛並確認它的執行檔沒被換過（每次執行期間核對一次）。
fn ready(base: &Path, state: &PluginState, id: &str) -> Result<Installed, String> {
    if !package::is_valid_id(id) {
        return Err("外掛代號不正確".into());
    }
    let plugin = package::load(
        &plugins_dir(base).join(id),
        &package::publisher_key(),
        env!("CARGO_PKG_VERSION"),
    )
    .map_err(|_| "找不到這個外掛，可能已被移除".to_string())?;
    let mark = format!("{}@{}", plugin.manifest.id, plugin.manifest.version);
    let known = state
        .verified
        .lock()
        .is_ok_and(|verified| verified.contains(&mark));
    if !known {
        package::verify_entry(&plugin)?;
        if let Ok(mut verified) = state.verified.lock() {
            verified.insert(mark);
        }
    }
    Ok(plugin)
}

fn forget_verified(state: &PluginState, id: &str) {
    if let Ok(mut verified) = state.verified.lock() {
        verified.retain(|mark| mark.split('@').next() != Some(id));
    }
}

/// 提供某個能力的外掛（有多個時取代號最前面的）。
fn provider(base: &Path, state: &PluginState, capability: &str) -> Option<Installed> {
    let plugin = installed(base).into_iter().find(|plugin| {
        plugin
            .manifest
            .provides
            .iter()
            .any(|name| name == capability)
    })?;
    ready(base, state, &plugin.manifest.id).ok()
}

/// adxdls 離線目錄：站方目錄的原始物件與取得時間（Unix 秒）。沒有外掛或沒有資料回傳 None。
/// 會啟動外掛程式，呼叫端要放在背景執行緒。
pub fn adx_catalog(app: &AppHandle) -> Option<(u64, Vec<Value>)> {
    let base = base(app).ok()?;
    let state = app.state::<PluginState>();
    let plugin = provider(&base, &state, ADX_SOURCE)?;
    let output = host::query(
        &plugin,
        &data_dir(&base, &plugin.manifest.id),
        &["adx-catalog"],
        ADX_CATALOG_LIMIT_BYTES,
        Duration::from_secs(30),
    )
    .ok()
    .filter(|output| output.code == Some(0))?;
    let mut value: Value = serde_json::from_slice(&output.stdout).ok()?;
    let fetched_at = value.get("fetchedAt").and_then(Value::as_u64).unwrap_or(0);
    match value.get_mut("entries").map(Value::take) {
        Some(Value::Array(entries)) if !entries.is_empty() => Some((fetched_at, entries)),
        _ => None,
    }
}

/// adxdls 離線譜面：這首歌的 maidata.txt 原文。沒有外掛或外掛沒有這首回傳 None。
/// 會啟動外掛程式，呼叫端要放在背景執行緒。
pub fn adx_chart(app: &AppHandle, slug: &str) -> Option<String> {
    let base = base(app).ok()?;
    let state = app.state::<PluginState>();
    let plugin = provider(&base, &state, ADX_SOURCE)?;
    let output = host::query(
        &plugin,
        &data_dir(&base, &plugin.manifest.id),
        &["adx-chart", slug],
        ADX_CHART_LIMIT_BYTES,
        Duration::from_secs(20),
    )
    .ok()?;
    if output.code == Some(NOT_FOUND_EXIT) || output.code != Some(0) {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    (!text.trim().is_empty()).then_some(text)
}

pub mod commands {
    use super::*;
    use tauri::Emitter;

    async fn blocking<T: Send + 'static>(
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        tauri::async_runtime::spawn_blocking(work)
            .await
            .map_err(|error| format!("外掛工作失敗：{error}"))?
    }

    /// 已安裝的外掛。
    #[tauri::command]
    pub async fn plugin_list(
        app: AppHandle,
        state: tauri::State<'_, PluginState>,
    ) -> Result<Vec<PluginInfo>, String> {
        let base = base(&app)?;
        let state = state.inner().clone();
        blocking(move || {
            Ok(installed(&base)
                .iter()
                .map(|plugin| info(&base, &state, plugin))
                .collect())
        })
        .await
    }

    /// 安裝使用者選的 .mmplugin 封裝；同代號的舊版會被換掉。
    #[tauri::command]
    pub async fn plugin_install(
        app: AppHandle,
        state: tauri::State<'_, PluginState>,
        path: String,
    ) -> Result<PluginInfo, String> {
        let base = base(&app)?;
        let state = state.inner().clone();
        blocking(move || {
            let package_path = PathBuf::from(path);
            let named = package_path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case(PACKAGE_EXTENSION));
            if !named {
                return Err(format!("請選擇 .{PACKAGE_EXTENSION} 檔"));
            }
            // 正在執行的外掛換不掉；先擋下來，訊息比較清楚。
            if installed(&base)
                .iter()
                .any(|plugin| state.running.is_running(&plugin.manifest.id))
            {
                return Err("有外掛正在執行，請等它完成再安裝".into());
            }
            let plugin = package::install(
                &package_path,
                &plugins_dir(&base),
                &package::publisher_key(),
                env!("CARGO_PKG_VERSION"),
            )?;
            forget_verified(&state, &plugin.manifest.id);
            Ok(info(&base, &state, &plugin))
        })
        .await
    }

    /// 移除外掛程式；`delete_data` 為 true 時連外掛自己的資料一起刪。
    #[tauri::command]
    pub async fn plugin_remove(
        app: AppHandle,
        state: tauri::State<'_, PluginState>,
        id: String,
        delete_data: bool,
    ) -> Result<(), String> {
        let base = base(&app)?;
        let state = state.inner().clone();
        blocking(move || {
            if !package::is_valid_id(&id) {
                return Err("外掛代號不正確".into());
            }
            if state.running.is_running(&id) {
                return Err("這個外掛正在執行，請等它完成再移除".into());
            }
            let remove = |dir: PathBuf, what: &str| match std::fs::remove_dir_all(&dir) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("無法移除{what}（檔案可能正在使用中）：{error}")),
            };
            remove(plugins_dir(&base).join(&id), "外掛")?;
            forget_verified(&state, &id);
            if delete_data {
                remove(data_dir(&base, &id), "外掛的資料")?;
            }
            Ok(())
        })
        .await
    }

    /// 外掛在設定頁顯示的摘要、數字與動作。
    #[tauri::command]
    pub async fn plugin_panel(
        app: AppHandle,
        state: tauri::State<'_, PluginState>,
        id: String,
    ) -> Result<Panel, String> {
        let base = base(&app)?;
        let state = state.inner().clone();
        blocking(move || {
            let plugin = ready(&base, &state, &id)?;
            host::panel(&plugin, &data_dir(&base, &id))
        })
        .await
    }

    /// 執行外掛的一個動作直到結束。進度以 `plugin-progress` 事件送出；
    /// 被 `plugin_cancel` 取消時 reject `cancelled`。
    #[tauri::command]
    pub async fn plugin_run(
        app: AppHandle,
        state: tauri::State<'_, PluginState>,
        id: String,
        action: String,
        save_path: Option<String>,
        selection: Option<Vec<String>>,
    ) -> Result<RunResult, String> {
        let base = base(&app)?;
        let state = state.inner().clone();
        if selection.as_ref().is_some_and(|items| {
            items.len() > MAX_SELECTION || items.iter().any(|item| item.len() > 200)
        }) {
            return Err("勾選的項目太多".into());
        }
        blocking(move || {
            let plugin = ready(&base, &state, &id)?;
            let save_path = save_path.map(PathBuf::from);
            let emitter = app.clone();
            let plugin_id = id.clone();
            host::run(
                &state.running,
                &plugin,
                &data_dir(&base, &id),
                &action,
                save_path.as_deref(),
                selection.as_deref(),
                move |progress| {
                    let _ = emitter.emit(
                        "plugin-progress",
                        PluginProgress {
                            id: plugin_id.clone(),
                            progress,
                        },
                    );
                },
            )
        })
        .await
    }

    /// 請正在執行的外掛停下來。
    #[tauri::command]
    pub fn plugin_cancel(state: tauri::State<'_, PluginState>, id: String) -> bool {
        state.running.cancel(&id)
    }
}

/// 只在本機跑的整合測試（需要真的外掛封裝），一般建置與 CI 不會編進來。
#[cfg(all(test, feature = "local-plugin-test"))]
#[path = "../../../plugins/host_live_test.rs"]
mod live_test;
