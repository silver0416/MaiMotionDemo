//! 手元預填模型管理。只發布大小及 SHA-256 都正確的檔案，不在 Rust 執行推論。

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelFileId {
    Det,
    Pose,
    Wasm,
}

#[derive(Clone, Copy, Debug)]
struct ModelFileSpec {
    id: ModelFileId,
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
    bytes: u64,
    zipped: bool,
}

// 與 docs/CONTRACT.md、frontend/src/lib/handcam/models.ts 一起更新。
const MODEL_FILES: [ModelFileSpec; 3] = [
    ModelFileSpec {
        id: ModelFileId::Det,
        name: "yolox_m_8xb8-300e_humanart-c2c7a14a.onnx",
        url: "https://download.openmmlab.com/mmpose/v1/projects/rtmposev1/onnx_sdk/yolox_m_8xb8-300e_humanart-c2c7a14a.zip",
        sha256: "3dea6513388889f0fff4b77bf7a26013600321b9eb9ceb0e9a400a82572f5f23",
        bytes: 101400344,
        zipped: true,
    },
    ModelFileSpec {
        id: ModelFileId::Pose,
        name: "rtmw-dw-x-l_simcc-cocktail14_270e-256x192_20231122.onnx",
        url: "https://download.openmmlab.com/mmpose/v1/projects/rtmw/onnx_sdk/rtmw-dw-x-l_simcc-cocktail14_270e-256x192_20231122.zip",
        sha256: "9a9bc17c13ff0a1e37507f45a037a21bc410cd08374b0e664c57d0080843aa58",
        bytes: 228708578,
        zipped: true,
    },
    ModelFileSpec {
        id: ModelFileId::Wasm,
        name: "ort-wasm-simd-threaded.asyncify.wasm",
        url: "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.29.0/dist/ort-wasm-simd-threaded.asyncify.wasm",
        sha256: "503d17cb7411b79781b9fad1cf0978f03cf06b050c7d399c730e914f473bf549",
        bytes: 25749873,
        zipped: false,
    },
];

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub installed: bool,
    pub files: BTreeMap<ModelFileId, String>,
    /// 僅計入驗證通過的正式檔案，不含 zip、暫存與損壞檔案。
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgress {
    pub file: ModelFileId,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Default)]
struct Cancellation {
    requested: AtomicBool,
    wake: Notify,
}

impl Cancellation {
    fn cancel(&self) {
        self.requested.store(true, Ordering::Release);
        // notify_one 保留通知：不會遺失 check 與 notified 之間的取消。
        self.wake.notify_one();
    }

    fn check(&self) -> Result<(), String> {
        if self.requested.load(Ordering::Acquire) {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }

    async fn wait<T>(
        &self,
        work: impl Future<Output = Result<T, reqwest::Error>>,
    ) -> Result<T, String> {
        self.check()?;
        tokio::select! {
            biased;
            _ = self.wake.notified() => Err("cancelled".into()),
            result = work => {
                self.check()?;
                result.map_err(|error| format!("模型下載失敗：{error}"))
            }
        }
    }
}

enum Operation {
    Install(Arc<Cancellation>),
    Remove,
}

type OperationSlot = Arc<Mutex<Option<Operation>>>;

pub struct HandcamState {
    client: reqwest::Client,
    operation: OperationSlot,
}

impl HandcamState {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            operation: Arc::new(Mutex::new(None)),
        }
    }

    fn begin(&self, operation: Operation) -> Result<OperationGuard, String> {
        let mut slot = self.operation.lock().map_err(|_| "模型狀態錯誤")?;
        if slot.is_some() {
            return Err("模型正在下載或移除，請等待完成後重試".into());
        }
        *slot = Some(operation);
        Ok(OperationGuard(self.operation.clone()))
    }

    fn cancel(&self) -> Result<(), String> {
        let slot = self.operation.lock().map_err(|_| "模型狀態錯誤")?;
        if let Some(Operation::Install(cancel)) = slot.as_ref() {
            cancel.cancel();
        }
        Ok(())
    }
}

struct OperationGuard(OperationSlot);

impl Drop for OperationGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = None;
        }
    }
}

pub fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .https_only(true)
        .user_agent(concat!("MaiMotionDemo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("無法建立模型下載連線：{error}"))
}

fn models_dir(base: &Path) -> PathBuf {
    base.join("models").join("handcam")
}

fn verified(path: &Path, spec: ModelFileSpec, cancel: &Cancellation) -> Result<bool, String> {
    cancel.check()?;
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("無法讀取 {}：{error}", path.display())),
    };
    let meta = file
        .metadata()
        .map_err(|error| format!("無法讀取模型大小：{error}"))?;
    if !meta.is_file() || meta.len() != spec.bytes {
        return Ok(false);
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        cancel.check()?;
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("無法驗證模型：{error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()) == spec.sha256)
}

fn status(
    base: &Path,
    specs: &[ModelFileSpec],
    cancel: &Cancellation,
) -> Result<ModelStatus, String> {
    let dir = models_dir(base);
    let mut result = ModelStatus::default();
    for &spec in specs {
        let path = dir.join(spec.name);
        if verified(&path, spec, cancel)? {
            result.bytes += spec.bytes;
            result
                .files
                .insert(spec.id, path.to_string_lossy().into_owned());
        }
    }
    result.installed = result.files.len() == specs.len();
    Ok(result)
}

/// 在所有正常失敗、取消及工作結束路徑清理暫存檔。
struct Temps {
    download: PathBuf,
    extracted: PathBuf,
}

impl Temps {
    fn new(dir: &Path, name: &str) -> Self {
        Self {
            download: dir.join(format!(".{name}.download")),
            extracted: dir.join(format!(".{name}.extract")),
        }
    }
}

impl Drop for Temps {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.download);
        let _ = fs::remove_file(&self.extracted);
    }
}

fn extract(temps: &Temps, spec: ModelFileSpec, cancel: &Cancellation) -> Result<(), String> {
    let input =
        File::open(&temps.download).map_err(|error| format!("無法開啟模型壓縮檔：{error}"))?;
    let mut zip =
        zip::ZipArchive::new(input).map_err(|error| format!("模型壓縮檔損壞：{error}"))?;
    // 只讀取唯一的 end2end.onnx，不把壓縮檔路徑拿來寫入檔案系統。
    let entries: Vec<_> = zip
        .file_names()
        .filter(|name| name.rsplit('/').next() == Some("end2end.onnx"))
        .map(str::to_owned)
        .collect();
    if entries.len() != 1 {
        return Err("模型壓縮檔必須包含唯一的 end2end.onnx".into());
    }
    let mut entry = zip
        .by_name(&entries[0])
        .map_err(|error| format!("無法讀取模型壓縮內容：{error}"))?;
    if entry.size() != spec.bytes {
        return Err(format!("{} 解壓後大小不符", spec.name));
    }
    let mut output =
        File::create(&temps.extracted).map_err(|error| format!("無法建立模型暫存檔：{error}"))?;
    let mut buffer = [0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        cancel.check()?;
        let count = entry
            .read(&mut buffer)
            .map_err(|error| format!("模型解壓失敗：{error}"))?;
        if count == 0 {
            break;
        }
        written += count as u64;
        if written > spec.bytes {
            return Err("模型解壓超過大小上限".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| format!("無法寫入模型暫存檔：{error}"))?;
    }
    output
        .sync_all()
        .map_err(|error| format!("無法儲存模型暫存檔：{error}"))
}

async fn install_file(
    client: &reqwest::Client,
    dir: &Path,
    spec: ModelFileSpec,
    cancel: Arc<Cancellation>,
    on_progress: &mut impl FnMut(ModelProgress),
) -> Result<(), String> {
    let target = dir.join(spec.name);
    let path = target.clone();
    let flag = cancel.clone();
    if tauri::async_runtime::spawn_blocking(move || verified(&path, spec, &flag))
        .await
        .map_err(|error| format!("模型驗證工作失敗：{error}"))??
    {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|error| format!("無法建立模型資料夾：{error}"))?;
    let temps = Temps::new(dir, spec.name);
    let mut response = cancel.wait(client.get(spec.url).send()).await?;
    if !response.status().is_success() {
        return Err(format!(
            "{} 下載失敗（HTTP {}）",
            spec.name,
            response.status().as_u16()
        ));
    }
    let total = response.content_length();
    // zip 多給 16 MiB 容納封裝與附帶設定；解壓內容另外嚴格限制大小。
    let limit = spec.bytes + if spec.zipped { 16 * 1024 * 1024 } else { 0 };
    if total.is_some_and(|size| size > limit) {
        return Err(format!("{} 下載超過大小上限", spec.name));
    }
    let mut file =
        File::create(&temps.download).map_err(|error| format!("無法建立下載暫存檔：{error}"))?;
    let mut downloaded = 0;
    let mut reported = 0;
    on_progress(ModelProgress {
        file: spec.id,
        downloaded,
        total,
    });
    while let Some(chunk) = cancel.wait(response.chunk()).await? {
        downloaded += chunk.len() as u64;
        if downloaded > limit {
            return Err(format!("{} 下載超過大小上限", spec.name));
        }
        file.write_all(&chunk)
            .map_err(|error| format!("無法寫入下載暫存檔：{error}"))?;
        if downloaded - reported >= 256 * 1024 {
            reported = downloaded;
            on_progress(ModelProgress {
                file: spec.id,
                downloaded,
                total,
            });
        }
    }
    file.sync_all()
        .map_err(|error| format!("無法儲存下載暫存檔：{error}"))?;
    drop(file);
    cancel.check()?;
    on_progress(ModelProgress {
        file: spec.id,
        downloaded,
        total,
    });
    // 大型解壓與雜湊放在 blocking pool；等待完成後才釋放操作鎖。
    tauri::async_runtime::spawn_blocking(move || {
        if spec.zipped {
            extract(&temps, spec, &cancel)?;
        }
        let ready = if spec.zipped {
            &temps.extracted
        } else {
            &temps.download
        };
        if !verified(ready, spec, &cancel)? {
            return Err(format!("{} 大小或 SHA-256 驗證失敗，請重新下載", spec.name));
        }
        cancel.check()?;
        // Windows rename 不覆蓋舊檔；只有新檔驗證通過後才移除舊的損壞檔。
        if target.exists() {
            fs::remove_file(&target).map_err(|error| format!("無法取代模型檔：{error}"))?;
        }
        fs::rename(ready, &target).map_err(|error| format!("無法放置模型檔：{error}"))?;
        cancel.check()
    })
    .await
    .map_err(|error| format!("模型安裝工作失敗：{error}"))?
}

async fn install(
    client: &reqwest::Client,
    base: &Path,
    specs: &[ModelFileSpec],
    cancel: Arc<Cancellation>,
    mut on_progress: impl FnMut(ModelProgress),
) -> Result<ModelStatus, String> {
    for &spec in specs {
        cancel.check()?;
        install_file(
            client,
            &models_dir(base),
            spec,
            cancel.clone(),
            &mut on_progress,
        )
        .await?;
    }
    let base = base.to_path_buf();
    let specs = specs.to_vec();
    tauri::async_runtime::spawn_blocking(move || {
        let result = status(&base, &specs, &cancel)?;
        cancel.check()?;
        Ok(result)
    })
    .await
    .map_err(|error| format!("模型狀態工作失敗：{error}"))?
}

fn remove(base: &Path) -> Result<ModelStatus, String> {
    // 唯一刪除目標固定為 AppLocalData/models/handcam，無前端傳入路徑。
    match fs::remove_dir_all(models_dir(base)) {
        Ok(()) => Ok(ModelStatus::default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ModelStatus::default()),
        Err(error) => Err(format!("無法移除模型資料夾（檔案可能正在使用中）：{error}")),
    }
}

pub mod commands {
    use super::*;
    use tauri::{AppHandle, Emitter, Manager};

    fn base(app: &AppHandle) -> Result<PathBuf, String> {
        app.path()
            .app_local_data_dir()
            .map_err(|error| format!("找不到本機模型資料夾：{error}"))
    }

    #[tauri::command]
    pub async fn handcam_model_status(app: AppHandle) -> Result<ModelStatus, String> {
        let base = base(&app)?;
        tauri::async_runtime::spawn_blocking(move || {
            status(&base, &MODEL_FILES, &Cancellation::default())
        })
        .await
        .map_err(|error| format!("模型狀態工作失敗：{error}"))?
    }

    #[tauri::command]
    pub async fn handcam_model_install(
        app: AppHandle,
        state: tauri::State<'_, HandcamState>,
    ) -> Result<ModelStatus, String> {
        let base = base(&app)?;
        let cancel = Arc::new(Cancellation::default());
        let guard = state.begin(Operation::Install(cancel.clone()))?;
        let client = state.client.clone();
        // 工作擁有 guard，invoke future 被丟棄也不會過早允許下一次 install/remove。
        tauri::async_runtime::spawn(async move {
            let _guard = guard;
            let result = install(&client, &base, &MODEL_FILES, cancel.clone(), |progress| {
                let _ = app.emit("handcam-model-progress", progress);
            })
            .await;
            cancel.check()?;
            result
        })
        .await
        .map_err(|error| format!("模型下載工作失敗：{error}"))?
    }

    #[tauri::command]
    pub fn handcam_model_cancel(state: tauri::State<'_, HandcamState>) -> Result<(), String> {
        state.cancel()
    }

    #[tauri::command]
    pub async fn handcam_model_remove(
        app: AppHandle,
        state: tauri::State<'_, HandcamState>,
    ) -> Result<ModelStatus, String> {
        let base = base(&app)?;
        let guard = state.begin(Operation::Remove)?;
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = guard;
            remove(&base)
        })
        .await
        .map_err(|error| format!("模型移除工作失敗：{error}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Cursor};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    const PAYLOAD: &[u8] = b"abc";
    const TEST_SPEC: ModelFileSpec = ModelFileSpec {
        id: ModelFileId::Det,
        name: "det.onnx",
        url: "http://unused.invalid/",
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        bytes: 3,
        zipped: false,
    };

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "maimotion-handcam-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn archive(names: &[&str]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in names {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(PAYLOAD).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// 真實 loopback HTTP，讓測試走與正式版相同的 reqwest、暫存、解壓及發布流程。
    fn server(reply: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            let _ = stream.write_all(&reply);
        });
        (url, thread)
    }

    fn response(body: &[u8], with_length: bool) -> Vec<u8> {
        let length = if with_length {
            format!("Content-Length: {}\r\n", body.len())
        } else {
            String::new()
        };
        let mut reply =
            format!("HTTP/1.1 200 OK\r\n{length}Connection: close\r\n\r\n").into_bytes();
        reply.extend(body);
        reply
    }

    fn run_download(
        reply: Vec<u8>,
        zipped: bool,
    ) -> (TestDir, Result<ModelStatus, String>, Vec<ModelProgress>) {
        let dir = TestDir::new();
        let (url, thread) = server(reply);
        let mut progress = Vec::new();
        // 每個 test spec 的 URL 只存活在此測試；正式來源固定為 &'static str。
        let spec = ModelFileSpec {
            url: Box::leak(url.into_boxed_str()),
            zipped,
            ..TEST_SPEC
        };
        let result = tauri::async_runtime::block_on(install(
            &reqwest::Client::new(),
            &dir.0,
            &[spec],
            Arc::new(Cancellation::default()),
            |p| progress.push(p),
        ));
        thread.join().unwrap();
        (dir, result, progress)
    }

    #[test]
    fn model_specs_match_frontend_and_asset_config() {
        let frontend = include_str!("../../frontend/src/lib/handcam/models.ts");
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let csp = config["app"]["security"]["csp"].as_str().unwrap();
        assert!(csp.contains("script-src 'self' 'wasm-unsafe-eval'"));
        assert!(csp.contains("connect-src ipc: http://ipc.localhost asset: http://asset.localhost"));
        assert!(config["app"]["security"]["assetProtocol"]["scope"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("$APPLOCALDATA/models/**")));
        for spec in MODEL_FILES {
            let id = serde_json::to_string(&spec.id).unwrap();
            let id = id.trim_matches('"');
            let block = frontend
                .split(&format!("id: '{id}',"))
                .nth(1)
                .unwrap()
                .split("},")
                .next()
                .unwrap();
            assert!(block.contains(spec.name));
            assert!(block.contains(spec.url));
            assert!(block.contains(spec.sha256));
            assert!(block.contains(&format!("bytes: {},", spec.bytes)));
        }
    }

    #[test]
    fn status_omits_corrupt_and_partial_files_and_remove_is_scoped() {
        let dir = TestDir::new();
        let models = models_dir(&dir.0);
        fs::create_dir_all(&models).unwrap();
        let cancel = Cancellation::default();
        fs::write(models.join(TEST_SPEC.name), b"bad").unwrap();
        fs::write(models.join(".det.onnx.download"), PAYLOAD).unwrap();
        let missing = status(&dir.0, &[TEST_SPEC], &cancel).unwrap();
        assert!(!missing.installed);
        assert_eq!(
            serde_json::to_value(missing).unwrap(),
            serde_json::json!({"installed": false, "files": {}, "bytes": 0})
        );
        fs::write(models.join(TEST_SPEC.name), PAYLOAD).unwrap();
        let ready = status(&dir.0, &[TEST_SPEC], &cancel).unwrap();
        assert!(ready.installed);
        assert_eq!(ready.bytes, 3);
        assert!(Path::new(&ready.files[&ModelFileId::Det]).is_absolute());
        fs::write(dir.0.join("models/sibling"), b"keep").unwrap();
        assert!(!remove(&dir.0).unwrap().installed);
        assert!(!models.exists());
        assert!(dir.0.join("models/sibling").exists());
        assert!(!remove(&dir.0).unwrap().installed);
    }

    #[test]
    fn downloads_direct_and_nested_zip_and_reports_wire_bytes() {
        for zipped in [false, true] {
            let body = if zipped {
                archive(&["config.json", "model/end2end.onnx"])
            } else {
                PAYLOAD.to_vec()
            };
            let (dir, result, progress) = run_download(response(&body, true), zipped);
            let result = result.unwrap();
            assert!(result.installed);
            assert_eq!(result.bytes, 3);
            assert_eq!(
                fs::read(models_dir(&dir.0).join(TEST_SPEC.name)).unwrap(),
                PAYLOAD
            );
            assert_eq!(fs::read_dir(models_dir(&dir.0)).unwrap().count(), 1);
            assert_eq!(progress.first().unwrap().downloaded, 0);
            let end = progress.last().unwrap();
            assert_eq!(
                (end.downloaded, end.total),
                (body.len() as u64, Some(body.len() as u64))
            );
            assert_eq!(serde_json::to_value(end).unwrap()["file"], "det");
        }
        let (_, result, progress) = run_download(response(PAYLOAD, false), false);
        assert!(result.unwrap().installed);
        assert!(progress.iter().all(|p| p.total.is_none()));
    }

    #[test]
    fn failed_downloads_are_not_published_and_temps_are_cleaned() {
        let bad_cases = [
            (response(b"bad", true), false, "SHA-256"),
            (response(b"ab", true), false, "大小"),
            (response(b"abcd", false), false, "上限"),
            (
                b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\n\r\n".to_vec(),
                false,
                "HTTP 503",
            ),
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nab".to_vec(),
                false,
                "下載失敗",
            ),
            (response(b"not zip", true), true, "壓縮檔"),
            (
                response(&archive(&["no-model.txt"]), true),
                true,
                "end2end.onnx",
            ),
            (
                response(&archive(&["a/end2end.onnx", "b/end2end.onnx"]), true),
                true,
                "唯一",
            ),
        ];
        for (reply, zipped, expected) in bad_cases {
            let (dir, result, _) = run_download(reply, zipped);
            let error = result.unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert_eq!(
                fs::read_dir(models_dir(&dir.0)).unwrap().count(),
                0,
                "{error}"
            );
        }
    }

    #[test]
    fn valid_files_skip_network_and_corruption_is_repaired() {
        let dir = TestDir::new();
        let models = models_dir(&dir.0);
        fs::create_dir_all(&models).unwrap();
        fs::write(models.join(TEST_SPEC.name), PAYLOAD).unwrap();
        let ready = tauri::async_runtime::block_on(install(
            &reqwest::Client::new(),
            &dir.0,
            &[TEST_SPEC],
            Arc::new(Cancellation::default()),
            |_| panic!("verified file should skip network"),
        ))
        .unwrap();
        assert!(ready.installed);
        fs::write(models.join(TEST_SPEC.name), b"bad").unwrap();
        let (url, thread) = server(response(PAYLOAD, true));
        let spec = ModelFileSpec {
            url: Box::leak(url.into_boxed_str()),
            ..TEST_SPEC
        };
        assert!(
            tauri::async_runtime::block_on(install(
                &reqwest::Client::new(),
                &dir.0,
                &[spec],
                Arc::new(Cancellation::default()),
                |_| {},
            ))
            .unwrap()
            .installed
        );
        thread.join().unwrap();
        assert_eq!(fs::read(models.join(TEST_SPEC.name)).unwrap(), PAYLOAD);
    }

    #[test]
    fn later_failure_keeps_verified_files_for_retry() {
        let dir = TestDir::new();
        let (good_url, good_thread) = server(response(PAYLOAD, true));
        let first = ModelFileSpec {
            url: Box::leak(good_url.into_boxed_str()),
            ..TEST_SPEC
        };
        let (bad_url, bad_thread) = server(response(b"bad", true));
        let second = ModelFileSpec {
            id: ModelFileId::Pose,
            name: "pose.onnx",
            url: Box::leak(bad_url.into_boxed_str()),
            ..TEST_SPEC
        };
        let client = reqwest::Client::new();
        let result = tauri::async_runtime::block_on(install(
            &client,
            &dir.0,
            &[first, second],
            Arc::new(Cancellation::default()),
            |_| {},
        ));
        assert!(result.unwrap_err().contains("SHA-256"));
        good_thread.join().unwrap();
        bad_thread.join().unwrap();
        let partial = status(&dir.0, &[first, second], &Cancellation::default()).unwrap();
        assert!(!partial.installed);
        assert_eq!(partial.bytes, 3);
        assert_eq!(partial.files.len(), 1);
        assert!(partial.files.contains_key(&ModelFileId::Det));
        assert_eq!(fs::read_dir(models_dir(&dir.0)).unwrap().count(), 1);
        let (retry_url, retry_thread) = server(response(PAYLOAD, true));
        let repaired = ModelFileSpec {
            url: Box::leak(retry_url.into_boxed_str()),
            ..second
        };
        let ready = tauri::async_runtime::block_on(install(
            &client,
            &dir.0,
            &[first, repaired],
            Arc::new(Cancellation::default()),
            |p| assert_eq!(p.file, ModelFileId::Pose),
        ))
        .unwrap();
        retry_thread.join().unwrap();
        assert!(ready.installed);
        assert_eq!(ready.bytes, 6);
    }

    #[test]
    fn cancellation_interrupts_stalled_network_and_cleans_temps() {
        let dir = TestDir::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\na")
                .unwrap();
            accepted_tx.send(()).unwrap();
            let _ = stop_rx.recv_timeout(Duration::from_secs(5));
        });
        let cancel = Arc::new(Cancellation::default());
        let signal = cancel.clone();
        let canceller = std::thread::spawn(move || {
            accepted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            signal.cancel();
        });
        let spec = ModelFileSpec {
            url: Box::leak(url.into_boxed_str()),
            ..TEST_SPEC
        };
        let started = std::time::Instant::now();
        let error = tauri::async_runtime::block_on(install(
            &reqwest::Client::new(),
            &dir.0,
            &[spec],
            cancel,
            |_| {},
        ))
        .unwrap_err();
        stop_tx.send(()).unwrap();
        thread.join().unwrap();
        canceller.join().unwrap();
        assert_eq!(error, "cancelled");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(fs::read_dir(models_dir(&dir.0)).unwrap().count(), 0);
    }

    #[test]
    fn cancellation_before_work_and_during_extraction() {
        let dir = TestDir::new();
        let cancel = Arc::new(Cancellation::default());
        cancel.cancel();
        let result = tauri::async_runtime::block_on(install(
            &reqwest::Client::new(),
            &dir.0,
            &[TEST_SPEC],
            cancel.clone(),
            |_| panic!("cancelled"),
        ));
        assert_eq!(result.unwrap_err(), "cancelled");
        let temps = Temps::new(&dir.0, "model");
        fs::write(&temps.download, archive(&["nested/end2end.onnx"])).unwrap();
        assert_eq!(
            extract(&temps, TEST_SPEC, &cancel).unwrap_err(),
            "cancelled"
        );
        drop(temps);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    }

    #[test]
    fn install_and_remove_are_mutually_exclusive_until_worker_finishes() {
        let state = HandcamState::new(reqwest::Client::new());
        state.cancel().unwrap();
        let cancel = Arc::new(Cancellation::default());
        let guard = state.begin(Operation::Install(cancel.clone())).unwrap();
        assert!(state
            .begin(Operation::Install(Arc::new(Cancellation::default())))
            .is_err());
        assert!(state.begin(Operation::Remove).is_err());
        state.cancel().unwrap();
        assert_eq!(cancel.check().unwrap_err(), "cancelled");
        assert!(state.begin(Operation::Remove).is_err());
        drop(guard);
        let guard = state.begin(Operation::Remove).unwrap();
        assert!(state
            .begin(Operation::Install(Arc::new(Cancellation::default())))
            .is_err());
        drop(guard);
        assert!(state.begin(Operation::Remove).is_ok());
    }

    /// 真正從三個固定 HTTPS 來源下載、解壓與驗證；使用獨立暫存目錄，結束清除。
    /// cargo test --manifest-path src-tauri/Cargo.toml live_handcam_models -- --ignored --nocapture
    #[test]
    #[ignore = "downloads approximately 340 MB from the public model hosts"]
    fn live_handcam_models() {
        let dir = TestDir::new();
        let mut last_file = None;
        let mut last_reported = 0;
        let result = tauri::async_runtime::block_on(install(
            &build_client().unwrap(),
            &dir.0,
            &MODEL_FILES,
            Arc::new(Cancellation::default()),
            |p| {
                if last_file != Some(p.file)
                    || p.downloaded - last_reported >= 16 * 1024 * 1024
                    || p.total == Some(p.downloaded)
                {
                    eprintln!("{:?}: {} / {:?}", p.file, p.downloaded, p.total);
                    last_file = Some(p.file);
                    last_reported = p.downloaded;
                }
            },
        ))
        .unwrap();
        assert!(result.installed);
        assert_eq!(result.files.len(), 3);
        assert_eq!(result.bytes, 355858795);
        eprintln!("verified: {result:?}");
        let again = tauri::async_runtime::block_on(install(
            &build_client().unwrap(),
            &dir.0,
            &MODEL_FILES,
            Arc::new(Cancellation::default()),
            |_| panic!("all files should skip network"),
        ))
        .unwrap();
        assert!(again.installed);
    }
}
