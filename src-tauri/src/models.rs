//! Model catalog (from Hugging Face), local model files and downloads.

use crate::Core;
use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::Emitter;

const HF_REPO: &str = "ggerganov/whisper.cpp";
const RECOMMENDED: &[&str] = &["ggml-large-v3-turbo-q5_0.bin", "ggml-large-v3-turbo.bin"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteModel {
    pub file: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Default)]
struct Download {
    received: AtomicU64,
    total: AtomicU64,
    cancel: AtomicBool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub file: String,
    pub received: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub file: String,
    pub family: String,
    pub variant: String,
    pub english_only: bool,
    pub size: u64,
    pub downloaded: bool,
    pub in_catalog: bool,
    pub recommended: bool,
    pub keep_loaded: bool,
    pub download: Option<DownloadProgress>,
    pub runtime: crate::engine::ModelRuntime,
}

pub struct ModelStore {
    cache_path: PathBuf,
    catalog: Mutex<Vec<RemoteModel>>,
    downloads: Mutex<HashMap<String, Arc<Download>>>,
}

impl ModelStore {
    pub fn new(cache_path: PathBuf) -> Self {
        let catalog = fs::read(&cache_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(builtin_catalog);
        Self {
            cache_path,
            catalog: Mutex::new(catalog),
            downloads: Mutex::new(HashMap::new()),
        }
    }

    pub fn cancel_download(&self, file: &str) {
        if let Some(d) = self.downloads.lock().unwrap().get(file) {
            d.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn remote(&self, file: &str) -> Option<RemoteModel> {
        self.catalog
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.file == file)
            .cloned()
    }
}

/// Everything the Models page shows: catalog entries plus any local `.bin` files.
pub fn list(core: &Core) -> Vec<ModelInfo> {
    let dir = core.models_dir();
    let keep = core.config.get().keep_loaded;
    let mut local: HashMap<String, u64> = HashMap::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".bin") && entry.path().is_file() {
                local.insert(name, entry.metadata().map(|m| m.len()).unwrap_or(0));
            }
        }
    }
    let downloads = core.models.downloads.lock().unwrap();
    let catalog = core.models.catalog.lock().unwrap().clone();

    let mut files: Vec<(String, u64, bool)> = catalog
        .iter()
        .map(|m| (m.file.clone(), m.size, true))
        .collect();
    for (name, size) in &local {
        if !catalog.iter().any(|m| &m.file == name) {
            files.push((name.clone(), *size, false));
        }
    }

    let mut models: Vec<ModelInfo> = files
        .into_iter()
        .map(|(file, size, in_catalog)| {
            let (family, variant, english_only) = describe(&file);
            ModelInfo {
                downloaded: local.contains_key(&file),
                size: local.get(&file).copied().unwrap_or(size),
                download: downloads.get(&file).map(|d| DownloadProgress {
                    file: file.clone(),
                    received: d.received.load(Ordering::Relaxed),
                    total: d.total.load(Ordering::Relaxed),
                }),
                runtime: core.engine.runtime(&file),
                keep_loaded: keep.contains(&file),
                recommended: RECOMMENDED.contains(&file.as_str()),
                in_catalog,
                family,
                variant,
                english_only,
                file,
            }
        })
        .collect();
    models.sort_by_key(|m| (family_rank(&m.family), m.english_only, m.variant.clone()));
    models
}

/// Split `ggml-large-v3-turbo-q5_0.bin` into (`large-v3-turbo`, `q5_0`, english_only=false).
fn describe(file: &str) -> (String, String, bool) {
    let stem = file
        .strip_prefix("ggml-")
        .unwrap_or(file)
        .strip_suffix(".bin")
        .unwrap_or(file);
    let english_only = stem.contains(".en");
    let stem = stem.replace(".en", "");
    let mut parts: Vec<&str> = stem.split('-').collect();
    let mut variant = String::new();
    if let Some(last) = parts.last()
        && (last.starts_with('q') && last.contains('_') || *last == "tdrz")
    {
        variant = last.to_string();
        parts.pop();
    }
    (parts.join("-"), variant, english_only)
}

fn family_rank(family: &str) -> (usize, String) {
    let order = [
        "tiny",
        "base",
        "small",
        "medium",
        "large-v3-turbo",
        "large-v3",
        "large-v2",
        "large-v1",
        "large",
    ];
    let rank = order
        .iter()
        .position(|f| *f == family)
        .unwrap_or(order.len());
    (rank, family.to_string())
}

/// Used until the first successful catalog fetch. Sizes are approximate.
fn builtin_catalog() -> Vec<RemoteModel> {
    const MB: u64 = 1_000_000;
    [
        ("ggml-tiny.bin", 78),
        ("ggml-tiny.en.bin", 78),
        ("ggml-base.bin", 148),
        ("ggml-base.en.bin", 148),
        ("ggml-small.bin", 488),
        ("ggml-small.en.bin", 488),
        ("ggml-medium.bin", 1533),
        ("ggml-medium.en.bin", 1533),
        ("ggml-large-v3.bin", 3095),
        ("ggml-large-v3-q5_0.bin", 1081),
        ("ggml-large-v3-turbo.bin", 1624),
        ("ggml-large-v3-turbo-q5_0.bin", 574),
        ("ggml-large-v3-turbo-q8_0.bin", 874),
    ]
    .into_iter()
    .map(|(file, mb)| RemoteModel {
        file: file.into(),
        size: mb * MB,
        sha256: None,
    })
    .collect()
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("win-whisper/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()?)
}

/// Fetch the file list of the whisper.cpp model repo on Hugging Face.
pub async fn refresh_catalog(core: &Core) -> Result<()> {
    #[derive(Deserialize)]
    struct Lfs {
        oid: String,
    }
    #[derive(Deserialize)]
    struct Entry {
        #[serde(rename = "type")]
        kind: String,
        path: String,
        size: u64,
        lfs: Option<Lfs>,
    }
    let url = format!("https://huggingface.co/api/models/{HF_REPO}/tree/main");
    let entries: Vec<Entry> = client()?
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("reading the model list")?;
    let catalog: Vec<RemoteModel> = entries
        .into_iter()
        .filter(|e| {
            e.kind == "file"
                && e.path.starts_with("ggml-")
                && e.path.ends_with(".bin")
                && !e.path.contains('/')
                && !e.path.contains("for-tests")
        })
        .map(|e| RemoteModel {
            sha256: e.lfs.map(|l| l.oid),
            size: e.size,
            file: e.path,
        })
        .collect();
    if catalog.is_empty() {
        bail!("the model list came back empty");
    }
    let _ = fs::write(&core.models.cache_path, serde_json::to_vec(&catalog)?);
    *core.models.catalog.lock().unwrap() = catalog;
    core.notify();
    Ok(())
}

fn valid_file_name(file: &str) -> bool {
    file.ends_with(".bin")
        && !file.contains(['/', '\\', ':'])
        && !file.starts_with('.')
        && !file.contains("..")
}

pub async fn download(core: Arc<Core>, file: String) -> Result<()> {
    if !valid_file_name(&file) {
        bail!("invalid model name {file}");
    }
    let state = Arc::new(Download::default());
    {
        let mut downloads = core.models.downloads.lock().unwrap();
        if downloads.contains_key(&file) {
            bail!("{file} is already downloading");
        }
        downloads.insert(file.clone(), state.clone());
    }
    core.notify();
    let result = download_inner(&core, &file, &state).await;
    core.models.downloads.lock().unwrap().remove(&file);
    core.notify();
    match result {
        Err(_) if state.cancel.load(Ordering::Relaxed) => Ok(()),
        other => other,
    }
}

async fn download_inner(core: &Core, file: &str, state: &Download) -> Result<()> {
    let dir = core.models_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let target = dir.join(file);
    let part = dir.join(format!("{file}.part"));
    let remote = core.models.remote(file);

    let mut existing = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let url = format!("https://huggingface.co/{HF_REPO}/resolve/main/{file}");
    let mut request = client()?.get(&url);
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = request.send().await?.error_for_status()?;
    let resuming = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resuming {
        existing = 0;
    }
    let total = response
        .content_length()
        .map(|len| len + existing)
        .or(remote.as_ref().map(|r| r.size))
        .unwrap_or(0);
    state.total.store(total, Ordering::Relaxed);
    state.received.store(existing, Ordering::Relaxed);

    let mut hasher = Sha256::new();
    if resuming {
        let mut reader = File::open(&part)?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(&part)
        .with_context(|| format!("opening {}", part.display()))?;

    let mut received = existing;
    let mut last_emit = Instant::now();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if state.cancel.load(Ordering::Relaxed) {
            return Err(anyhow!("cancelled"));
        }
        let chunk = chunk.context("downloading")?;
        out.write_all(&chunk)?;
        hasher.update(&chunk);
        received += chunk.len() as u64;
        state.received.store(received, Ordering::Relaxed);
        if last_emit.elapsed() > Duration::from_millis(200) {
            last_emit = Instant::now();
            let _ = core.app.emit(
                "download-progress",
                DownloadProgress {
                    file: file.to_string(),
                    received,
                    total,
                },
            );
        }
    }
    out.flush()?;
    drop(out);

    if let Some(expected) = remote.and_then(|r| r.sha256) {
        let actual: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if !actual.eq_ignore_ascii_case(&expected) {
            let _ = fs::remove_file(&part);
            bail!("{file} failed its checksum check; the partial file was deleted, try again");
        }
    }
    fs::rename(&part, &target).with_context(|| format!("saving {}", target.display()))?;
    log::info!("downloaded {file}");
    Ok(())
}

pub fn delete(core: &Core, file: &str) -> Result<()> {
    if !valid_file_name(file) {
        bail!("invalid model name {file}");
    }
    core.engine.unload(file);
    let dir = core.models_dir();
    for path in [dir.join(file), dir.join(format!("{file}.part"))] {
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("deleting {}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_model_files() {
        assert_eq!(
            describe("ggml-large-v3-turbo-q5_0.bin"),
            ("large-v3-turbo".into(), "q5_0".into(), false)
        );
        assert_eq!(
            describe("ggml-base.en.bin"),
            ("base".into(), "".into(), true)
        );
        assert_eq!(
            describe("ggml-small.en-tdrz.bin"),
            ("small".into(), "tdrz".into(), true)
        );
        assert_eq!(
            describe("my-custom.bin"),
            ("my-custom".into(), "".into(), false)
        );
    }

    #[test]
    fn rejects_path_like_names() {
        assert!(valid_file_name("ggml-base.bin"));
        assert!(!valid_file_name("../evil.bin"));
        assert!(!valid_file_name("C:\\x.bin"));
        assert!(!valid_file_name("ggml-base.exe"));
    }
}
