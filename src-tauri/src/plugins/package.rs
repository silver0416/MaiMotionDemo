//! 外掛封裝（.mmplugin）的格式、驗證與安裝。
//!
//! 封裝是一個 zip：
//! ```text
//! plugin.json   外掛說明；files 列出每個檔案的 SHA-256 與大小
//! plugin.sig    plugin.json 位元組的 Ed25519 簽章（十六進位）
//! 其餘          files 列出的檔案
//! ```
//! 外掛是會被主程式執行的程式，所以只安裝簽章對得上內建公鑰的封裝；
//! 裝好之後每次啟用前再核對一次簽章與執行檔雜湊。

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const PACKAGE_EXTENSION: &str = "mmplugin";
const MANIFEST_NAME: &str = "plugin.json";
const SIGNATURE_NAME: &str = "plugin.sig";
const FORMAT: u32 = 1;
/// 發布者公鑰（Ed25519）。對應的私鑰不在版本庫裡。
const PUBLISHER_KEY: &str = "5def3bcc6fd83664c9cb6d27b40eb673bb6b05d87968bc3927dbd1e074fbbd9f";

const PACKAGE_LIMIT_BYTES: u64 = 256 * 1024 * 1024;
const MANIFEST_LIMIT_BYTES: u64 = 256 * 1024;
const TOTAL_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FILES: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// 這個平台要執行的檔案（files 裡的名稱）。
    pub entry: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub min_app_version: Option<String>,
    /// 平台（`windows-x86_64`、`linux-x86_64`、`macos-aarch64`…）→ 進入點。
    pub targets: BTreeMap<String, Target>,
    pub files: BTreeMap<String, FileEntry>,
    /// 外掛提供的能力（例如 `adx-source`：adxdls 離線來源）。
    #[serde(default)]
    pub provides: Vec<String>,
}

/// 已安裝、簽章核對過的外掛。
#[derive(Debug, Clone)]
pub struct Installed {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub entry: PathBuf,
}

pub fn current_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub fn publisher_key() -> [u8; 32] {
    unhex(PUBLISHER_KEY)
        .and_then(|bytes| bytes.try_into().ok())
        .expect("publisher key is 32 hex-encoded bytes")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() % 2 != 0 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

/// 外掛代號：小寫英數與連字號，1–40 字，英數開頭。也用作資料夾名稱。
pub fn is_valid_id(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && id.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// 封裝內的檔名：最多四層，每層只有英數、`.`、`_`、`-`，不能是 `.`、`..` 或保留名稱。
fn is_safe_file_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    name.len() <= 200
        && parts.len() <= 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && !part.ends_with('.')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
        && !name.eq_ignore_ascii_case(MANIFEST_NAME)
        && !name.eq_ignore_ascii_case(SIGNATURE_NAME)
}

fn version_parts(version: &str) -> Option<Vec<u64>> {
    let parts: Option<Vec<u64>> = version.split('.').map(|part| part.parse().ok()).collect();
    parts.filter(|parts| (1..=4).contains(&parts.len()))
}

/// 核對簽章後解析並檢查 plugin.json。
pub fn verify_manifest(
    key: &[u8; 32],
    manifest_bytes: &[u8],
    signature_text: &[u8],
    app_version: &str,
) -> Result<Manifest, String> {
    let invalid = || "外掛封裝的簽章不正確，無法安裝".to_string();
    let key = VerifyingKey::from_bytes(key).map_err(|_| invalid())?;
    let signature: [u8; 64] = std::str::from_utf8(signature_text)
        .ok()
        .and_then(unhex)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(invalid)?;
    key.verify(manifest_bytes, &Signature::from_bytes(&signature))
        .map_err(|_| invalid())?;
    let manifest: Manifest = serde_json::from_slice(manifest_bytes)
        .map_err(|error| format!("外掛說明檔格式不正確：{error}"))?;
    validate(&manifest, app_version)?;
    Ok(manifest)
}

fn validate(manifest: &Manifest, app_version: &str) -> Result<(), String> {
    if manifest.format != FORMAT {
        return Err(format!(
            "這個外掛的封裝格式（第 {} 版）需要較新的 MaiMotionDemo",
            manifest.format
        ));
    }
    if !is_valid_id(&manifest.id) {
        return Err("外掛代號格式不正確".into());
    }
    if manifest.name.trim().is_empty() || manifest.name.chars().count() > 60 {
        return Err("外掛名稱格式不正確".into());
    }
    if version_parts(&manifest.version).is_none() {
        return Err("外掛版本格式不正確".into());
    }
    if let Some(minimum) = &manifest.min_app_version {
        let wanted = version_parts(minimum).ok_or("外掛要求的版本格式不正確")?;
        let current = version_parts(app_version).unwrap_or_default();
        if current < wanted {
            return Err(format!("這個外掛需要 MaiMotionDemo {minimum} 以上"));
        }
    }
    if manifest.files.is_empty() || manifest.files.len() > MAX_FILES {
        return Err("外掛的檔案清單不正確".into());
    }
    let mut total: u64 = 0;
    for (name, file) in &manifest.files {
        if !is_safe_file_name(name) {
            return Err(format!("外掛含有不允許的檔名：{name}"));
        }
        if file.sha256.len() != 64 || unhex(&file.sha256).is_none() {
            return Err(format!("外掛檔案 {name} 的雜湊格式不正確"));
        }
        total = total.saturating_add(file.bytes);
    }
    if total > TOTAL_LIMIT_BYTES {
        return Err("外掛超過大小上限".into());
    }
    let target = manifest
        .targets
        .get(&current_target())
        .ok_or_else(|| format!("這個外掛不支援這台電腦（{}）", current_target()))?;
    if !manifest.files.contains_key(&target.entry) {
        return Err("外掛的進入點不在檔案清單裡".into());
    }
    Ok(())
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let entry = archive
        .by_name(name)
        .map_err(|_| format!("外掛封裝缺少 {name}"))?;
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("讀取外掛封裝失敗：{error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("外掛封裝裡的 {name} 超過大小上限"));
    }
    Ok(bytes)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("無法建立外掛資料夾：{error}"))?;
    }
    std::fs::write(path, bytes).map_err(|error| format!("無法寫入外掛檔案：{error}"))
}

fn installed(manifest: Manifest, dir: PathBuf) -> Result<Installed, String> {
    let target = manifest
        .targets
        .get(&current_target())
        .ok_or_else(|| "這個外掛不支援這台電腦".to_string())?;
    let entry = dir.join(target.entry.replace('/', std::path::MAIN_SEPARATOR_STR));
    Ok(Installed {
        manifest,
        dir,
        entry,
    })
}

/// 驗證封裝並安裝到 `plugins_dir/<id>/`；已安裝同代號的外掛會被換掉（外掛自己的資料不動）。
pub fn install(
    package: &Path,
    plugins_dir: &Path,
    key: &[u8; 32],
    app_version: &str,
) -> Result<Installed, String> {
    let size = std::fs::metadata(package)
        .map_err(|error| format!("讀不到外掛封裝：{error}"))?
        .len();
    if size > PACKAGE_LIMIT_BYTES {
        return Err("外掛封裝超過大小上限".into());
    }
    let file = std::fs::File::open(package).map_err(|error| format!("打不開外掛封裝：{error}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| "這不是 MaiMotionDemo 的外掛封裝".to_string())?;
    let manifest_bytes = read_entry(&mut archive, MANIFEST_NAME, MANIFEST_LIMIT_BYTES)?;
    let signature = read_entry(&mut archive, SIGNATURE_NAME, 1024)?;
    let manifest = verify_manifest(key, &manifest_bytes, &signature, app_version)?;

    // 先解到暫存資料夾，全部核對過才換上去。
    let staging = plugins_dir.join(format!(".installing-{}", manifest.id));
    let _ = std::fs::remove_dir_all(&staging);
    let mut unpack = || -> Result<(), String> {
        for (name, expected) in &manifest.files {
            let bytes = read_entry(&mut archive, name, expected.bytes)?;
            if bytes.len() as u64 != expected.bytes
                || hex(&Sha256::digest(&bytes)) != expected.sha256
            {
                return Err(format!("外掛檔案 {name} 與說明檔不符，封裝可能已損壞"));
            }
            write_file(
                &staging.join(name.replace('/', std::path::MAIN_SEPARATOR_STR)),
                &bytes,
            )?;
        }
        write_file(&staging.join(MANIFEST_NAME), &manifest_bytes)?;
        write_file(&staging.join(SIGNATURE_NAME), &signature)
    };
    if let Err(error) = unpack() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    let target = plugins_dir.join(&manifest.id);
    if target.exists() {
        if let Err(error) = std::fs::remove_dir_all(&target) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("無法換掉舊的外掛（可能正在執行）：{error}"));
        }
    }
    std::fs::rename(&staging, &target).map_err(|error| format!("無法安裝外掛：{error}"))?;
    let installed = installed(manifest, target)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&installed.entry, std::fs::Permissions::from_mode(0o755));
    }
    Ok(installed)
}

/// 讀取已安裝的外掛並核對說明檔的簽章。
pub fn load(dir: &Path, key: &[u8; 32], app_version: &str) -> Result<Installed, String> {
    let read = |name: &str, limit: u64| -> Result<Vec<u8>, String> {
        let path = dir.join(name);
        let size = std::fs::metadata(&path)
            .map_err(|error| format!("讀不到外掛的 {name}：{error}"))?
            .len();
        if size > limit {
            return Err(format!("外掛的 {name} 超過大小上限"));
        }
        std::fs::read(&path).map_err(|error| format!("讀不到外掛的 {name}：{error}"))
    };
    let manifest = verify_manifest(
        key,
        &read(MANIFEST_NAME, MANIFEST_LIMIT_BYTES)?,
        &read(SIGNATURE_NAME, 1024)?,
        app_version,
    )?;
    if dir.file_name().and_then(|name| name.to_str()) != Some(manifest.id.as_str()) {
        return Err("外掛資料夾與代號不符".into());
    }
    installed(manifest, dir.to_path_buf())
}

/// 核對執行檔的雜湊；裝好之後檔案被換過就不執行。
pub fn verify_entry(plugin: &Installed) -> Result<(), String> {
    let target = plugin
        .manifest
        .targets
        .get(&current_target())
        .ok_or_else(|| "這個外掛不支援這台電腦".to_string())?;
    let expected = plugin
        .manifest
        .files
        .get(&target.entry)
        .ok_or_else(|| "外掛的進入點不在檔案清單裡".to_string())?;
    let mut file =
        std::fs::File::open(&plugin.entry).map_err(|error| format!("打不開外掛程式：{error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("讀取外掛程式失敗：{error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if hex(&hasher.finalize()) != expected.sha256 {
        return Err("外掛程式與安裝時不同，請重新安裝".into());
    }
    Ok(())
}

/// 所有裝好且核對通過的外掛，依代號排序。壞掉的資料夾略過。
pub fn list(plugins_dir: &Path, key: &[u8; 32], app_version: &str) -> Vec<Installed> {
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return Vec::new();
    };
    let mut plugins: Vec<Installed> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| entry.file_name().to_str().is_some_and(is_valid_id))
        .filter_map(|entry| load(&entry.path(), key, app_version).ok())
        .collect();
    plugins.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    plugins
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::Write;

    pub const APP_VERSION: &str = "1.3.2";

    pub fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "maimotion-plugin-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest_json(id: &str, files: &[(&str, &[u8])], min_app: &str) -> Vec<u8> {
        let files: serde_json::Map<String, serde_json::Value> = files
            .iter()
            .map(|(name, bytes)| {
                (
                    name.to_string(),
                    serde_json::json!({"sha256": hex(&Sha256::digest(bytes)), "bytes": bytes.len()}),
                )
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "format": 1, "id": id, "name": "測試外掛", "version": "1.0.0",
            "minAppVersion": min_app,
            "targets": { current_target(): { "entry": "tool.bin" } },
            "files": files, "provides": ["adx-source"],
        }))
        .unwrap()
    }

    /// 組一個封裝；`packed` 是實際放進 zip 的檔案（可以和 manifest 宣告的不同，用來測竄改）。
    pub fn package(path: &Path, manifest: &[u8], key: &SigningKey, packed: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        let signature = hex(&key.sign(manifest).to_bytes());
        for (name, bytes) in [
            (MANIFEST_NAME, manifest),
            (SIGNATURE_NAME, signature.as_bytes()),
        ]
        .into_iter()
        .chain(packed.iter().copied())
        {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn public(key: &SigningKey) -> [u8; 32] {
        key.verifying_key().to_bytes()
    }

    #[test]
    fn installs_lists_and_replaces() {
        let dir = temp_dir("install");
        let plugins = dir.join("plugins");
        let key = signing_key();
        let files: [(&str, &[u8]); 2] = [("tool.bin", b"program v1"), ("data/readme.txt", b"hi")];
        let package_path = dir.join("a.mmplugin");
        package(
            &package_path,
            &manifest_json("demo", &files, "1.0.0"),
            &key,
            &files,
        );

        let plugin = install(&package_path, &plugins, &public(&key), APP_VERSION).unwrap();
        assert_eq!(plugin.manifest.id, "demo");
        assert_eq!(std::fs::read(&plugin.entry).unwrap(), b"program v1");
        assert_eq!(
            std::fs::read(plugins.join("demo/data/readme.txt")).unwrap(),
            b"hi"
        );
        verify_entry(&plugin).unwrap();
        assert_eq!(list(&plugins, &public(&key), APP_VERSION).len(), 1);
        // 別人的公鑰看不到這個外掛。
        assert!(list(&plugins, &[9u8; 32], APP_VERSION).is_empty());

        // 裝好之後執行檔被換掉：啟用前會被擋下。
        std::fs::write(&plugin.entry, b"something else").unwrap();
        assert!(verify_entry(&plugin).is_err());

        // 再裝一次新版，舊檔案整個換掉。
        let files: [(&str, &[u8]); 1] = [("tool.bin", b"program v2")];
        package(
            &package_path,
            &manifest_json("demo", &files, "1.0.0"),
            &key,
            &files,
        );
        let plugin = install(&package_path, &plugins, &public(&key), APP_VERSION).unwrap();
        assert_eq!(std::fs::read(&plugin.entry).unwrap(), b"program v2");
        assert!(!plugins.join("demo/data").exists());
        assert!(!plugins.join(".installing-demo").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_bad_packages() {
        let dir = temp_dir("reject");
        let plugins = dir.join("plugins");
        let key = signing_key();
        let path = dir.join("a.mmplugin");
        let files: [(&str, &[u8]); 1] = [("tool.bin", b"program")];
        let good = manifest_json("demo", &files, "1.0.0");
        let attempt = |manifest: &[u8], signer: &SigningKey, packed: &[(&str, &[u8])]| {
            package(&path, manifest, signer, packed);
            install(&path, &plugins, &public(&key), APP_VERSION).map(|_| ())
        };

        // 別人簽的。
        assert!(attempt(&good, &SigningKey::from_bytes(&[8u8; 32]), &files)
            .unwrap_err()
            .contains("簽章"));
        // 檔案內容和說明檔不符。
        let swapped: [(&str, &[u8]); 1] = [("tool.bin", b"malware")];
        assert!(attempt(&good, &key, &swapped).unwrap_err().contains("不符"));
        // 需要更新的主程式。
        assert!(
            attempt(&manifest_json("demo", &files, "9.0.0"), &key, &files)
                .unwrap_err()
                .contains("9.0.0")
        );
        // 想寫到外掛資料夾之外。
        let escape: [(&str, &[u8]); 2] = [("tool.bin", b"program"), ("../evil.exe", b"x")];
        assert!(
            attempt(&manifest_json("demo", &escape, "1.0.0"), &key, &escape)
                .unwrap_err()
                .contains("檔名")
        );
        // 代號不合規則。
        assert!(attempt(&manifest_json("../demo", &files, "1.0.0"), &key, &files).is_err());
        // 不是 zip。
        std::fs::write(&path, b"not a zip").unwrap();
        assert!(install(&path, &plugins, &public(&key), APP_VERSION).is_err());

        assert!(list(&plugins, &public(&key), APP_VERSION).is_empty());
        assert!(!plugins.join("evil.exe").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn file_name_rules() {
        for good in ["tool.exe", "bin/tool", "a/b/c/d.txt", "x_y-z.1"] {
            assert!(is_safe_file_name(good), "{good}");
        }
        for bad in [
            "",
            "../x",
            "a/../b",
            "/abs",
            "a//b",
            "C:/x",
            "a\\b",
            "plugin.json",
            "PLUGIN.SIG",
            "a/b/c/d/e",
            "x.",
            "na me",
        ] {
            assert!(!is_safe_file_name(bad), "{bad}");
        }
        assert!(is_valid_id("my-plugin"));
        assert!(!is_valid_id("-x"));
        assert!(!is_valid_id("Adx"));
        assert!(!is_valid_id(""));
    }

    #[test]
    fn publisher_key_decodes() {
        assert_eq!(hex(&publisher_key()), PUBLISHER_KEY);
        assert!(VerifyingKey::from_bytes(&publisher_key()).is_ok());
    }
}
