//! 執行外掛程式並解讀它的輸出（協定第 1 版）。
//!
//! 外掛是獨立的執行檔，主程式用參數叫它做事，從 stdout 讀結果：
//! ```text
//! <entry> panel                    一筆 JSON：摘要、數字、可以按的動作
//! <entry> run <action> [...]       一行一筆 JSON 事件：progress／select／done／error
//! <entry> <其他查詢>                依能力而定（例如 adx-source 的 adx-catalog、adx-chart）
//! ```
//! 環境變數 `MMPLUGIN_DATA_DIR` 是外掛自己的資料夾（重裝、升級外掛不會動到）。
//! 取消時往 stdin 寫一行 `cancel`，外掛收尾後自己結束；幾秒內沒結束才強制關閉。

use super::package::Installed;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PROTOCOL: &str = "1";
/// 被使用者取消時回傳的錯誤字串；前端據此靜默處理（與分析的取消相同）。
pub const CANCELLED: &str = "cancelled";
const EVENT_LINE_LIMIT: u64 = 16 * 1024 * 1024;
const STDERR_KEEP_BYTES: usize = 2000;
const CANCEL_GRACE: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub label: String,
    pub value: String,
}

/// 動作需要使用者先選存檔位置（例如匯出）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSpec {
    pub default_name: String,
    #[serde(default)]
    pub filter_name: Option<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub primary: bool,
    /// 有值時，執行前先用這段文字請使用者確認。
    #[serde(default)]
    pub confirm: Option<String>,
    #[serde(default)]
    pub save: Option<SaveSpec>,
}

/// 外掛在設定頁顯示的內容。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Panel {
    pub summary: Option<String>,
    pub facts: Vec<Fact>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

/// 外掛請使用者勾選項目，確認後以勾選的 id 執行 `action`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectRequest {
    pub title: String,
    #[serde(default)]
    pub message: Option<String>,
    pub items: Vec<SelectItem>,
    pub action: String,
    #[serde(default)]
    pub confirm_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u64,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Progress(Progress),
    Select(SelectRequest),
    Done {
        #[serde(default)]
        message: Option<String>,
    },
    Error {
        message: String,
    },
}

/// 動作的結果：完成訊息，或請使用者勾選後再執行下一步。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub message: Option<String>,
    pub select: Option<SelectRequest>,
}

/// 動作代號與外掛代號同一套規則：小寫英數與連字號。
pub fn is_valid_action(action: &str) -> bool {
    super::package::is_valid_id(action)
}

/// 逐行累積事件，最後收斂成結果。和程序分開，方便測試。
#[derive(Default)]
pub struct Collector {
    result: RunResult,
    error: Option<String>,
    finished: bool,
}

impl Collector {
    /// 處理一行輸出；是進度就回傳，其餘記在自己身上。看不懂的行略過（外掛可以多印東西）。
    pub fn feed(&mut self, line: &str) -> Option<Progress> {
        match serde_json::from_str::<Event>(line.trim()).ok()? {
            Event::Progress(progress) => return Some(progress),
            Event::Select(request) => {
                self.result.select = Some(request);
                self.finished = true;
            }
            Event::Done { message } => {
                self.result.message = message;
                self.finished = true;
            }
            Event::Error { message } => self.error = Some(message),
        }
        None
    }

    pub fn finish(
        self,
        cancelled: bool,
        exit: Option<i32>,
        stderr: &str,
    ) -> Result<RunResult, String> {
        if cancelled {
            return Err(CANCELLED.into());
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.finished {
            return Ok(self.result);
        }
        let code = exit.map_or("被中止".to_string(), |code| format!("結束碼 {code}"));
        let detail = stderr.trim();
        Err(if detail.is_empty() {
            format!("外掛沒有回報結果（{code}）")
        } else {
            format!("外掛沒有回報結果（{code}）：{detail}")
        })
    }
}

struct Task {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    cancelled: AtomicBool,
}

impl Task {
    fn finished(&self) -> bool {
        self.child
            .lock()
            .map(|mut child| !matches!(child.try_wait(), Ok(None)))
            .unwrap_or(true)
    }

    fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

/// 正在執行的動作，一個外掛同時只跑一個。
#[derive(Clone, Default)]
pub struct Running(Arc<Mutex<HashMap<String, Arc<Task>>>>);

impl Running {
    pub fn is_running(&self, id: &str) -> bool {
        self.0.lock().is_ok_and(|tasks| tasks.contains_key(id))
    }

    /// 請外掛停下來；有在跑才回傳 true。
    pub fn cancel(&self, id: &str) -> bool {
        let Some(task) = self.0.lock().ok().and_then(|tasks| tasks.get(id).cloned()) else {
            return false;
        };
        task.cancelled.store(true, Ordering::Release);
        if let Some(mut stdin) = task.stdin.lock().ok().and_then(|mut slot| slot.take()) {
            let _ = stdin.write_all(b"cancel\n");
            let _ = stdin.flush();
        }
        std::thread::spawn(move || {
            let deadline = Instant::now() + CANCEL_GRACE;
            while Instant::now() < deadline {
                if task.finished() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            task.kill();
        });
        true
    }
}

fn command(plugin: &Installed, data_dir: &Path) -> Command {
    let mut command = Command::new(&plugin.entry);
    command
        .current_dir(&plugin.dir)
        .env("MMPLUGIN_PROTOCOL", PROTOCOL)
        .env("MMPLUGIN_DATA_DIR", data_dir)
        .env("MMPLUGIN_APP_VERSION", env!("CARGO_PKG_VERSION"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn spawn_error(error: std::io::Error) -> String {
    format!("無法啟動外掛：{error}")
}

/// 在背景把 stderr 讀完，只留最後一小段（錯誤訊息用）；不讀的話外掛寫滿管線會卡住。
fn drain_stderr(child: &mut Child) -> std::thread::JoinHandle<String> {
    let stderr = child.stderr.take();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_end(&mut bytes);
        }
        let start = bytes.len().saturating_sub(STDERR_KEEP_BYTES);
        String::from_utf8_lossy(&bytes[start..]).into_owned()
    })
}

pub struct QueryOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// 執行一個很快就會結束的查詢，收集 stdout。超過時間或大小就中止。
pub fn query(
    plugin: &Installed,
    data_dir: &Path,
    args: &[&str],
    limit: usize,
    timeout: Duration,
) -> Result<QueryOutput, String> {
    let mut child = command(plugin, data_dir)
        .args(args)
        .stdin(Stdio::null())
        .spawn()
        .map_err(spawn_error)?;
    let stderr = drain_stderr(&mut child);
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(stdout) = stdout {
            let _ = stdout.take(limit as u64 + 1).read_to_end(&mut bytes);
        }
        bytes
    });
    let deadline = Instant::now() + timeout;
    // 輸出過大時讀取端會停讀，外掛卡在寫入，一樣由期限收掉。
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = reader.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if stdout.len() > limit {
        return Err("外掛的輸出超過大小上限".into());
    }
    let Some(status) = status else {
        return Err("外掛沒有回應，已中止".into());
    };
    Ok(QueryOutput {
        code: status.code(),
        stdout,
        stderr,
    })
}

/// 讀取外掛在設定頁要顯示的內容。
pub fn panel(plugin: &Installed, data_dir: &Path) -> Result<Panel, String> {
    let output = query(
        plugin,
        data_dir,
        &["panel"],
        1024 * 1024,
        Duration::from_secs(20),
    )?;
    if output.code != Some(0) {
        let detail = output.stderr.trim();
        return Err(if detail.is_empty() {
            "外掛無法回報狀態".to_string()
        } else {
            format!("外掛無法回報狀態：{detail}")
        });
    }
    let mut panel: Panel = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("外掛回報的狀態格式不正確：{error}"))?;
    panel.actions.retain(|action| is_valid_action(&action.id));
    Ok(panel)
}

/// 執行一個動作直到結束，進度經 `on_progress` 回報。這個函式會阻塞，呼叫端放在背景執行緒。
pub fn run(
    running: &Running,
    plugin: &Installed,
    data_dir: &Path,
    action: &str,
    save_path: Option<&Path>,
    selection: Option<&[String]>,
    mut on_progress: impl FnMut(Progress),
) -> Result<RunResult, String> {
    if !is_valid_action(action) {
        return Err("外掛動作代號不正確".into());
    }
    std::fs::create_dir_all(data_dir).map_err(|error| format!("無法建立外掛資料夾：{error}"))?;
    let id = plugin.manifest.id.clone();

    // 勾選的項目可能上千筆，寫成檔案傳給外掛，不放在命令列上。
    let selection_file = match selection {
        Some(items) => {
            let path = data_dir.join(format!(".selection-{}.json", std::process::id()));
            let bytes = serde_json::to_vec(items).map_err(|error| error.to_string())?;
            std::fs::write(&path, bytes).map_err(|error| format!("無法寫入勾選清單：{error}"))?;
            Some(path)
        }
        None => None,
    };
    let cleanup = |path: &Option<std::path::PathBuf>| {
        if let Some(path) = path {
            let _ = std::fs::remove_file(path);
        }
    };

    let mut command = command(plugin, data_dir);
    command.arg("run").arg(action).stdin(Stdio::piped());
    if let Some(path) = save_path {
        command.arg("--save-path").arg(path);
    }
    if let Some(path) = &selection_file {
        command.arg("--select-file").arg(path);
    }

    // 登記與啟動在同一把鎖裡，同一個外掛不會同時跑兩個動作。
    let (task, stdout, stderr) = {
        let mut tasks = running.0.lock().map_err(|_| "外掛狀態錯誤".to_string())?;
        if tasks.contains_key(&id) {
            cleanup(&selection_file);
            return Err("這個外掛正在執行，請等它完成".into());
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                cleanup(&selection_file);
                return Err(spawn_error(error));
            }
        };
        let stderr = drain_stderr(&mut child);
        let stdout = child.stdout.take();
        let stdin = child.stdin.take();
        let task = Arc::new(Task {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            cancelled: AtomicBool::new(false),
        });
        tasks.insert(id.clone(), task.clone());
        (task, stdout, stderr)
    };

    let mut collector = Collector::default();
    if let Some(stdout) = stdout {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match (&mut reader)
                .take(EVENT_LINE_LIMIT)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if line.last() != Some(&b'\n') && line.len() as u64 >= EVENT_LINE_LIMIT {
                        task.kill();
                        break;
                    }
                    if let Some(progress) = collector.feed(&String::from_utf8_lossy(&line)) {
                        on_progress(progress);
                    }
                }
            }
        }
    }
    // stdout 關了就等程序結束；等的時候不握著鎖，取消才插得進來。
    let exit = loop {
        let status = task
            .child
            .lock()
            .ok()
            .and_then(|mut child| child.try_wait().ok());
        match status {
            Some(Some(status)) => break status.code(),
            Some(None) => std::thread::sleep(Duration::from_millis(20)),
            None => break None,
        }
    };
    let stderr = stderr.join().unwrap_or_default();
    if let Ok(mut tasks) = running.0.lock() {
        tasks.remove(&id);
    }
    cleanup(&selection_file);
    collector.finish(task.cancelled.load(Ordering::Acquire), exit, &stderr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_progress_then_done() {
        let mut collector = Collector::default();
        let progress = collector
            .feed(r#"{"type":"progress","done":3,"total":10,"message":"XODUS"}"#)
            .unwrap();
        assert_eq!((progress.done, progress.total), (3, Some(10)));
        assert!(collector.feed("not json").is_none());
        assert!(collector.feed(r#"{"type":"future-event"}"#).is_none());
        assert!(collector
            .feed(r#"{"type":"done","message":"已下載 10 首"}"#)
            .is_none());
        let result = collector.finish(false, Some(0), "").unwrap();
        assert_eq!(result.message.as_deref(), Some("已下載 10 首"));
        assert!(result.select.is_none());
    }

    #[test]
    fn select_request_is_a_result() {
        let mut collector = Collector::default();
        collector.feed(
            r#"{"type":"select","title":"新增 1 首","items":[{"id":"12061","label":"XODUS","detail":"DX","group":"新增"}],"action":"apply","confirmLabel":"下載"}"#,
        );
        let select = collector
            .finish(false, Some(0), "")
            .unwrap()
            .select
            .unwrap();
        assert_eq!(select.action, "apply");
        assert_eq!(select.items[0].id, "12061");
        assert_eq!(select.confirm_label.as_deref(), Some("下載"));
    }

    #[test]
    fn errors_and_cancellation_win() {
        let mut collector = Collector::default();
        collector.feed(r#"{"type":"error","message":"無法連線"}"#);
        assert_eq!(
            collector.finish(false, Some(1), "").unwrap_err(),
            "無法連線"
        );

        let mut collector = Collector::default();
        collector.feed(r#"{"type":"done","message":"已取消"}"#);
        assert_eq!(collector.finish(true, Some(0), "").unwrap_err(), CANCELLED);

        let silent = Collector::default().finish(false, Some(101), "panicked at x\n");
        assert!(silent.unwrap_err().contains("panicked at x"));
        assert!(Collector::default()
            .finish(false, None, "")
            .unwrap_err()
            .contains("被中止"));
    }

    #[test]
    fn panel_tolerates_missing_fields() {
        let panel: Panel = serde_json::from_str(
            r#"{"summary":"已存 2 首","actions":[{"id":"export","label":"匯出","save":{"defaultName":"a.sqlite","extensions":["sqlite"]}}]}"#,
        )
        .unwrap();
        assert!(panel.facts.is_empty());
        assert!(!panel.actions[0].primary);
        assert_eq!(
            panel.actions[0].save.as_ref().unwrap().default_name,
            "a.sqlite"
        );
        assert!(is_valid_action("apply"));
        assert!(!is_valid_action("--save-path"));
    }
}
