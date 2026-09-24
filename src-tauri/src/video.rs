//! 影片同步比對：yt-dlp／Deno 工具管理、YouTube 搜尋與下載、本機影片快取。
//!
//! - 工具放在 `<本機資料>/tools`，第一次使用時從 GitHub Releases 下載最新版；
//!   沒有下載時退回系統 PATH 上的 yt-dlp。
//! - 影片放在 `<本機資料>/videos/<YouTube ID>.<副檔名>`，旁邊的 `<ID>.json` 記錄標題等資訊。
//! - 只下載不需要 ffmpeg 合併的單一檔案（720p 以下的純影像優先），標註用不到聲音。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 標註用的畫質：720p 以下、WebView 能直接播放的單一檔案。
const FORMAT: &str = "bv*[vcodec^=avc1][height<=720]/bv*[vcodec^=vp][height<=720]/b[ext=mp4][height<=720]/bv*[height<=720]/b";
/// yt-dlp 從這一版開始支援 `--js-runtimes`。
const JS_RUNTIMES_SINCE: &str = "2025.11";
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);
const TOOL_LIMIT_BYTES: u64 = 400 * 1024 * 1024;
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "webm", "mov", "mkv"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    YtDlp,
    Deno,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub path: String,
    pub version: String,
    /// 由本程式下載管理（可更新、可刪除）；false 表示系統 PATH 上的。
    pub managed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    /// `deno` 或 `node`
    pub kind: String,
    pub path: String,
    pub version: String,
    pub managed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsStatus {
    pub yt_dlp: Option<ToolInfo>,
    pub runtime: Option<RuntimeInfo>,
    /// 這個平台有現成的下載檔。
    pub installable: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchItem {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub duration: Option<f64>,
    pub views: Option<u64>,
    pub downloaded: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoEntry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// 影片檔名（在 videos 目錄內）。
    pub file: String,
    /// 完整路徑；讀取時才填，不寫進 json。
    #[serde(default, skip_deserializing, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_deserializing, skip_serializing_if = "is_zero")]
    pub bytes: u64,
    #[serde(default)]
    pub downloaded_at: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVideo {
    pub path: String,
    pub name: String,
    pub bytes: u64,
}

/// 下載進度（事件 `video-download-progress`）。
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: Option<u64>,
    pub total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
}

/// 工具下載進度（事件 `video-tool-progress`）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProgress {
    pub tool: Tool,
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// 搜尋結果帶來的資訊；yt-dlp 沒回報時用它補齊。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoHint {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub duration: Option<f64>,
}

struct Job {
    pid: Option<u32>,
    cancelled: Arc<AtomicBool>,
}

pub struct VideoState {
    client: reqwest::Client,
    jobs: Mutex<HashMap<String, Job>>,
    installing: Mutex<Vec<Tool>>,
    status: Mutex<Option<ToolsStatus>>,
}

impl VideoState {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            jobs: Mutex::new(HashMap::new()),
            installing: Mutex::new(Vec::new()),
            status: Mutex::new(None),
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
        .map_err(|error| format!("無法建立工具下載的網路連線：{error}"))
}

// ---------------------------------------------------------------------------
// 目錄與小工具

pub fn tools_dir(base: &Path) -> PathBuf {
    base.join("tools")
}

pub fn videos_dir(base: &Path) -> PathBuf {
    base.join("videos")
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// YouTube 影片 ID：11 個英數字、`-` 或 `_`。檢查過才會拿來組網址與檔名。
pub fn valid_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn which(name: &str) -> Option<PathBuf> {
    let file = exe(name);
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Windows 的管線預設是系統語系編碼，影片標題會變亂碼。
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// 結束程序與它的子程序。yt-dlp.exe 是 PyInstaller 單檔，真正工作的是子程序。
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
}

struct Output {
    success: bool,
    stdout: String,
    stderr: String,
}

fn drain<R: Read + Send + 'static>(reader: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut reader) = reader {
            let _ = reader.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn run(mut command: Command, timeout: Duration) -> Result<Output, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("無法執行外部工具：{error}"))?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                kill_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err("外部工具執行逾時".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(error) => return Err(format!("等待外部工具失敗：{error}")),
        }
    };
    Ok(Output {
        success: status.success(),
        stdout: String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned(),
        stderr: String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned(),
    })
}

fn version_of(path: &Path) -> Option<String> {
    let mut command = command(path);
    command.arg("--version");
    let output = run(command, VERSION_TIMEOUT).ok()?;
    if !output.success {
        return None;
    }
    let line = output.stdout.lines().next()?.trim().to_string();
    // deno 印「deno 2.x.y (...)」、node 印「v22.x」、yt-dlp 印「2025.11.12」。
    let version = line
        .strip_prefix("deno ")
        .map(|rest| rest.split_whitespace().next().unwrap_or(rest).to_string())
        .unwrap_or(line);
    (!version.is_empty()).then_some(version)
}

/// yt-dlp 與 JS 執行環境的現況。受管理的工具優先，其次是系統 PATH。
pub fn detect(base: &Path) -> ToolsStatus {
    let tools = tools_dir(base);
    let managed_yt = tools.join(exe("yt-dlp"));
    let yt_dlp = [(managed_yt, true)]
        .into_iter()
        .chain(which("yt-dlp").map(|path| (path, false)))
        .filter(|(path, _)| path.is_file())
        .find_map(|(path, managed)| {
            version_of(&path).map(|version| ToolInfo {
                path: path.to_string_lossy().into_owned(),
                version,
                managed,
            })
        });
    let managed_deno = tools.join(exe("deno"));
    let runtime = [
        ("deno", Some(managed_deno), true),
        ("deno", which("deno"), false),
        ("node", which("node"), false),
    ]
    .into_iter()
    .filter_map(|(kind, path, managed)| path.filter(|p| p.is_file()).map(|p| (kind, p, managed)))
    .find_map(|(kind, path, managed)| {
        version_of(&path).map(|version| RuntimeInfo {
            kind: kind.into(),
            path: path.to_string_lossy().into_owned(),
            version,
            managed,
        })
    });
    ToolsStatus {
        yt_dlp,
        runtime,
        installable: asset_name(Tool::YtDlp).is_some(),
    }
}

/// 告訴 yt-dlp 要用哪個 JS 執行環境；舊版 yt-dlp 不認得這個參數就不加。
fn runtime_args(yt_dlp: &ToolInfo, runtime: Option<&RuntimeInfo>) -> Vec<String> {
    let Some(runtime) = runtime else {
        return Vec::new();
    };
    if yt_dlp.version.as_str() < JS_RUNTIMES_SINCE {
        return Vec::new();
    }
    vec![
        "--js-runtimes".into(),
        format!("{}:{}", runtime.kind, runtime.path),
    ]
}

fn yt_command(status: &ToolsStatus) -> Result<(Command, &ToolInfo), String> {
    let yt_dlp = status
        .yt_dlp
        .as_ref()
        .ok_or_else(|| "還沒有 yt-dlp，請先在影片視窗或設定裡下載".to_string())?;
    let mut command = command(Path::new(&yt_dlp.path));
    command.args(["--no-config", "--ignore-config"]);
    command.args(runtime_args(yt_dlp, status.runtime.as_ref()));
    Ok((command, yt_dlp))
}

/// yt-dlp 的錯誤訊息：取最後一行 ERROR，去掉前綴。
fn error_message(stderr: &str, fallback: &str) -> String {
    stderr
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("ERROR:"))
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

// ---------------------------------------------------------------------------
// 搜尋

pub fn parse_search(
    json: &str,
    downloaded: impl Fn(&str) -> bool,
) -> Result<Vec<SearchItem>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("搜尋結果格式無法解析：{error}"))?;
    let entries = value
        .get("entries")
        .and_then(|entries| entries.as_array())
        .cloned()
        .unwrap_or_default();
    let text = |entry: &serde_json::Value, key: &str| {
        entry
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    Ok(entries
        .iter()
        .filter(|entry| {
            let live = text(entry, "live_status");
            live != "is_live" && live != "is_upcoming"
        })
        .filter_map(|entry| {
            let id = text(entry, "id");
            if !valid_id(&id) {
                return None;
            }
            let channel = [text(entry, "channel"), text(entry, "uploader")]
                .into_iter()
                .find(|name| !name.is_empty())
                .unwrap_or_default();
            Some(SearchItem {
                downloaded: downloaded(&id),
                title: text(entry, "title"),
                channel,
                duration: entry.get("duration").and_then(|v| v.as_f64()),
                views: entry.get("view_count").and_then(|v| v.as_u64()),
                id,
            })
        })
        .collect())
}

pub fn search(
    status: &ToolsStatus,
    videos: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchItem>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("請輸入搜尋文字".into());
    }
    let (mut command, _) = yt_command(status)?;
    command
        .args(["--flat-playlist", "-J", "--no-warnings"])
        .arg(format!("ytsearch{}:{query}", limit.clamp(1, 30)));
    let output = run(command, SEARCH_TIMEOUT)?;
    if !output.success {
        return Err(error_message(&output.stderr, "YouTube 搜尋失敗"));
    }
    parse_search(&output.stdout, |id| find_entry(videos, id).is_some())
}

// ---------------------------------------------------------------------------
// 下載

/// 解析 `--progress-template` 輸出的一行；不是進度行回傳 None。
pub fn parse_progress(id: &str, line: &str) -> Option<DownloadProgress> {
    let rest = line.trim().strip_prefix("MMPROG ")?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let number = |index: usize| fields.get(index).and_then(|v| v.parse::<f64>().ok());
    let total = number(1).or(number(2));
    Some(DownloadProgress {
        id: id.to_string(),
        downloaded: number(0).map(|v| v as u64),
        total: total.map(|v| v as u64),
        speed: number(3),
        eta: number(4),
    })
}

/// 下載完成後 yt-dlp 印出的資訊（`--print after_move:`）。
fn parse_done(line: &str) -> Option<serde_json::Value> {
    let rest = line.trim().strip_prefix("MMDONE ")?;
    serde_json::from_str(rest).ok()
}

fn is_partial(name: &str) -> bool {
    name.ends_with(".part")
        || name.ends_with(".ytdl")
        || name.contains(".part-Frag")
        || name.ends_with(".temp")
}

/// 影片檔（不含 json 與下載中的暫存檔）。
fn video_file(videos: &Path, id: &str) -> Option<PathBuf> {
    let prefix = format!("{id}.");
    std::fs::read_dir(videos)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(&prefix)
                && !is_partial(name)
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| {
                        VIDEO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
                    })
        })
}

fn remove_files(videos: &Path, id: &str) {
    let prefix = format!("{id}.");
    if let Ok(entries) = std::fs::read_dir(videos) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

fn meta_path(videos: &Path, id: &str) -> PathBuf {
    videos.join(format!("{id}.json"))
}

/// 已下載的影片；影片檔不見了就當作沒有。
pub fn find_entry(videos: &Path, id: &str) -> Option<VideoEntry> {
    if !valid_id(id) {
        return None;
    }
    let bytes = std::fs::read(meta_path(videos, id)).ok()?;
    let mut entry: VideoEntry = serde_json::from_slice(&bytes).ok()?;
    let path = videos.join(&entry.file);
    let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
    entry.path = path.to_string_lossy().into_owned();
    entry.bytes = meta.len();
    Some(entry)
}

pub fn list(videos: &Path) -> Vec<VideoEntry> {
    let mut entries: Vec<VideoEntry> = std::fs::read_dir(videos)
        .map(|dir| {
            dir.flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let id = name.strip_suffix(".json")?;
                    find_entry(videos, id)
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| b.downloaded_at.cmp(&a.downloaded_at));
    entries
}

pub fn cache_info(videos: &Path) -> CacheInfo {
    let mut info = CacheInfo::default();
    if let Ok(entries) = std::fs::read_dir(videos) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    info.files += 1;
                    info.bytes += meta.len();
                }
            }
        }
    }
    info
}

pub fn delete(videos: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("影片 ID 無效".into());
    }
    remove_files(videos, id);
    Ok(())
}

pub fn clear(videos: &Path) -> Result<CacheInfo, String> {
    let info = cache_info(videos);
    match std::fs::remove_dir_all(videos) {
        Ok(()) => Ok(info),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(info),
        Err(error) => Err(format!("無法清除影片快取：{error}")),
    }
}

/// 下載一部 YouTube 影片。`on_spawn` 拿到程序 ID（取消用），`on_progress` 收到進度。
pub fn download(
    status: &ToolsStatus,
    videos: &Path,
    id: &str,
    hint: &VideoHint,
    cancelled: &AtomicBool,
    on_spawn: impl FnOnce(u32),
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<VideoEntry, String> {
    if !valid_id(id) {
        return Err("影片 ID 無效".into());
    }
    if let Some(entry) = find_entry(videos, id) {
        return Ok(entry);
    }
    std::fs::create_dir_all(videos).map_err(|error| format!("無法建立影片資料夾：{error}"))?;
    // 上次中斷留下的殘檔會被 yt-dlp 當成已下載，先清掉。
    remove_files(videos, id);
    let (mut command, _) = yt_command(status)?;
    command
        .args(["--no-playlist", "--newline", "--progress", "--no-mtime"])
        .args(["-f", FORMAT])
        .arg("-o")
        .arg(videos.join(format!("{id}.%(ext)s")))
        .args([
            "--progress-template",
            "download:MMPROG %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s",
        ])
        .args([
            "--print",
            "after_move:MMDONE %(.{id,title,channel,uploader,duration,fps,width,height,filepath})j",
        ])
        .arg("--")
        .arg(format!("https://www.youtube.com/watch?v={id}"));
    let mut child: Child = command
        .spawn()
        .map_err(|error| format!("無法執行 yt-dlp：{error}"))?;
    on_spawn(child.id());

    // 進度可能印在 stdout 或 stderr（安靜模式），兩邊都讀。
    let (sender, receiver) = mpsc::channel::<String>();
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        let sender = sender.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        let sender = sender.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        }));
    }
    drop(sender);

    let mut done: Option<serde_json::Value> = None;
    let mut errors = String::new();
    for line in receiver {
        if let Some(progress) = parse_progress(id, &line) {
            on_progress(progress);
        } else if let Some(value) = parse_done(&line) {
            done = Some(value);
        } else if line.trim_start().starts_with("ERROR:") {
            errors.push_str(&line);
            errors.push('\n');
        }
    }
    for reader in readers {
        let _ = reader.join();
    }
    let success = child.wait().map(|s| s.success()).unwrap_or(false);
    if cancelled.load(Ordering::Acquire) {
        remove_files(videos, id);
        return Err("已取消下載".into());
    }
    if !success {
        remove_files(videos, id);
        return Err(error_message(&errors, "影片下載失敗"));
    }
    let Some(file) = video_file(videos, id) else {
        remove_files(videos, id);
        return Err("下載完成但找不到影片檔".into());
    };
    let text = |key: &str| {
        done.as_ref()
            .and_then(|v| v.get(key))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && *s != "NA")
            .map(str::to_string)
    };
    let number = |key: &str| {
        done.as_ref()
            .and_then(|v| v.get(key))
            .and_then(|v| v.as_f64())
    };
    let entry = VideoEntry {
        id: id.to_string(),
        title: text("title").unwrap_or_else(|| hint.title.clone()),
        channel: text("channel")
            .or_else(|| text("uploader"))
            .unwrap_or_else(|| hint.channel.clone()),
        duration: number("duration").or(hint.duration),
        fps: number("fps"),
        width: number("width").map(|v| v as u32),
        height: number("height").map(|v| v as u32),
        file: file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: String::new(),
        bytes: 0,
        downloaded_at: now(),
    };
    let json = serde_json::to_vec_pretty(&entry).map_err(|error| error.to_string())?;
    std::fs::write(meta_path(videos, id), json)
        .map_err(|error| format!("無法寫入影片資訊：{error}"))?;
    find_entry(videos, id).ok_or_else(|| "下載完成但讀不到影片資訊".into())
}

// ---------------------------------------------------------------------------
// 工具下載

fn asset_name(tool: Tool) -> Option<&'static str> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    match tool {
        Tool::YtDlp => match (os, arch) {
            ("windows", "x86_64") | ("windows", "x86") => Some("yt-dlp.exe"),
            ("windows", "aarch64") => Some("yt-dlp_arm64.exe"),
            ("macos", _) => Some("yt-dlp_macos"),
            ("linux", "x86_64") => Some("yt-dlp_linux"),
            ("linux", "aarch64") => Some("yt-dlp_linux_aarch64"),
            _ => None,
        },
        Tool::Deno => match (os, arch) {
            ("windows", "x86_64") => Some("deno-x86_64-pc-windows-msvc.zip"),
            ("windows", "aarch64") => Some("deno-aarch64-pc-windows-msvc.zip"),
            ("macos", "x86_64") => Some("deno-x86_64-apple-darwin.zip"),
            ("macos", "aarch64") => Some("deno-aarch64-apple-darwin.zip"),
            ("linux", "x86_64") => Some("deno-x86_64-unknown-linux-gnu.zip"),
            ("linux", "aarch64") => Some("deno-aarch64-unknown-linux-gnu.zip"),
            _ => None,
        },
    }
}

fn tool_url(tool: Tool) -> Option<String> {
    let asset = asset_name(tool)?;
    Some(match tool {
        Tool::YtDlp => format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{asset}"),
        Tool::Deno => format!("https://github.com/denoland/deno/releases/latest/download/{asset}"),
    })
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("無法設定執行權限：{error}"))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), String> {
    Ok(())
}

/// 從 GitHub Releases 下載最新版工具到 tools 目錄（已有的會被取代，也就是更新）。
pub async fn install(
    client: &reqwest::Client,
    base: &Path,
    tool: Tool,
    mut on_progress: impl FnMut(ToolProgress),
) -> Result<(), String> {
    let url = tool_url(tool).ok_or_else(|| "這個平台沒有現成的下載檔，請自行安裝".to_string())?;
    let dir = tools_dir(base);
    std::fs::create_dir_all(&dir).map_err(|error| format!("無法建立工具資料夾：{error}"))?;
    let temp = dir.join(format!(".{}.download", exe(tool_name(tool))));
    let response = client.get(&url).send().await.map_err(|error| {
        if error.is_timeout() {
            "下載逾時，請稍後再試".to_string()
        } else if error.is_connect() {
            "無法連線 GitHub，請檢查網路後再試".to_string()
        } else {
            format!("下載失敗：{error}")
        }
    })?;
    if !response.status().is_success() {
        return Err(format!("下載失敗（HTTP {}）", response.status().as_u16()));
    }
    let total = response.content_length();
    if total.is_some_and(|size| size > TOOL_LIMIT_BYTES) {
        return Err("下載檔超過大小上限".into());
    }
    let mut file =
        std::fs::File::create(&temp).map_err(|error| format!("無法寫入暫存檔：{error}"))?;
    let mut downloaded: u64 = 0;
    let mut reported: u64 = 0;
    let mut response = response;
    on_progress(ToolProgress {
        tool,
        downloaded: 0,
        total,
    });
    let result: Result<(), String> = async {
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("下載中斷：{error}"))?
        {
            downloaded += chunk.len() as u64;
            if downloaded > TOOL_LIMIT_BYTES {
                return Err("下載檔超過大小上限".into());
            }
            file.write_all(&chunk)
                .map_err(|error| format!("無法寫入暫存檔：{error}"))?;
            if downloaded - reported >= 256 * 1024 {
                reported = downloaded;
                on_progress(ToolProgress {
                    tool,
                    downloaded,
                    total,
                });
            }
        }
        file.flush()
            .map_err(|error| format!("無法寫入暫存檔：{error}"))?;
        Ok(())
    }
    .await;
    drop(file);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    on_progress(ToolProgress {
        tool,
        downloaded,
        total: Some(downloaded),
    });
    let target = dir.join(exe(tool_name(tool)));
    let placed = match tool {
        Tool::YtDlp => replace(&temp, &target),
        Tool::Deno => extract(&temp, &target, &exe("deno")),
    };
    let _ = std::fs::remove_file(&temp);
    placed?;
    make_executable(&target)
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::YtDlp => "yt-dlp",
        Tool::Deno => "deno",
    }
}

fn replace(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        std::fs::remove_file(to)
            .map_err(|error| format!("無法取代舊版（可能正在使用中）：{error}"))?;
    }
    std::fs::rename(from, to).map_err(|error| format!("無法放置下載的檔案：{error}"))
}

fn extract(zip_path: &Path, to: &Path, entry_name: &str) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|error| format!("無法開啟壓縮檔：{error}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| format!("壓縮檔損壞：{error}"))?;
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|_| format!("壓縮檔裡沒有 {entry_name}"))?;
    let temp = to.with_extension("extract");
    let mut out = std::fs::File::create(&temp).map_err(|error| format!("無法寫入檔案：{error}"))?;
    std::io::copy(&mut entry, &mut out).map_err(|error| format!("解壓縮失敗：{error}"))?;
    drop(out);
    replace(&temp, to)
}

pub fn remove_tool(base: &Path, tool: Tool) -> Result<(), String> {
    let path = tools_dir(base).join(exe(tool_name(tool)));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("無法刪除（可能正在使用中）：{error}")),
    }
}

/// 本機影片檔：確認存在而且是常見的影片格式。
pub fn local_video(path: &str) -> Result<LocalVideo, String> {
    let path = PathBuf::from(path);
    let meta = std::fs::metadata(&path).map_err(|_| "找不到這個影片檔".to_string())?;
    if !meta.is_file() {
        return Err("這不是檔案".into());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        return Err("不支援的影片格式，請用 mp4 或 webm".into());
    }
    Ok(LocalVideo {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
        bytes: meta.len(),
    })
}

// ---------------------------------------------------------------------------
// 與 Tauri 的接點

pub mod commands {
    use super::*;
    use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

    pub const WINDOW_LABEL: &str = "video";

    pub fn base(app: &AppHandle) -> PathBuf {
        app.path()
            .app_local_data_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("MaiMotionDemo"))
    }

    /// 允許前端以 asset 協定讀取影片資料夾。
    pub fn allow_videos(app: &AppHandle) {
        let dir = videos_dir(&base(app));
        let _ = std::fs::create_dir_all(&dir);
        let _ = app.asset_protocol_scope().allow_directory(&dir, true);
    }

    async fn status(app: &AppHandle, state: &VideoState, refresh: bool) -> ToolsStatus {
        if !refresh {
            if let Some(status) = state.status.lock().ok().and_then(|s| s.clone()) {
                return status;
            }
        }
        let base = base(app);
        let status = tauri::async_runtime::spawn_blocking(move || detect(&base))
            .await
            .unwrap_or(ToolsStatus {
                yt_dlp: None,
                runtime: None,
                installable: false,
            });
        if let Ok(mut slot) = state.status.lock() {
            *slot = Some(status.clone());
        }
        status
    }

    #[tauri::command]
    pub async fn video_open_window(app: AppHandle) -> Result<(), String> {
        if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            return Ok(());
        }
        WebviewWindowBuilder::new(&app, WINDOW_LABEL, WebviewUrl::App("index.html".into()))
            .title("影片同步比對 - MaiMotionDemo")
            .inner_size(1120.0, 720.0)
            .min_inner_size(640.0, 480.0)
            .build()
            .map(|_| ())
            .map_err(|error| format!("無法開啟影片視窗：{error}"))
    }

    #[tauri::command]
    pub async fn video_tools_status(
        app: AppHandle,
        state: tauri::State<'_, VideoState>,
        refresh: bool,
    ) -> Result<ToolsStatus, String> {
        Ok(status(&app, &state, refresh).await)
    }

    #[tauri::command]
    pub async fn video_install_tool(
        app: AppHandle,
        state: tauri::State<'_, VideoState>,
        tool: Tool,
    ) -> Result<ToolsStatus, String> {
        {
            let mut installing = state.installing.lock().map_err(|_| "狀態錯誤")?;
            if installing.contains(&tool) {
                return Err("正在下載中".into());
            }
            installing.push(tool);
        }
        let emitter = app.clone();
        let result = install(&state.client, &base(&app), tool, move |progress| {
            let _ = emitter.emit("video-tool-progress", progress);
        })
        .await;
        if let Ok(mut installing) = state.installing.lock() {
            installing.retain(|item| *item != tool);
        }
        result?;
        Ok(status(&app, &state, true).await)
    }

    #[tauri::command]
    pub async fn video_remove_tool(
        app: AppHandle,
        state: tauri::State<'_, VideoState>,
        tool: Tool,
    ) -> Result<ToolsStatus, String> {
        remove_tool(&base(&app), tool)?;
        Ok(status(&app, &state, true).await)
    }

    #[tauri::command]
    pub async fn video_search(
        app: AppHandle,
        state: tauri::State<'_, VideoState>,
        query: String,
        limit: Option<usize>,
    ) -> Result<Vec<SearchItem>, String> {
        let status = status(&app, &state, false).await;
        let videos = videos_dir(&base(&app));
        tauri::async_runtime::spawn_blocking(move || {
            search(&status, &videos, &query, limit.unwrap_or(12))
        })
        .await
        .map_err(|error| format!("搜尋工作失敗：{error}"))?
    }

    #[tauri::command]
    pub async fn video_download(
        app: AppHandle,
        state: tauri::State<'_, VideoState>,
        id: String,
        hint: Option<VideoHint>,
    ) -> Result<VideoEntry, String> {
        if !valid_id(&id) {
            return Err("影片 ID 無效".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut jobs = state.jobs.lock().map_err(|_| "狀態錯誤")?;
            if jobs.contains_key(&id) {
                return Err("這部影片正在下載".into());
            }
            jobs.insert(
                id.clone(),
                Job {
                    pid: None,
                    cancelled: cancelled.clone(),
                },
            );
        }
        let status = status(&app, &state, false).await;
        let videos = videos_dir(&base(&app));
        let handle = app.clone();
        let job_id = id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let state = handle.state::<VideoState>();
            download(
                &status,
                &videos,
                &job_id,
                &hint.unwrap_or_default(),
                &cancelled,
                |pid| {
                    if let Ok(mut jobs) = state.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id) {
                            job.pid = Some(pid);
                        }
                    }
                },
                |progress| {
                    let _ = handle.emit("video-download-progress", progress);
                },
            )
        })
        .await
        .map_err(|error| format!("下載工作失敗：{error}"));
        if let Ok(mut jobs) = state.jobs.lock() {
            jobs.remove(&id);
        }
        let entry = result??;
        let _ = app.emit("video-library-changed", ());
        Ok(entry)
    }

    #[tauri::command]
    pub fn video_cancel_download(state: tauri::State<'_, VideoState>, id: String) {
        if let Ok(jobs) = state.jobs.lock() {
            if let Some(job) = jobs.get(&id) {
                job.cancelled.store(true, Ordering::Release);
                if let Some(pid) = job.pid {
                    kill_tree(pid);
                }
            }
        }
    }

    #[tauri::command]
    pub fn video_get(app: AppHandle, id: String) -> Option<VideoEntry> {
        find_entry(&videos_dir(&base(&app)), &id)
    }

    #[tauri::command]
    pub fn video_list(app: AppHandle) -> Vec<VideoEntry> {
        list(&videos_dir(&base(&app)))
    }

    #[tauri::command]
    pub fn video_delete(
        app: AppHandle,
        state: tauri::State<'_, VideoState>,
        id: String,
    ) -> Result<(), String> {
        if state
            .jobs
            .lock()
            .map(|jobs| jobs.contains_key(&id))
            .unwrap_or(false)
        {
            return Err("這部影片正在下載，請先取消".into());
        }
        delete(&videos_dir(&base(&app)), &id)?;
        let _ = app.emit("video-library-changed", ());
        Ok(())
    }

    #[tauri::command]
    pub fn video_cache_info(app: AppHandle) -> CacheInfo {
        cache_info(&videos_dir(&base(&app)))
    }

    #[tauri::command]
    pub fn video_clear_cache(
        app: AppHandle,
        state: tauri::State<'_, VideoState>,
    ) -> Result<CacheInfo, String> {
        if state
            .jobs
            .lock()
            .map(|jobs| !jobs.is_empty())
            .unwrap_or(false)
        {
            return Err("還有影片正在下載，請先取消".into());
        }
        let info = clear(&videos_dir(&base(&app)))?;
        allow_videos(&app);
        let _ = app.emit("video-library-changed", ());
        Ok(info)
    }

    /// 本機影片：檢查檔案並允許 asset 協定讀取。
    #[tauri::command]
    pub fn video_open_local(app: AppHandle, path: String) -> Result<LocalVideo, String> {
        let video = local_video(&path)?;
        app.asset_protocol_scope()
            .allow_file(&video.path)
            .map_err(|error| format!("無法開放讀取這個檔案：{error}"))?;
        Ok(video)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_checked_before_use() {
        assert!(valid_id("dQw4w9WgXcQ"));
        assert!(valid_id("a-b_c123456"));
        assert!(!valid_id("short"));
        assert!(!valid_id("--exec=calc"));
        assert!(!valid_id("dQw4w9WgXc/"));
    }

    #[test]
    fn search_json_is_parsed_and_live_streams_skipped() {
        let json = r#"{"entries":[
            {"id":"AAAAAAAAAAA","title":"曲名 MASTER 手元","channel":"someone","duration":130.0,"view_count":1234},
            {"id":"BBBBBBBBBBB","title":"live","uploader":"x","live_status":"is_live"},
            {"id":"bad","title":"broken"},
            {"id":"CCCCCCCCCCC","title":"no channel","uploader":"up"}
        ]}"#;
        let items = parse_search(json, |id| id == "CCCCCCCCCCC").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "曲名 MASTER 手元");
        assert_eq!(items[0].duration, Some(130.0));
        assert_eq!(items[0].views, Some(1234));
        assert!(!items[0].downloaded);
        assert_eq!(items[1].channel, "up");
        assert!(items[1].downloaded);
    }

    #[test]
    fn progress_lines_are_parsed() {
        let p = parse_progress("AAAAAAAAAAA", "MMPROG 1024 NA 4096 512.5 3").unwrap();
        assert_eq!(p.downloaded, Some(1024));
        assert_eq!(p.total, Some(4096));
        assert_eq!(p.speed, Some(512.5));
        assert_eq!(p.eta, Some(3.0));
        let p = parse_progress("AAAAAAAAAAA", "MMPROG 10 20 NA NA NA").unwrap();
        assert_eq!(p.total, Some(20));
        assert_eq!(p.speed, None);
        assert!(parse_progress("AAAAAAAAAAA", "[download] 10%").is_none());
        let done = parse_done(r#"MMDONE {"id":"AAAAAAAAAAA","title":"t","fps":60}"#).unwrap();
        assert_eq!(done["fps"], 60);
    }

    #[test]
    fn js_runtime_flag_only_for_new_yt_dlp() {
        let runtime = RuntimeInfo {
            kind: "deno".into(),
            path: "C:/tools/deno.exe".into(),
            version: "2.5.0".into(),
            managed: true,
        };
        let new = ToolInfo {
            path: "yt".into(),
            version: "2025.12.08".into(),
            managed: true,
        };
        let old = ToolInfo {
            path: "yt".into(),
            version: "2025.09.26".into(),
            managed: false,
        };
        assert_eq!(
            runtime_args(&new, Some(&runtime)),
            ["--js-runtimes", "deno:C:/tools/deno.exe"]
        );
        assert!(runtime_args(&old, Some(&runtime)).is_empty());
        assert!(runtime_args(&new, None).is_empty());
    }

    #[test]
    fn library_lists_deletes_and_clears() {
        let dir = std::env::temp_dir().join(format!(
            "maimotion-video-test-{}-{}",
            std::process::id(),
            now()
        ));
        let videos = videos_dir(&dir);
        std::fs::create_dir_all(&videos).unwrap();
        assert!(list(&videos).is_empty());
        let entry = VideoEntry {
            id: "AAAAAAAAAAA".into(),
            title: "t".into(),
            file: "AAAAAAAAAAA.mp4".into(),
            downloaded_at: 5,
            ..Default::default()
        };
        std::fs::write(videos.join("AAAAAAAAAAA.mp4"), b"1234").unwrap();
        std::fs::write(
            meta_path(&videos, "AAAAAAAAAAA"),
            serde_json::to_vec(&entry).unwrap(),
        )
        .unwrap();
        // 下載中斷的暫存檔不算。
        std::fs::write(videos.join("BBBBBBBBBBB.mp4.part"), b"12").unwrap();
        let found = list(&videos);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes, 4);
        assert!(found[0].path.ends_with("AAAAAAAAAAA.mp4"));
        assert!(video_file(&videos, "BBBBBBBBBBB").is_none());
        assert_eq!(cache_info(&videos).files, 3);
        delete(&videos, "AAAAAAAAAAA").unwrap();
        assert!(find_entry(&videos, "AAAAAAAAAAA").is_none());
        assert_eq!(clear(&videos).unwrap().files, 1);
        assert_eq!(cache_info(&videos).files, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_video_accepts_only_video_files() {
        let dir = std::env::temp_dir().join(format!(
            "maimotion-local-test-{}-{}",
            std::process::id(),
            now()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("clip.MP4");
        std::fs::write(&video, b"xx").unwrap();
        let text = dir.join("note.txt");
        std::fs::write(&text, b"xx").unwrap();
        let ok = local_video(video.to_str().unwrap()).unwrap();
        assert_eq!((ok.name.as_str(), ok.bytes), ("clip.MP4", 2));
        assert!(local_video(text.to_str().unwrap()).is_err());
        assert!(local_video(dir.join("missing.mp4").to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 實際連網：下載 yt-dlp、搜尋並下載一部短片。`cargo test -- --ignored live` 手動執行。
    #[test]
    #[ignore]
    fn live_search_and_download() {
        let base = std::env::temp_dir().join("maimotion-video-live");
        tauri::async_runtime::block_on(async {
            let client = build_client().unwrap();
            if !tools_dir(&base).join(exe("yt-dlp")).exists() {
                install(&client, &base, Tool::YtDlp, |p| {
                    eprintln!("yt-dlp {:?}/{:?}", p.downloaded, p.total)
                })
                .await
                .unwrap();
            }
            if which("deno").is_none()
                && which("node").is_none()
                && !tools_dir(&base).join(exe("deno")).exists()
            {
                install(&client, &base, Tool::Deno, |_| {}).await.unwrap();
            }
        });
        let status = detect(&base);
        eprintln!("{status:?}");
        let videos = videos_dir(&base);
        let items = search(&status, &videos, "maimai 手元 MASTER", 5).unwrap();
        eprintln!("{items:#?}");
        assert!(!items.is_empty());
        let shortest = items
            .iter()
            .filter(|item| item.duration.is_some())
            .min_by(|a, b| a.duration.partial_cmp(&b.duration).unwrap())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let entry = download(
            &status,
            &videos,
            &shortest.id,
            &VideoHint::default(),
            &cancelled,
            |_| {},
            |p| eprintln!("{:?}/{:?}", p.downloaded, p.total),
        )
        .unwrap();
        eprintln!("{entry:#?}");
        assert!(entry.bytes > 0);
    }

    /// 實際連網：下載並解壓 Deno。`cargo test -- --ignored live_install_deno` 手動執行。
    #[test]
    #[ignore]
    fn live_install_deno() {
        let base = std::env::temp_dir().join("maimotion-video-deno");
        tauri::async_runtime::block_on(async {
            install(&build_client().unwrap(), &base, Tool::Deno, |_| {})
                .await
                .unwrap();
        });
        let deno = tools_dir(&base).join(exe("deno"));
        let version = version_of(&deno).unwrap();
        eprintln!("deno {version}");
        assert!(version.starts_with('2'));
        remove_tool(&base, Tool::Deno).unwrap();
        assert!(!deno.exists());
    }

    /// 模擬沒裝 Node.js／Deno 的電腦：PATH 只留系統目錄，從零下載 yt-dlp 與 Deno，
    /// 確認偵測到受管理的 Deno 並用它下載影片。`cargo test -- --ignored live_without_node`。
    #[test]
    #[ignore]
    fn live_without_node_uses_downloaded_deno() {
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        std::env::set_var("PATH", format!(r"{system}\System32;{system}"));
        assert!(which("node").is_none() && which("deno").is_none());
        let base = std::env::temp_dir().join(format!("maimotion-nodeless-{}", now()));
        let before = detect(&base);
        assert!(before.runtime.is_none(), "{before:?}");
        tauri::async_runtime::block_on(async {
            let client = build_client().unwrap();
            install(&client, &base, Tool::YtDlp, |_| {}).await.unwrap();
            install(&client, &base, Tool::Deno, |_| {}).await.unwrap();
        });
        let status = detect(&base);
        eprintln!("{status:?}");
        let runtime = status.runtime.as_ref().unwrap();
        assert_eq!(runtime.kind, "deno");
        assert!(runtime.managed);
        let args = runtime_args(status.yt_dlp.as_ref().unwrap(), Some(runtime));
        assert_eq!(args[0], "--js-runtimes");
        assert!(args[1].starts_with("deno:"));
        let videos = videos_dir(&base);
        let items = search(&status, &videos, "maimai 手元 MASTER", 5).unwrap();
        assert!(!items.is_empty());
        let cancelled = AtomicBool::new(false);
        let entry = download(&status, &videos, &items[0].id, &VideoHint::default(), &cancelled, |_| {}, |_| {}).unwrap();
        eprintln!("{} {}p {} bytes", entry.title, entry.height.unwrap_or(0), entry.bytes);
        assert!(entry.bytes > 0);
        let _ = std::fs::remove_dir_all(&base);
    }
}

