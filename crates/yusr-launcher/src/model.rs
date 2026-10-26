//! The advisor model for the desktop package that ships without it: downloaded in the
//! background into the data folder (a partial file is resumed, after a pause, a dropped
//! connection or a restart), checked against its pinned SHA-256, then run with the bundled
//! `llama-server` and started or stopped from Settings.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use yusr_server::advisor::LocalModel;

use crate::{free_port, llama_args, llama_pid_file, process};

/// Where the model comes from.
#[derive(Debug, Clone)]
pub struct ModelSource {
    pub url: String,
    pub sha256: String,
}

impl ModelSource {
    /// The same file and checksum `app/scripts/fetch-resources.ps1` bundles in the full package.
    pub fn advisor() -> Self {
        Self {
            url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf".into(),
            sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223".into(),
        }
    }
}

/// Retries after a dropped connection before the download reports an error.
const ATTEMPTS: u64 = 5;
/// A stalled connection counts as dropped after this long without a byte.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct ModelManager(Arc<Shared>);

struct Shared {
    bin: PathBuf,
    model: PathBuf,
    data: PathBuf,
    source: ModelSource,
    bind_all: bool,
    mlock: bool,
    fetched: AtomicU64,
    /// Full size once the server has said; 0 while unknown.
    total: AtomicU64,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    server: Option<(Child, u16)>,
    download: Option<tokio::task::AbortHandle>,
    error: Option<String>,
}

impl ModelManager {
    pub fn new(bin: PathBuf, model: PathBuf, data: PathBuf, source: ModelSource, bind_all: bool, mlock: bool) -> Self {
        Self(Arc::new(Shared { bin, model, data, source, bind_all, mlock, fetched: AtomicU64::new(0), total: AtomicU64::new(0), inner: Mutex::default() }))
    }

    /// Starts the model if it is downloaded and the user has not turned it off.
    pub fn autostart(&self) {
        if self.0.model.exists() && !self.0.off_flag().exists() {
            if let Err(e) = self.0.start() {
                tracing::warn!(error = %e, "advisor model server did not start");
            }
        }
    }

    /// On exit: stops the download (the partial file stays) and the model server, without
    /// recording a stop by the user.
    pub fn shutdown(&self) {
        let mut inner = self.0.lock();
        if let Some(h) = inner.download.take() {
            h.abort();
        }
        self.0.kill(&mut inner);
    }
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("model lock")
    }

    fn part(&self) -> PathBuf {
        self.model.with_extension("gguf.part")
    }

    /// Present while the user has stopped the model, so it stays stopped across restarts.
    fn off_flag(&self) -> PathBuf {
        self.data.join("run").join("advisor-off")
    }

    fn start(&self) -> Result<(), String> {
        let mut inner = self.lock();
        if self.running(&mut inner).is_some() {
            return Ok(());
        }
        if !self.model.exists() {
            return Err("the model is not downloaded yet".into());
        }
        process::kill_stale(&llama_pid_file(&self.data));
        let port = free_port().map_err(|e| e.to_string())?;
        let log = std::fs::File::create(self.data.join("logs").join("llama-server.log")).map_err(|e| e.to_string())?;
        let child = process::background(&self.bin, &llama_args(&self.model, port, self.bind_all, self.mlock), log).map_err(|e| format!("{}: {e}", self.bin.display()))?;
        let _ = std::fs::write(llama_pid_file(&self.data), child.id().to_string());
        let _ = std::fs::remove_file(self.off_flag());
        inner.server = Some((child, port));
        inner.error = None;
        Ok(())
    }

    /// The port while the model server is alive; notes it if it has exited on its own.
    fn running(&self, inner: &mut Inner) -> Option<u16> {
        let (child, port) = inner.server.as_mut()?;
        if matches!(child.try_wait(), Ok(None)) {
            return Some(*port);
        }
        inner.server = None;
        inner.error = Some("the model server stopped; see logs/llama-server.log".into());
        None
    }

    fn kill(&self, inner: &mut Inner) {
        if let Some((mut child, _)) = inner.server.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(llama_pid_file(&self.data));
    }
}

impl LocalModel for ModelManager {
    fn endpoint(&self) -> Option<String> {
        let port = self.0.running(&mut self.0.lock())?;
        Some(format!("http://127.0.0.1:{port}/v1"))
    }

    fn status(&self) -> Value {
        let s = &self.0;
        let mut inner = s.lock();
        let downloading = inner.download.as_ref().is_some_and(|h| !h.is_finished());
        let partial = std::fs::metadata(s.part()).map_or(0, |m| m.len());
        let state = if s.running(&mut inner).is_some() {
            "running"
        } else if downloading {
            "downloading"
        } else if s.model.exists() {
            "stopped"
        } else if partial > 0 {
            "paused"
        } else {
            "missing"
        };
        let downloaded = if downloading { s.fetched.load(Ordering::Relaxed) } else { partial };
        json!({ "state": state, "downloaded": downloaded, "total": s.total.load(Ordering::Relaxed), "error": inner.error })
    }

    fn download(&self) -> Result<(), String> {
        let mut inner = self.0.lock();
        if self.0.model.exists() || inner.download.as_ref().is_some_and(|h| !h.is_finished()) {
            return Ok(());
        }
        inner.error = None;
        let shared = self.0.clone();
        let task = tokio::spawn(async move {
            let result = fetch(&shared).await;
            match result {
                Ok(()) => {
                    tracing::info!("advisor model downloaded and verified");
                    if !shared.off_flag().exists() {
                        if let Err(e) = shared.start() {
                            shared.lock().error = Some(e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "advisor model download failed");
                    shared.lock().error = Some(e);
                }
            }
        });
        inner.download = Some(task.abort_handle());
        Ok(())
    }

    fn pause(&self) {
        if let Some(h) = self.0.lock().download.take() {
            h.abort();
        }
    }

    fn start(&self) -> Result<(), String> {
        self.0.start()
    }

    fn stop(&self) {
        let _ = std::fs::write(self.0.off_flag(), "");
        self.0.kill(&mut self.0.lock());
    }
}

async fn fetch(s: &Shared) -> Result<(), String> {
    let part = s.part();
    if let Some(dir) = part.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let http = yusr_server::advisor::client(READ_TIMEOUT);
    for attempt in 1..=ATTEMPTS {
        match fetch_once(s, &http, &part).await {
            Ok(()) => break,
            Err(e) if attempt < ATTEMPTS => {
                tracing::warn!(error = %e, attempt, "model download interrupted; resuming");
                tokio::time::sleep(Duration::from_secs(5 * attempt)).await;
            }
            Err(e) => return Err(e),
        }
    }
    let file = part.clone();
    let got = tokio::task::spawn_blocking(move || sha256_file(&file)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    if !got.eq_ignore_ascii_case(&s.source.sha256) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("the downloaded model is damaged (SHA-256 {got}); download it again"));
    }
    std::fs::rename(&part, &s.model).map_err(|e| format!("{}: {e}", s.model.display()))
}

/// One request continuing from the bytes already on disk.
async fn fetch_once(s: &Shared, http: &reqwest::Client, part: &Path) -> Result<(), String> {
    let have = std::fs::metadata(part).map_or(0, |m| m.len());
    let mut req = http.get(&s.source.url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let mut res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    // The partial file already holds every byte (or more, which the checksum then rejects).
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok(());
    }
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    // A server that ignores the range sends the whole file again.
    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
    let start = if resumed { have } else { 0 };
    s.total.store(res.content_length().map_or(0, |n| start + n), Ordering::Relaxed);
    s.fetched.store(start, Ordering::Relaxed);
    let mut file = std::fs::OpenOptions::new().create(true).append(resumed).write(true).truncate(!resumed).open(part).map_err(|e| e.to_string())?;
    // Blocking writes on purpose: a pause aborts the task only at an await, so the file on
    // disk is always a clean prefix of the model that the next request can continue.
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        s.fetched.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    Ok(())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex::encode(hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(dir: &Path) -> ModelManager {
        ModelManager::new(dir.join("llama-server"), dir.join("models").join("advisor.gguf"), dir.to_path_buf(), ModelSource::advisor(), false, false)
    }

    #[test]
    fn states_follow_the_files() {
        let dir = std::env::temp_dir().join(format!("yusr-model-{}", crate::random_secret(6)));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let m = manager(&dir);
        assert_eq!(m.status()["state"], "missing");
        std::fs::write(m.0.part(), b"abc").unwrap();
        assert_eq!(m.status()["state"], "paused");
        assert_eq!(m.status()["downloaded"], 3);
        std::fs::write(&m.0.model, b"abc").unwrap();
        assert_eq!(m.status()["state"], "stopped");
        assert!(m.endpoint().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Serves `dir` on loopback with HTTP range support, like the model's real host.
    async fn serve(dir: &Path) -> String {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().fallback_service(tower_http::services::ServeDir::new(dir));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/m.gguf")
    }

    async fn finished(m: &ModelManager) -> Value {
        for _ in 0..400 {
            let s = m.status();
            if s["state"] != "downloading" {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the download never finished");
    }

    fn download_case(source: &[u8], partial: &[u8], expected: &[u8]) -> (PathBuf, ModelManager) {
        let dir = std::env::temp_dir().join(format!("yusr-model-{}", crate::random_secret(6)));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::write(dir.join("src").join("m.gguf"), source).unwrap();
        let source = ModelSource { url: String::new(), sha256: hex::encode(Sha256::digest(expected)) };
        let m = ModelManager::new(dir.join("no-llama-server"), dir.join("models").join("advisor.gguf"), dir.clone(), source, false, false);
        std::fs::write(m.0.part(), partial).unwrap();
        (dir, m)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn download_continues_the_partial_file() {
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        // A prefix the server never sent: the result only matches if the request asked for
        // the remaining bytes instead of the whole file.
        let partial = vec![0u8; 120_000];
        let expected = [&partial[..], &body[120_000..]].concat();
        let (dir, mut m) = download_case(&body, &partial, &expected);
        Arc::get_mut(&mut m.0).unwrap().source.url = serve(&dir.join("src")).await;
        m.download().unwrap();
        let s = finished(&m).await;
        assert_eq!(std::fs::read(&m.0.model).unwrap(), expected);
        assert!(!m.0.part().exists());
        // Downloaded and verified; starting fails here only because there is no llama-server.
        assert_eq!(s["state"], "stopped");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn damaged_download_is_discarded() {
        let body = vec![7u8; 50_000];
        let (dir, mut m) = download_case(&body, &body[..10_000], b"something else");
        Arc::get_mut(&mut m.0).unwrap().source.url = serve(&dir.join("src")).await;
        m.download().unwrap();
        let s = finished(&m).await;
        assert_eq!(s["state"], "missing");
        assert!(s["error"].as_str().unwrap().contains("damaged"));
        assert!(!m.0.part().exists() && !m.0.model.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checksum_matches_sha256sum() {
        let dir = std::env::temp_dir().join(format!("yusr-model-{}", crate::random_secret(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("f");
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(sha256_file(&f).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
