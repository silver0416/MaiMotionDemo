#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod majdata;

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

fn main() {
    let majdata_client = majdata::build_client().expect("Failed to initialize Majdata HTTP client");
    tauri::Builder::default()
        .manage(AnalysisGate(Arc::new(AtomicBool::new(false))))
        .manage(MajdataClient(majdata_client))
        .invoke_handler(tauri::generate_handler![
            analyze_chart,
            search_majdata_charts,
            fetch_majdata_chart
        ])
        .run(tauri::generate_context!())
        .expect("Failed to run MaiMotionDemo");
}
