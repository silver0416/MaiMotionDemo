#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod majdata;
mod wiki;

use mai_motion_core::{AnalyzeRequest, AnalyzeResponse};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct AnalysisGate(Arc<AtomicBool>);
struct MajdataClient(reqwest::Client);
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

/// 本地搜尋 Wiki 索引（不打 Wiki）。`chart_type` 為 `all`／`standard`／`deluxe`。
#[tauri::command]
async fn wiki_search_songs(
    app: tauri::AppHandle,
    state: tauri::State<'_, wiki::WikiState>,
    query: String,
    chart_type: Option<String>,
) -> Result<Vec<wiki::WikiSong>, String> {
    // 搜尋本身是純本地計算；包成 async command 只是配合 invoke 簽名。
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

fn main() {
    let majdata_client = majdata::build_client().expect("Failed to initialize Majdata HTTP client");
    let wiki_client = wiki::build_client().expect("Failed to initialize simai Wiki HTTP client");
    tauri::Builder::default()
        .manage(AnalysisGate(Arc::new(AtomicBool::new(false))))
        .manage(MajdataClient(majdata_client))
        .manage(wiki::WikiState::new(wiki_client))
        .invoke_handler(tauri::generate_handler![
            analyze_chart,
            search_majdata_charts,
            fetch_majdata_chart,
            wiki_refresh_index,
            wiki_search_songs,
            wiki_fetch_chart
        ])
        .run(tauri::generate_context!())
        .expect("Failed to run MaiMotionDemo");
}
