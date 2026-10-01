use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Weak};
use std::time::{Duration, Instant};

use futures::{StreamExt as _, stream};
use reqwest::StatusCode;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt as _, BufWriter};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::events::{Event, Reporter};
use crate::features::download::hls::segment_urls;
use crate::features::download::library::{ArchiveEntry, Library};
use crate::features::download::models::{CompletedDownload, DownloadJob};
use crate::features::download::tagging::apply_tags;
use crate::features::download::validation::is_valid_audio;
use crate::shared::config::{
    DOWNLOAD_ATTEMPTS, DOWNLOAD_BACKOFF_BASE, DOWNLOAD_BACKOFF_CAP, HLS_SEGMENT_CONCURRENCY,
    MAX_ARTWORK_BYTES, MAX_AUDIO_BYTES, PROGRESS_INTERVAL,
};
use crate::shared::error::{Error, Result};
use crate::shared::filesystem::ensure_within;
use crate::shared::http::{ensure_success, read_limited};
use crate::shared::retry::{RetryPolicy, retry};
use crate::shared::soundcloud::client::SoundCloudClient;

static FILE_LOCKS: LazyLock<std::sync::Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn file_lock(path: &Path) -> Arc<Mutex<()>> {
    let key = PathBuf::from(path.to_string_lossy().to_lowercase());
    let mut locks = FILE_LOCKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    locks.retain(|_, lock| lock.strong_count() > 0);
    let lock = Arc::new(Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

const WRITE_BUFFER: usize = 256 * 1024;
const SEGMENT_POLICY: RetryPolicy = RetryPolicy {
    attempts: 3,
    base: Duration::from_millis(500),
    cap: Duration::from_secs(5),
};

struct PartFile {
    path: PathBuf,
    committed: bool,
    keep: bool,
}

impl PartFile {
    fn path_for(final_path: &Path) -> PathBuf {
        let mut name = final_path.file_name().unwrap_or_default().to_os_string();
        name.push(".part");
        final_path.with_file_name(name)
    }

    fn new(final_path: &Path) -> Self {
        Self {
            path: Self::path_for(final_path),
            committed: false,
            keep: false,
        }
    }
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if !self.committed && !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub struct RateLimiter {
    bytes_per_second: f64,
    state: Mutex<(Instant, f64)>,
}

impl RateLimiter {
    pub fn new(kbps: u32) -> Self {
        let bytes_per_second = f64::from(kbps) * 1000.0 / 8.0;
        Self {
            bytes_per_second,
            state: Mutex::new((Instant::now(), bytes_per_second)),
        }
    }

    pub async fn consume(&self, bytes: usize) {
        let wait = {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let refill = now.duration_since(state.0).as_secs_f64() * self.bytes_per_second;
            state.0 = now;
            state.1 = (state.1 + refill).min(self.bytes_per_second) - bytes as f64;
            (state.1 < 0.0).then(|| Duration::from_secs_f64(-state.1 / self.bytes_per_second))
        };
        if let Some(wait) = wait {
            tokio::time::sleep(wait).await;
        }
    }
}

struct Progress<'a> {
    reporter: &'a Reporter,
    track_id: i64,
    last: Option<Instant>,
}

impl Progress<'_> {
    fn update(&mut self, bytes: u64, fraction: Option<f64>) {
        if self
            .last
            .is_some_and(|last| last.elapsed() < PROGRESS_INTERVAL)
        {
            return;
        }
        self.last = Some(Instant::now());
        self.reporter.emit(&Event::TrackProgress {
            track_id: self.track_id,
            bytes,
            fraction: fraction.map(|f| f.clamp(0.0, 1.0)),
        });
    }
}

fn reject_non_audio(response: &reqwest::Response) -> Result<()> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if content_type.starts_with("text/") || content_type.contains("json") {
        return Err(Error::Integrity(format!(
            "CDN respondeu '{content_type}' em vez de audio"
        )));
    }
    Ok(())
}

fn content_range_start(response: &reqwest::Response) -> Option<u64> {
    let value = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    value
        .strip_prefix("bytes ")?
        .split('-')
        .next()?
        .trim()
        .parse()
        .ok()
}

async fn blocking<T: Send + 'static>(task: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|err| Error::Download(format!("tarefa interrompida: {err}")))
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub struct AudioFetcher {
    client: Arc<SoundCloudClient>,
    reporter: Reporter,
    cancel: CancellationToken,
    library: Arc<Library>,
    limiter: Option<Arc<RateLimiter>>,
}

impl AudioFetcher {
    pub fn new(
        client: Arc<SoundCloudClient>,
        reporter: Reporter,
        cancel: CancellationToken,
        library: Arc<Library>,
        limiter: Option<Arc<RateLimiter>>,
    ) -> Self {
        Self {
            client,
            reporter,
            cancel,
            library,
            limiter,
        }
    }

    pub async fn download(
        &self,
        job: &DownloadJob,
        archived: Option<ArchiveEntry>,
    ) -> Result<CompletedDownload> {
        let started = Instant::now();
        tokio::fs::create_dir_all(&job.output_dir).await?;
        let file_name = job.naming.file_name(job.format.extension);
        let final_path = ensure_within(&job.output_dir, &job.output_dir.join(file_name))?;
        let lock = file_lock(&final_path);
        let _guard = lock.lock().await;

        if let Some(reused) = self.reuse_existing(job, archived).await? {
            return Ok(reused);
        }
        if let Some(copied) = self.copy_from_library(job, &final_path).await? {
            return Ok(copied);
        }

        let attempts_used = AtomicU32::new(0);
        let final_path = &final_path;
        let on_retry = |attempt: u32, err: &Error, delay: Duration| {
            self.reporter.emit(&Event::TrackRetry {
                track_id: job.track_id,
                title: job.title.clone(),
                attempt,
                max: DOWNLOAD_ATTEMPTS,
                reason: err.to_string(),
                delay_s: delay.as_secs_f64(),
            });
        };
        let policy = RetryPolicy {
            attempts: DOWNLOAD_ATTEMPTS,
            base: DOWNLOAD_BACKOFF_BASE,
            cap: DOWNLOAD_BACKOFF_CAP,
        };
        let outcome = retry(&policy, &self.cancel, on_retry, |attempt| {
            attempts_used.store(attempt, Ordering::Relaxed);
            self.transfer(job, final_path)
        })
        .await;
        if outcome.is_err() {
            let _ = tokio::fs::remove_file(PartFile::path_for(final_path)).await;
        }
        outcome?;
        let attempts_used = attempts_used.load(Ordering::Relaxed);

        self.tag(job, final_path).await?;
        self.library.record(job.track_id, final_path);
        let size = tokio::fs::metadata(final_path).await?.len();
        let elapsed = started.elapsed().as_secs_f64();
        self.reporter.emit(&Event::TrackDownloaded {
            track_id: job.track_id,
            file: file_name_of(final_path),
            bytes: size,
            attempts: attempts_used,
            elapsed_s: (elapsed * 1000.0).round() / 1000.0,
            protocol: job.protocol.clone(),
        });
        Ok(CompletedDownload {
            track_id: job.track_id,
            title: job.title.clone(),
            path: final_path.display().to_string(),
            size_bytes: size,
            reused: false,
            from_library: false,
            attempts: attempts_used,
            elapsed_seconds: elapsed,
        })
    }

    async fn tag(&self, job: &DownloadJob, path: &Path) -> Result<()> {
        let mut artwork = self.fetch_artwork(job.artwork_url.as_deref()).await;
        if artwork.is_none() {
            artwork = self
                .fetch_artwork(job.artwork_fallback_url.as_deref())
                .await;
        }
        let (tag_path, tag_job) = (path.to_path_buf(), job.clone());
        if let Err(reason) = blocking(move || apply_tags(&tag_path, &tag_job, artwork)).await? {
            tracing::warn!(track_id = job.track_id, %reason, "metadados nao aplicados");
        }
        Ok(())
    }

    async fn reuse_existing(
        &self,
        job: &DownloadJob,
        archived: Option<ArchiveEntry>,
    ) -> Result<Option<CompletedDownload>> {
        let (dir, format, naming) = (job.output_dir.clone(), job.format, job.naming.clone());
        let found = blocking(move || {
            let from_archive = archived.map(|entry| dir.join(entry.file)).filter(|path| {
                path.is_file()
                    && path.extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case(format.extension.trim_start_matches('.'))
                    })
            });
            from_archive
                .or_else(|| naming.find_existing(&dir, format.extension))
                .map(|path| {
                    let valid = is_valid_audio(&path, format);
                    (path, valid)
                })
        })
        .await?;
        let Some((existing, valid)) = found else {
            return Ok(None);
        };
        if !valid {
            tracing::warn!(track_id = job.track_id, file = %existing.display(), "arquivo existente corrompido, baixando novamente");
            tokio::fs::remove_file(&existing).await?;
            return Ok(None);
        }
        let size = tokio::fs::metadata(&existing).await?.len();
        self.library.record(job.track_id, &existing);
        self.reporter.emit(&Event::TrackReused {
            track_id: job.track_id,
            file: file_name_of(&existing),
        });
        Ok(Some(CompletedDownload {
            track_id: job.track_id,
            title: job.title.clone(),
            path: existing.display().to_string(),
            size_bytes: size,
            reused: true,
            from_library: false,
            attempts: 0,
            elapsed_seconds: 0.0,
        }))
    }

    async fn copy_from_library(
        &self,
        job: &DownloadJob,
        final_path: &Path,
    ) -> Result<Option<CompletedDownload>> {
        let Some(source) = self.library.lookup(job.track_id) else {
            return Ok(None);
        };
        let same_format = source.extension() == final_path.extension();
        if !same_format || source == final_path {
            return Ok(None);
        }
        let (check, format) = (source.clone(), job.format);
        if !blocking(move || is_valid_audio(&check, format)).await? {
            return Ok(None);
        }
        let mut part = PartFile::new(final_path);
        tokio::fs::copy(&source, &part.path).await?;
        tokio::fs::rename(&part.path, final_path).await?;
        part.committed = true;
        self.tag(job, final_path).await?;
        self.library.record(job.track_id, final_path);
        let size = tokio::fs::metadata(final_path).await?.len();
        self.reporter.emit(&Event::TrackCopied {
            track_id: job.track_id,
            file: file_name_of(final_path),
            source: source.display().to_string(),
        });
        Ok(Some(CompletedDownload {
            track_id: job.track_id,
            title: job.title.clone(),
            path: final_path.display().to_string(),
            size_bytes: size,
            reused: true,
            from_library: true,
            attempts: 0,
            elapsed_seconds: 0.0,
        }))
    }

    async fn transfer(&self, job: &DownloadJob, final_path: &Path) -> Result<()> {
        let stream_url = self
            .client
            .stream_url(&job.transcoding_url, job.track_authorization.as_deref())
            .await?;
        let mut part = PartFile::new(final_path);
        let mut progress = Progress {
            reporter: &self.reporter,
            track_id: job.track_id,
            last: None,
        };
        progress.update(0, Some(0.0));

        if job.protocol == "progressive" {
            self.fetch_progressive(&stream_url, &mut part, &mut progress)
                .await?;
        } else {
            let mut writer =
                BufWriter::with_capacity(WRITE_BUFFER, File::create(&part.path).await?);
            self.fetch_hls(&stream_url, &mut writer, &mut progress)
                .await?;
            writer.flush().await?;
            let file = writer.into_inner();
            file.sync_all().await?;
        }

        let (check_path, format) = (part.path.clone(), job.format);
        if !blocking(move || is_valid_audio(&check_path, format)).await? {
            return Err(Error::Integrity(
                "arquivo baixado falhou na validacao de integridade".into(),
            ));
        }
        tokio::fs::rename(&part.path, final_path).await?;
        part.committed = true;
        Ok(())
    }

    async fn fetch_progressive(
        &self,
        url: &str,
        part: &mut PartFile,
        progress: &mut Progress<'_>,
    ) -> Result<()> {
        let resume_from = tokio::fs::metadata(&part.path)
            .await
            .map_or(0, |meta| meta.len());
        let response = self.client.http().get_from(url, resume_from).await?;
        let resumed = resume_from > 0
            && response.status() == StatusCode::PARTIAL_CONTENT
            && content_range_start(&response) == Some(resume_from);
        let mut response = ensure_success(response, "CDN")?;
        reject_non_audio(&response)?;
        let offset = if resumed { resume_from } else { 0 };
        if resumed {
            tracing::info!(offset, "retomando download interrompido");
        }
        let expected = response
            .content_length()
            .filter(|len| *len > 0)
            .map(|len| len + offset);
        if expected.is_some_and(|len| len > MAX_AUDIO_BYTES) {
            return Err(Error::Download(format!(
                "audio excede o limite de {MAX_AUDIO_BYTES} bytes"
            )));
        }
        let file = if resumed {
            OpenOptions::new().append(true).open(&part.path).await?
        } else {
            File::create(&part.path).await?
        };
        let mut writer = BufWriter::with_capacity(WRITE_BUFFER, file);
        let mut written = offset;
        let streamed: Result<()> = async {
            while let Some(chunk) = response.chunk().await? {
                written += chunk.len() as u64;
                if written > MAX_AUDIO_BYTES {
                    return Err(Error::Download(format!(
                        "transferencia excede o limite de {MAX_AUDIO_BYTES} bytes"
                    )));
                }
                writer.write_all(&chunk).await?;
                if let Some(limiter) = &self.limiter {
                    limiter.consume(chunk.len()).await;
                }
                progress.update(written, expected.map(|total| written as f64 / total as f64));
            }
            Ok(())
        }
        .await;
        writer.flush().await?;
        let file = writer.into_inner();
        file.sync_all().await?;
        drop(file);
        if let Err(err) = streamed {
            part.keep = matches!(err, Error::Transfer(_)) && written > 0;
            return Err(err);
        }
        if let Some(total) = expected.filter(|total| *total != written) {
            return Err(Error::Integrity(format!(
                "download truncado ({written}/{total} bytes)"
            )));
        }
        Ok(())
    }

    async fn fetch_hls(
        &self,
        url: &str,
        writer: &mut BufWriter<File>,
        progress: &mut Progress<'_>,
    ) -> Result<()> {
        let http = self.client.http();
        let parts = segment_urls(http, url).await?;
        let total = parts.len();
        let mut segments = stream::iter(parts)
            .map(|part_url| async move {
                let part_url = &part_url;
                retry(
                    &SEGMENT_POLICY,
                    &self.cancel,
                    |_, _, _| {},
                    |_| async move {
                        let response =
                            ensure_success(http.get(part_url, &[]).await?, "segmento HLS")?;
                        read_limited(response, MAX_AUDIO_BYTES, "segmento HLS").await
                    },
                )
                .await
            })
            .buffered(HLS_SEGMENT_CONCURRENCY);

        let (mut written, mut done): (u64, usize) = (0, 0);
        while let Some(bytes) = segments.next().await {
            let bytes = bytes?;
            written += bytes.len() as u64;
            if written > MAX_AUDIO_BYTES {
                return Err(Error::Download(format!(
                    "transferencia excede o limite de {MAX_AUDIO_BYTES} bytes"
                )));
            }
            writer.write_all(&bytes).await?;
            if let Some(limiter) = &self.limiter {
                limiter.consume(bytes.len()).await;
            }
            done += 1;
            progress.update(written, Some(done as f64 / total as f64));
        }
        Ok(())
    }

    async fn fetch_artwork(&self, url: Option<&str>) -> Option<Vec<u8>> {
        let url = url.filter(|u| self.client.http().is_trusted(u))?;
        let response = self
            .client
            .http()
            .get(url, &[])
            .await
            .ok()
            .filter(|r| r.status().is_success())?;
        let is_image = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("image/"));
        if !is_image {
            return None;
        }
        read_limited(response, MAX_ARTWORK_BYTES, "capa")
            .await
            .inspect_err(|err| tracing::debug!(error = %err, "capa indisponivel"))
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn rate_limiter_paces_bytes() {
        let limiter = RateLimiter::new(80);
        let started = tokio::time::Instant::now();
        limiter.consume(10_000).await;
        limiter.consume(20_000).await;
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(1_900), "{elapsed:?}");
        assert!(elapsed <= Duration::from_millis(2_100), "{elapsed:?}");
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::features::download::models::{DownloadOptions, DownloadRequest};
    use crate::features::download::planning::{build_job, plan_download};
    use crate::shared::soundcloud::models::Resource;
    use crate::shared::soundcloud::transcoding::Container;

    #[tokio::test]
    #[ignore = "depende da rede e do SoundCloud"]
    async fn live_hls_aac_download() {
        let client = Arc::new(SoundCloudClient::new(None).expect("cliente"));
        let Resource::Track(mut track) = client
            .resolve("https://soundcloud.com/forss/flickermood")
            .await
            .expect("resolve")
        else {
            panic!("esperava faixa");
        };
        if let Some(media) = track.media.as_mut() {
            media.transcodings.retain(|t| {
                t.format.protocol == "hls" && t.format.mime_type.starts_with("audio/mp4")
            });
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let mut request = DownloadRequest::new("x");
        request.output_dir = dir.path().to_path_buf();
        let reporter = Reporter::silent("live");
        let plans = plan_download(Resource::Track(track.clone()), &request, &reporter)
            .await
            .expect("plano");
        let job = build_job(&plans[0], &track, &DownloadOptions::default()).expect("job");
        assert_eq!(job.format.container, Container::Mp4);
        let fetcher = AudioFetcher::new(
            client,
            reporter,
            CancellationToken::new(),
            Arc::new(Library::disabled()),
            None,
        );
        let done = fetcher.download(&job, None).await.expect("download");
        assert!(done.size_bytes > 1_000_000, "{done:?}");
    }
}
