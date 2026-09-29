#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod majdata;
mod self_update;
mod update;
mod video;
mod wiki;

use mai_motion_core::{AnalyzeRequest, AnalyzeResponse, EvaluateRequest, EvaluateResponse};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct AnalysisGate(Arc<AtomicBool>);
struct MajdataClient(reqwest::Client);
struct UpdateClient(reqwest::Client);
struct BusyGuard(Arc<AtomicBool>);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
async fn analyze_chart(
    request: AnalyzeRequest,
    gate: tauri::State<'_, AnalysisGate>,
) -> Result<AnalyzeResponse, String> {
    if gate
        .0
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("正在分析另一份譜面，請等待完成後重試".into());
    }
    let guard = BusyGuard(gate.0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        mai_motion_core::analyze_chart(request)
    })
    .await
    .map_err(|e| format!("分析工作失敗：{e}"))
}

/// 比對真人標註與模型：求模型最佳解與照標註的最佳解。與分析共用同一個忙碌旗標。
#[tauri::command]
async fn evaluate_annotation(
    request: EvaluateRequest,
    gate: tauri::State<'_, AnalysisGate>,
) -> Result<EvaluateResponse, String> {
    if gate
        .0
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("正在分析另一份譜面，請等待完成後重試".into());
    }
    let guard = BusyGuard(gate.0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        mai_motion_core::evaluate_annotation(request)
    })
    .await
    .map_err(|e| format!("比對工作失敗：{e}"))
}

/// 批量匯出標註檔用：只解析譜面（不求解），回傳音符穩定鍵。順序與標註分頁相同：
/// 依判定時間，同時的依音符編號；這樣開著與沒開著的譜面匯出的順序一致。
#[tauri::command]
async fn chart_note_keys(source: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        mai_motion_core::parse_chart(&source, 0.0)
            .map(|parsed| {
                let number = |id: &str| id[1..].parse::<usize>().unwrap_or(usize::MAX);
                let mut notes = parsed.chart.notes;
                notes.sort_by(|a, b| {
                    a.time_seconds
                        .total_cmp(&b.time_seconds)
                        .then_with(|| number(&a.id).cmp(&number(&b.id)))
                });
                notes.into_iter().map(|n| n.key).collect()
            })
            .map_err(|d| d.message)
    })
    .await
    .map_err(|e| format!("解析工作失敗：{e}"))?
}

/// 匯出的文字檔只允許 .json，避免前端誤寫其他類型的檔案。
fn export_path_ok(path: &std::path::Path) -> Result<(), String> {
    let json = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if json {
        Ok(())
    } else {
        Err("只能存成 .json 檔".into())
    }
}

/// 存到使用者在另存新檔對話框選的位置（對話框已確認覆寫）。
#[tauri::command]
async fn save_text_file(path: String, contents: String) -> Result<String, String> {
    let target = std::path::PathBuf::from(&path);
    export_path_ok(&target)?;
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::write(&target, contents).map_err(|e| format!("無法寫入 {path}：{e}"))?;
        Ok(path)
    })
    .await
    .map_err(|e| format!("存檔工作失敗：{e}"))?
}

/// 存到預設資料夾：同名檔已存在時加上「 (2)」「 (3)」…，不覆寫。回傳實際路徑。
#[tauri::command]
async fn save_text_in_dir(dir: String, name: String, contents: String) -> Result<String, String> {
    let folder = std::path::PathBuf::from(&dir);
    if !folder.is_dir() {
        return Err(format!("預設匯出資料夾不存在：{dir}"));
    }
    if name.contains(['/', '\\']) || name.is_empty() {
        return Err("檔名不正確".into());
    }
    let first = folder.join(&name);
    export_path_ok(&first)?;
    tauri::async_runtime::spawn_blocking(move || {
        let stem = name.strip_suffix(".json").unwrap_or(&name).to_string();
        let mut target = first;
        let mut n = 2;
        while target.exists() {
            target = folder.join(format!("{stem} ({n}).json"));
            n += 1;
        }
        std::fs::write(&target, contents).map_err(|e| format!("無法寫入 {}：{e}", target.display()))?;
        Ok(target.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("存檔工作失敗：{e}"))?
}

#[tauri::command]
async fn search_majdata_charts(
    query: String,
    client: tauri::State<'_, MajdataClient>,
) -> Result<Vec<majdata::MajdataChartSummary>, String> {
    majdata::search(&client.0, query).await
}

#[tauri::command]
async fn fetch_majdata_chart(
    song_id: String,
    client: tauri::State<'_, MajdataClient>,
) -> Result<String, String> {
    majdata::fetch_chart(&client.0, song_id).await
}

/// 下載／更新 simai Wiki 的 Standard＋DX 索引。
/// `force` 為 true 時跳過記憶體與新鮮快取，直接抓 Wiki。
/// Wiki 失敗但有舊快取時回傳 stale 資料（`stale: true`），不直接報錯。
#[tauri::command]
async fn wiki_refresh_index(
    app: tauri::AppHandle,
    state: tauri::State<'_, wiki::WikiState>,
    force: bool,
) -> Result<wiki::WikiIndexPayload, String> {
    wiki::refresh_index(&app, &state, force).await
}

/// 本機搜尋 Wiki 索引（不打 Wiki）。`chart_type` 為 `all`／`standard`／`deluxe`。
#[tauri::command]
async fn wiki_search_songs(
    app: tauri::AppHandle,
    state: tauri::State<'_, wiki::WikiState>,
    query: String,
    chart_type: Option<String>,
) -> Result<Vec<wiki::WikiSong>, String> {
    // 搜尋本身是純本機計算；包成 async command 只是配合 invoke 簽名。
    wiki::search_cached(&app, &state, &query, chart_type.as_deref())
}

/// 下載歌曲頁並抽取指定難度的 raw simai 文字（已用既有 parser 驗證）。
/// `difficulty` 接受 `easy`/`basic`/`advanced`/`expert`/`master`/`remaster`
/// 或 `ESY`/`BSC`/`ADV`/`EXP`/`MAS`/`Re:MAS`。
#[tauri::command]
async fn wiki_fetch_chart(
    app: tauri::AppHandle,
    state: tauri::State<'_, wiki::WikiState>,
    page_id: u32,
    chart_type: String,
    difficulty: String,
) -> Result<wiki::WikiChartPayload, String> {
    let chart_type =
        wiki::ChartType::from_param(&chart_type).ok_or_else(|| "譜面種類無效".to_string())?;
    let difficulty =
        wiki::Difficulty::from_param(&difficulty).ok_or_else(|| "難度無效".to_string())?;
    wiki::fetch_chart(&app, &state, page_id, chart_type, difficulty).await
}

/// 目前這份建置的發佈通路：portable／installed；debug 建置回 dev。
#[tauri::command]
fn app_distribution() -> String {
    update::distribution().to_string()
}

/// 檢查 GitHub Releases 是否有新版。離線或限流時回 Err，前端靜默處理。
#[tauri::command]
async fn check_update(client: tauri::State<'_, UpdateClient>) -> Result<update::UpdateInfo, String> {
    update::check(&client.0).await
}

/// simai Wiki 磁碟快取（索引＋歌曲頁）的用量。
#[tauri::command]
fn wiki_cache_info(app: tauri::AppHandle) -> wiki::WikiCacheInfo {
    wiki::cache_info(&app)
}

/// 清除 simai Wiki 磁碟快取與記憶體索引；回傳清除前的用量。
#[tauri::command]
fn wiki_clear_cache(
    app: tauri::AppHandle,
    state: tauri::State<'_, wiki::WikiState>,
) -> Result<wiki::WikiCacheInfo, String> {
    wiki::clear_cache(&app, &state)
}

fn main() {
    let majdata_client = majdata::build_client().expect("Failed to initialize Majdata HTTP client");
    let update_client = update::build_client().expect("Failed to initialize update HTTP client");
    let wiki_client = wiki::build_client().expect("Failed to initialize simai Wiki HTTP client");
    let video_client = video::build_client().expect("Failed to initialize video tool HTTP client");
    let self_update_client =
        self_update::build_client().expect("Failed to initialize update download HTTP client");
    let previous = self_update::previous_from_args(std::env::args());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            video::commands::allow_videos(app.handle());
            Ok(())
        })
        // 主視窗關閉時一併關掉影片視窗，程式才會結束。
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                use tauri::Manager;
                if let Some(video) = window
                    .app_handle()
                    .get_webview_window(video::commands::WINDOW_LABEL)
                {
                    let _ = video.close();
                }
            }
        })
        .manage(AnalysisGate(Arc::new(AtomicBool::new(false))))
        .manage(MajdataClient(majdata_client))
        .manage(UpdateClient(update_client))
        .manage(wiki::WikiState::new(wiki_client))
        .manage(video::VideoState::new(video_client))
        .manage(self_update::SelfUpdateState::new(self_update_client, previous))
        .invoke_handler(tauri::generate_handler![
            analyze_chart,
            evaluate_annotation,
            chart_note_keys,
            save_text_file,
            save_text_in_dir,
            app_distribution,
            check_update,
            search_majdata_charts,
            fetch_majdata_chart,
            wiki_refresh_index,
            wiki_search_songs,
            wiki_fetch_chart,
            wiki_cache_info,
            wiki_clear_cache,
            video::commands::video_open_window,
            video::commands::video_tools_status,
            video::commands::video_install_tool,
            video::commands::video_remove_tool,
            video::commands::video_search,
            video::commands::video_download,
            video::commands::video_cancel_download,
            video::commands::video_get,
            video::commands::video_list,
            video::commands::video_delete,
            video::commands::video_cache_info,
            video::commands::video_clear_cache,
            video::commands::video_open_local,
            self_update::commands::update_download,
            self_update::commands::update_restart,
            self_update::commands::update_previous,
            self_update::commands::update_remove_previous
        ])
        .run(tauri::generate_context!())
        .expect("Failed to run MaiMotionDemo");
}
