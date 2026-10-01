use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use futures::{StreamExt as _, stream};
use tokio::sync::Semaphore;
use tokio::task::{Id, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::events::{Event, Reporter};
use crate::features::download::audio::{AudioFetcher, RateLimiter};
use crate::features::download::library::{ArchiveEntry, Library};
use crate::features::download::models::{
    CompletedDownload, DownloadJob, DownloadOptions, DownloadReport, DownloadRequest, DownloadStats,
};
use crate::features::download::planning::{
    DownloadPlan, NOT_RETURNED_REASON, build_job, claim_unique_name, plan_download,
    playlist_file_name,
};
use crate::shared::config::{REMOVED_FOLDER, TRACK_BATCH_CONCURRENCY, TRACK_BATCH_SIZE};
use crate::shared::error::{Error, Result};
use crate::shared::filesystem::{atomic_write, ensure_within};
use crate::shared::http::count_transport_retries;
use crate::shared::soundcloud::client::SoundCloudClient;
use crate::shared::soundcloud::models::Track;
use crate::shared::soundcloud::transcoding::{Quality, REASON_FILTERED};

const UNKNOWN_DURATION_S: u64 = 240;
const SPACE_MARGIN: f64 = 1.1;

async fn or_cancel<T>(
    cancel: &CancellationToken,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(Error::Cancelled),
        result = future => result,
    }
}

struct TaskInfo {
    plan: usize,
    track_id: i64,
    title: String,
    artist: String,
    duration_ms: Option<i64>,
}

struct Scheduler<'a> {
    plans: &'a [DownloadPlan],
    options: &'a DownloadOptions,
    reporter: &'a Reporter,
    cancel: &'a CancellationToken,
    fetcher: Arc<AudioFetcher>,
    permits: Arc<Semaphore>,
    transport_retries: Arc<AtomicU32>,
    claimed: HashMap<usize, HashMap<String, i64>>,
    tasks: JoinSet<Result<CompletedDownload>>,
    task_info: HashMap<Id, TaskInfo>,
    scheduled: usize,
    unavailable: BTreeMap<i64, String>,
    failed: BTreeMap<i64, String>,
}

struct Drained {
    completed: Vec<CompletedDownload>,
    unavailable: BTreeMap<i64, String>,
    failed: BTreeMap<i64, String>,
    scheduled: usize,
}

impl Scheduler<'_> {
    fn schedule(&mut self, plan_index: usize, track: &Track) {
        let plan = &self.plans[plan_index];
        if !self.options.accepts_duration(track.duration) {
            self.unavailable(track.id, REASON_FILTERED.to_owned());
            return;
        }
        let mut job = match build_job(plan, track, self.options) {
            Ok(job) => job,
            Err(err) => {
                self.unavailable(track.id, err.to_string());
                return;
            }
        };
        self.claim_unique_name(plan_index, &mut job);
        let archived = plan.archive.get(track.id);
        let fetcher = Arc::clone(&self.fetcher);
        let permits = Arc::clone(&self.permits);
        let cancel = self.cancel.clone();
        let info = TaskInfo {
            plan: plan_index,
            track_id: track.id,
            title: job.title.clone(),
            artist: job.artist.clone(),
            duration_ms: job.duration_ms,
        };
        let counter = Arc::clone(&self.transport_retries);
        let handle = self
            .tasks
            .spawn(count_transport_retries(counter, async move {
                or_cancel(&cancel, async {
                    let _permit = permits.acquire().await.map_err(|_| Error::Cancelled)?;
                    fetcher.download(&job, archived).await
                })
                .await
            }));
        self.task_info.insert(handle.id(), info);
        self.scheduled += 1;
    }

    fn claim_unique_name(&mut self, plan_index: usize, job: &mut DownloadJob) {
        let plan = &self.plans[plan_index];
        let claimed = self.claimed.entry(plan_index).or_insert_with(|| {
            plan.archive
                .entries()
                .into_iter()
                .map(|(id, entry)| (entry.file.to_lowercase(), id))
                .collect()
        });
        claim_unique_name(claimed, job);
    }

    fn unavailable(&mut self, track_id: i64, reason: String) {
        self.reporter.emit(&Event::TrackUnavailable {
            track_id,
            reason: reason.clone(),
        });
        self.unavailable.insert(track_id, reason);
    }

    fn fail(&mut self, track_id: i64, reason: String) {
        self.reporter.emit(&Event::TrackFailed {
            track_id,
            reason: reason.clone(),
        });
        self.failed.insert(track_id, reason);
    }

    async fn hydrate_stubs(&mut self, client: &SoundCloudClient) {
        let stubs: Vec<(usize, Vec<i64>)> = self
            .plans
            .iter()
            .enumerate()
            .map(|(i, plan)| (i, plan.stub_ids()))
            .collect();
        let batches: Vec<_> = stubs
            .iter()
            .flat_map(|(plan, ids)| {
                ids.chunks(TRACK_BATCH_SIZE)
                    .map(move |chunk| (*plan, chunk))
            })
            .map(|(plan, chunk)| async move { (plan, chunk, client.fetch_batch(chunk).await) })
            .collect();
        let mut batches = stream::iter(batches).buffer_unordered(TRACK_BATCH_CONCURRENCY);
        loop {
            let next = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return,
                next = batches.next() => next,
            };
            let Some((plan, requested, result)) = next else {
                return;
            };
            match result {
                Ok(tracks) => {
                    let mut found: HashMap<i64, Track> =
                        tracks.into_iter().map(|t| (t.id, t)).collect();
                    for track_id in requested {
                        match found.remove(track_id) {
                            Some(track) if !track.is_stub() => self.schedule(plan, &track),
                            _ => self.unavailable(*track_id, NOT_RETURNED_REASON.to_owned()),
                        }
                    }
                }
                Err(Error::Cancelled) => return,
                Err(err) => {
                    let reason = format!("falha ao consultar lote: {err}");
                    for track_id in requested {
                        self.fail(*track_id, reason.clone());
                    }
                }
            }
        }
    }

    async fn drain(mut self) -> Drained {
        let mut completed = Vec::with_capacity(self.scheduled);
        while let Some(joined) = self.tasks.join_next_with_id().await {
            let (id, result) = match joined {
                Ok(outcome) => outcome,
                Err(err) => {
                    tracing::error!(error = %err, "tarefa de download terminou de forma inesperada");
                    let reason = "erro inesperado; detalhes no registro".to_owned();
                    (err.id(), Err(Error::Download(reason)))
                }
            };
            let Some(info) = self.task_info.remove(&id) else {
                continue;
            };
            match result {
                Ok(done) => {
                    let file = Path::new(&done.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned());
                    if let Some(file) = file {
                        self.plans[info.plan].archive.record(
                            info.track_id,
                            ArchiveEntry {
                                file,
                                title: info.title,
                                artist: info.artist,
                                duration_ms: info.duration_ms,
                            },
                        );
                    }
                    completed.push(done);
                }
                Err(Error::Cancelled) => {}
                Err(err) => self.fail(info.track_id, err.to_string()),
            }
        }
        Drained {
            completed,
            unavailable: self.unavailable,
            failed: self.failed,
            scheduled: self.scheduled,
        }
    }
}

pub async fn run_download(
    client: Arc<SoundCloudClient>,
    request: DownloadRequest,
    cancel: CancellationToken,
    reporter: Reporter,
) -> DownloadReport {
    let transport_retries = Arc::new(AtomicU32::new(0));
    let run = execute(
        client,
        request,
        cancel,
        reporter,
        Arc::clone(&transport_retries),
    );
    count_transport_retries(transport_retries, run).await
}

async fn execute(
    client: Arc<SoundCloudClient>,
    request: DownloadRequest,
    cancel: CancellationToken,
    reporter: Reporter,
    transport_retries: Arc<AtomicU32>,
) -> DownloadReport {
    let started = Instant::now();
    let mut report = DownloadReport {
        run_id: reporter.run_id().to_owned(),
        ..DownloadReport::default()
    };

    let planned = async {
        request.validate()?;
        let resource = or_cancel(&cancel, client.resolve(&request.url)).await?;
        let plans = plan_download(resource, &request, &reporter).await?;
        check_disk_space(&plans, &request.output_dir, request.options.quality)?;
        Ok(plans)
    };
    let plans = match planned.await {
        Ok(plans) => plans,
        Err(Error::Cancelled) => {
            report.cancelled = true;
            return finish(report, started, &reporter);
        }
        Err(err) => {
            reporter.emit(&Event::ResolveFailed {
                url: request.url.clone(),
                reason: err.to_string(),
            });
            report.fatal_error = Some(err.to_string());
            report.fatal_error_code = Some(err.code());
            return finish(report, started, &reporter);
        }
    };
    report.target_dir = Some(match plans.as_slice() {
        [single] => single.target_dir.display().to_string(),
        _ => request.output_dir.display().to_string(),
    });
    let moved = if request.syncs_removals() {
        move_removed(&plans, &reporter).await
    } else {
        0
    };

    let library = Arc::new(if request.options.use_library {
        Library::load(request.library_path.clone())
    } else {
        Library::disabled()
    });
    let limiter = request
        .options
        .max_kbps
        .map(|kbps| Arc::new(RateLimiter::new(kbps)));
    let mut scheduler = Scheduler {
        plans: &plans,
        options: &request.options,
        reporter: &reporter,
        cancel: &cancel,
        fetcher: Arc::new(AudioFetcher::new(
            Arc::clone(&client),
            reporter.clone(),
            cancel.clone(),
            Arc::clone(&library),
            limiter,
        )),
        permits: Arc::new(Semaphore::new(request.workers)),
        transport_retries: Arc::clone(&transport_retries),
        claimed: HashMap::new(),
        tasks: JoinSet::new(),
        task_info: HashMap::new(),
        scheduled: 0,
        unavailable: BTreeMap::new(),
        failed: BTreeMap::new(),
    };
    for (index, plan) in plans.iter().enumerate() {
        for track in plan.selected.iter().filter(|t| !t.is_stub()) {
            scheduler.schedule(index, track);
        }
    }
    scheduler.hydrate_stubs(&client).await;
    let drained = scheduler.drain().await;

    persist(&plans, &library, &request.options, &reporter);
    let selected = plans.iter().map(|plan| plan.selected.len()).sum();
    report.stats = DownloadStats::new(
        selected,
        drained.scheduled,
        &drained.completed,
        started.elapsed().as_secs_f64(),
    );
    report.stats.moved_removed = moved;
    report.stats.transport_retries = transport_retries.load(Ordering::Relaxed);
    report.completed = drained.completed;
    report.unavailable = drained.unavailable;
    report.failed = drained.failed;
    report.cancelled = cancel.is_cancelled();
    finish(report, started, &reporter)
}

fn estimate_bytes(plans: &[DownloadPlan], quality: Quality) -> u64 {
    let kbps: u64 = match quality {
        Quality::Compatible => 128,
        Quality::Best => 160,
    };
    let seconds: u64 = plans
        .iter()
        .flat_map(|plan| plan.selected.iter().map(move |track| (plan, track)))
        .filter(|(plan, track)| {
            plan.archive
                .get(track.id)
                .is_none_or(|entry| !plan.target_dir.join(entry.file).is_file())
        })
        .map(|(_, track)| {
            track
                .duration
                .and_then(|ms| u64::try_from(ms / 1000).ok())
                .unwrap_or(UNKNOWN_DURATION_S)
        })
        .sum();
    (seconds as f64 * kbps as f64 * 1000.0 / 8.0 * SPACE_MARGIN) as u64
}

fn ensure_space(needed: u64, available: u64) -> Result<()> {
    const MB: u64 = 1024 * 1024;
    if available >= needed {
        return Ok(());
    }
    Err(Error::InsufficientSpace {
        needed_mb: needed.div_ceil(MB),
        available_mb: available / MB,
    })
}

fn check_disk_space(plans: &[DownloadPlan], output_dir: &Path, quality: Quality) -> Result<()> {
    let needed = estimate_bytes(plans, quality);
    match fs4::available_space(output_dir) {
        Ok(available) => ensure_space(needed, available),
        Err(err) => {
            tracing::warn!(error = %err, "nao foi possivel medir o espaco livre; seguindo sem checar");
            Ok(())
        }
    }
}

async fn move_removed(plans: &[DownloadPlan], reporter: &Reporter) -> usize {
    let mut moved = 0;
    for plan in plans.iter().filter(|plan| !plan.single) {
        let current: HashSet<i64> = plan.order.iter().copied().collect();
        for (track_id, entry) in plan.archive.entries() {
            if current.contains(&track_id) {
                continue;
            }
            let source = plan.target_dir.join(&entry.file);
            if source.is_file() {
                let removed_dir = plan.target_dir.join(REMOVED_FOLDER);
                let mut target = removed_dir.join(&entry.file);
                if target.exists() {
                    target = removed_dir.join(format!("{track_id} - {}", entry.file));
                }
                let result = async {
                    tokio::fs::create_dir_all(&removed_dir).await?;
                    tokio::fs::rename(&source, &target).await
                }
                .await;
                if let Err(err) = result {
                    tracing::warn!(track_id, error = %err, "nao foi possivel mover faixa removida da playlist");
                    continue;
                }
                reporter.emit(&Event::TrackMoved {
                    track_id,
                    file: entry.file.clone(),
                });
                moved += 1;
            }
            plan.archive.remove(track_id);
        }
    }
    moved
}

fn persist(
    plans: &[DownloadPlan],
    library: &Library,
    options: &DownloadOptions,
    reporter: &Reporter,
) {
    for plan in plans {
        if let Err(err) = plan.archive.save() {
            tracing::warn!(dir = %plan.target_dir.display(), error = %err, "historico da pasta nao gravado");
        }
        if options.write_playlist_file && !plan.single {
            match write_playlist_file(plan) {
                Ok(Some((file, tracks))) => {
                    reporter.emit(&Event::PlaylistFileWritten { file, tracks });
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(dir = %plan.target_dir.display(), error = %err, "m3u8 nao gravado");
                }
            }
        }
    }
    if let Err(err) = library.save() {
        tracing::warn!(error = %err, "biblioteca local nao gravada");
    }
}

fn write_playlist_file(plan: &DownloadPlan) -> Result<Option<(String, usize)>> {
    let mut lines = vec!["#EXTM3U".to_owned(), format!("#PLAYLIST:{}", plan.album)];
    let mut tracks = 0;
    for track_id in &plan.order {
        let Some(entry) = plan.archive.get(*track_id) else {
            continue;
        };
        if !plan.target_dir.join(&entry.file).is_file() {
            continue;
        }
        let seconds = entry.duration_ms.map_or(-1, |ms| ms / 1000);
        lines.push(format!(
            "#EXTINF:{seconds},{} - {}",
            entry.artist, entry.title
        ));
        lines.push(entry.file.clone());
        tracks += 1;
    }
    if tracks == 0 {
        return Ok(None);
    }
    let file = playlist_file_name(&plan.album);
    let path = ensure_within(&plan.target_dir, &plan.target_dir.join(&file))?;
    atomic_write(&path, format!("{}\n", lines.join("\n")).as_bytes())?;
    Ok(Some((file, tracks)))
}

fn finish(mut report: DownloadReport, started: Instant, reporter: &Reporter) -> DownloadReport {
    report.elapsed_seconds = (started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
    if report.cancelled {
        reporter.emit(&Event::Cancelled);
    } else {
        reporter.emit(&Event::Finished {
            downloaded: report.downloaded().count(),
            reused: report.reused_count(),
            failed: report.failed.len(),
            unavailable: report.unavailable.len(),
            bytes: report.transferred_bytes(),
            elapsed_s: report.elapsed_seconds,
        });
    }
    report
}

#[cfg(test)]
mod tests {

    use serde_json::json;
    use wiremock::matchers::{path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::shared::config::ARCHIVE_FILE;
    use crate::test_support::{client, collecting_reporter, mount_media, track};

    async fn server_with(tracks: impl FnOnce(&str) -> serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        let tracks = tracks(&server.uri());
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "playlist", "title": "Argonautas", "user": {"username": "Perseus"}, "tracks": tracks
            })))
            .mount(&server)
            .await;
        mount_media(&server).await;
        server
    }

    fn request_in(dir: &tempfile::TempDir) -> DownloadRequest {
        let mut request = DownloadRequest::new("https://soundcloud.com/perseus/sets/argonautas");
        request.output_dir = dir.path().join("musica");
        request.library_path = Some(dir.path().join("library.json"));
        std::fs::create_dir_all(&request.output_dir).expect("pasta");
        request
    }

    #[tokio::test]
    async fn downloads_progressive_and_hls_then_reuses_on_resync() {
        let server = server_with(|base| {
            json!([
                track(1, "Primeira", "progressive", base),
                track(2, "Segunda", "hls", base),
                {"id": 3, "title": "Protegida", "media": {"transcodings": [{"url": format!("{base}/x"),
                    "format": {"protocol": "ctr-encrypted-hls", "mime_type": "audio/mp4"}}]}}
            ])
        })
        .await;
        let dir = tempfile::tempdir().expect("tempdir");
        let (reporter, events) = collecting_reporter();

        let report = run_download(
            client(&server),
            request_in(&dir),
            CancellationToken::new(),
            reporter,
        )
        .await;
        assert!(report.fatal_error.is_none(), "{:?}", report.fatal_error);
        assert_eq!(report.downloaded().count(), 2, "{:?}", report.failed);
        assert!(report.unavailable[&3].contains("DRM"));
        assert_eq!((report.stats.selected, report.stats.scheduled), (3, 2));
        assert!(report.stats.bytes_per_second > 0.0);
        let folder = dir.path().join("musica").join("Perseus - Argonautas");
        assert!(folder.join("01. Perseus - Primeira.mp3").is_file());
        assert!(folder.join("02. Perseus - Segunda.mp3").is_file());
        assert!(folder.join(ARCHIVE_FILE).is_file());
        let m3u = std::fs::read_to_string(folder.join("Argonautas.m3u8")).expect("m3u8");
        assert_eq!(
            m3u,
            "#EXTM3U\n#PLAYLIST:Argonautas\n#EXTINF:180,Perseus - Primeira\n01. Perseus - Primeira.mp3\n\
             #EXTINF:180,Perseus - Segunda\n02. Perseus - Segunda.mp3\n"
        );
        assert!(std::fs::read_dir(&folder).expect("dir").all(|e| {
            !e.expect("entrada")
                .file_name()
                .to_string_lossy()
                .ends_with(".part")
        }));
        assert!(
            events
                .lock()
                .expect("lock")
                .iter()
                .any(|e| matches!(e, Event::TrackProgress { .. }))
        );

        let second = run_download(
            client(&server),
            request_in(&dir),
            CancellationToken::new(),
            Reporter::silent("t2"),
        )
        .await;
        assert_eq!(second.reused_count(), 2);
        assert_eq!(second.transferred_bytes(), 0);
    }

    #[tokio::test]
    async fn library_copies_tracks_already_downloaded_elsewhere_without_network() {
        let server = server_with(|base| json!([track(1, "Primeira", "progressive", base)])).await;
        let dir = tempfile::tempdir().expect("tempdir");
        run_download(
            client(&server),
            request_in(&dir),
            CancellationToken::new(),
            Reporter::silent("a"),
        )
        .await;

        let other = MockServer::start().await;
        let base = other.uri();
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "playlist", "title": "Outra", "user": {"username": "Curadora"},
                "tracks": [track(1, "Primeira", "progressive", &base)]
            })))
            .mount(&other)
            .await;
        Mock::given(path("/progressive"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&other)
            .await;
        let (reporter, events) = collecting_reporter();
        let report = run_download(
            client(&other),
            request_in(&dir),
            CancellationToken::new(),
            reporter,
        )
        .await;

        assert_eq!(
            (report.reused_count(), report.stats.library_hits),
            (1, 1),
            "{:?}",
            report.failed
        );
        assert!(
            dir.path()
                .join("musica")
                .join("Curadora - Outra")
                .join("01. Perseus - Primeira.mp3")
                .is_file()
        );
        assert!(
            events
                .lock()
                .expect("lock")
                .iter()
                .any(|e| matches!(e, Event::TrackCopied { .. }))
        );
    }

    #[tokio::test]
    async fn sync_moves_tracks_that_left_the_playlist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = server_with(|base| {
            json!([
                track(1, "Fica", "progressive", base),
                track(2, "Sai", "progressive", base)
            ])
        })
        .await;
        run_download(
            client(&first),
            request_in(&dir),
            CancellationToken::new(),
            Reporter::silent("a"),
        )
        .await;

        let second = server_with(|base| json!([track(1, "Fica", "progressive", base)])).await;
        let mut request = request_in(&dir);
        request.options.sync_removed = true;
        let (reporter, events) = collecting_reporter();
        let report =
            run_download(client(&second), request, CancellationToken::new(), reporter).await;

        let folder = dir.path().join("musica").join("Perseus - Argonautas");
        assert_eq!(report.stats.moved_removed, 1);
        assert!(folder.join("01. Perseus - Fica.mp3").is_file());
        assert!(!folder.join("02. Perseus - Sai.mp3").exists());
        assert!(
            folder
                .join(REMOVED_FOLDER)
                .join("02. Perseus - Sai.mp3")
                .is_file(),
            "movida, nunca apagada"
        );
        assert!(
            events
                .lock()
                .expect("lock")
                .iter()
                .any(|e| matches!(e, Event::TrackMoved { track_id: 2, .. }))
        );
        let m3u = std::fs::read_to_string(folder.join("Argonautas.m3u8")).expect("m3u8");
        assert!(!m3u.contains("Sai"), "{m3u}");
    }

    #[tokio::test]
    async fn duration_filter_and_template_are_applied() {
        let server = server_with(|base| {
            let mut long = track(2, "Mix de 2 horas", "progressive", base);
            long["duration"] = json!(7_200_000);
            json!([track(1, "Curta", "progressive", base), long])
        })
        .await;
        let dir = tempfile::tempdir().expect("tempdir");
        let mut request = request_in(&dir);
        request.options.max_duration_s = Some(600);
        request.options.name_template = Some("{title} [{id}]".into());
        let report = run_download(
            client(&server),
            request,
            CancellationToken::new(),
            Reporter::silent("t"),
        )
        .await;

        assert_eq!(report.unavailable[&2], REASON_FILTERED);
        assert!(
            dir.path()
                .join("musica")
                .join("Perseus - Argonautas")
                .join("Curta [1].mp3")
                .is_file()
        );
    }

    #[tokio::test]
    async fn collections_download_each_playlist_into_its_folder() {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(path("/resolve"))
            .and(query_param("url", "https://soundcloud.com/band"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"kind": "user", "id": 9, "username": "Band"})),
            )
            .mount(&server)
            .await;
        Mock::given(path("/users/9/albums"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"collection": [
                {"kind": "playlist", "title": "Um", "user": {"username": "Band"}, "tracks": [track(1, "A", "progressive", &base)]},
                {"kind": "playlist", "title": "Dois", "user": {"username": "Band"}, "tracks": [track(2, "B", "progressive", &base)]}
            ], "next_href": null})))
            .mount(&server)
            .await;
        mount_media(&server).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let mut request = request_in(&dir);
        request.url = "https://soundcloud.com/band/albums".into();
        let report = run_download(
            client(&server),
            request,
            CancellationToken::new(),
            Reporter::silent("t"),
        )
        .await;

        assert_eq!(
            report.downloaded().count(),
            2,
            "{:?} {:?}",
            report.failed,
            report.fatal_error
        );
        let root = dir.path().join("musica");
        assert!(root.join("Band - Um").join("01. Perseus - A.mp3").is_file());
        assert!(
            root.join("Band - Dois")
                .join("01. Perseus - B.mp3")
                .is_file()
        );
        assert_eq!(
            report.target_dir.as_deref(),
            Some(root.display().to_string().as_str())
        );
    }

    #[tokio::test]
    async fn failed_stub_batch_only_affects_its_own_tracks() {
        let server = server_with(|base| {
            let mut tracks = vec![track(1, "Completa", "progressive", base)];
            tracks.extend((100..160).map(|id| json!({"id": id})));
            json!(tracks)
        })
        .await;
        let first_batch = (100..150)
            .map(|id: i64| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let second_batch = (150..160)
            .map(|id: i64| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        Mock::given(path("/tracks"))
            .and(query_param("ids", first_batch.as_str()))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;
        let uri = server.uri();
        Mock::given(path("/tracks"))
            .and(query_param("ids", second_batch.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                track(150, "Hidratada A", "progressive", &uri),
                track(151, "Hidratada B", "progressive", &uri)
            ])))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let report = run_download(
            client(&server),
            request_in(&dir),
            CancellationToken::new(),
            Reporter::silent("t"),
        )
        .await;

        assert_eq!(report.downloaded().count(), 3, "{:?}", report.failed);
        assert_eq!(report.failed.len(), 50);
        assert!(report.failed[&100].contains("falha ao consultar lote"));
        assert_eq!(report.unavailable.len(), 8);
        assert_eq!(report.unavailable[&159], NOT_RETURNED_REASON);
        assert_eq!(report.stats.selected, 61);
        assert!(!report.ok());
    }

    #[tokio::test]
    async fn reports_resolve_failures_as_fatal() {
        let server = MockServer::start().await;
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tempdir");
        let report = run_download(
            client(&server),
            request_in(&dir),
            CancellationToken::new(),
            Reporter::silent("t"),
        )
        .await;
        assert!(
            report
                .fatal_error
                .expect("fatal")
                .contains("nao encontrado")
        );
        assert_eq!(report.fatal_error_code, Some("not_found"));
        assert!(!report.cancelled);
    }

    #[tokio::test]
    async fn cancelled_before_start_reports_cancellation() {
        let server = MockServer::start().await;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let report = run_download(
            client(&server),
            DownloadRequest::new("https://soundcloud.com/a/b"),
            cancel,
            Reporter::silent("t"),
        )
        .await;
        assert!(report.cancelled);
        assert!(!report.ok());
    }

    #[test]
    fn disk_space_check_fails_early_with_sizes() {
        assert!(ensure_space(100, 200).is_ok());
        let err = ensure_space(3 * 1024 * 1024 * 1024, 1024 * 1024 * 1024).expect_err("sem espaco");
        assert!(
            matches!(
                err,
                Error::InsufficientSpace {
                    needed_mb: 3072,
                    available_mb: 1024
                }
            ),
            "{err:?}"
        );
        assert_eq!(err.code(), "insufficient_space");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[ignore = "teste de carga; lento em debug"]
    async fn load_500_tracks_with_transient_failures_then_idempotent_resync() {
        const TRACKS: i64 = 500;
        let server = MockServer::start().await;
        Mock::given(path("/audio.mp3"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(40)
            .mount(&server)
            .await;
        let base = server.uri();
        let tracks: Vec<_> = (1..=TRACKS)
            .map(|id| {
                let protocol = if id % 5 == 0 { "hls" } else { "progressive" };
                track(id, &format!("Faixa {id}"), protocol, &base)
            })
            .collect();
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "playlist", "title": "Carga", "user": {"username": "Perseus"}, "tracks": tracks
            })))
            .mount(&server)
            .await;
        mount_media(&server).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let mut request = request_in(&dir);
        request.workers = 16;

        let started = Instant::now();
        let first = run_download(
            client(&server),
            request.clone(),
            CancellationToken::new(),
            Reporter::silent("load"),
        )
        .await;
        let elapsed = started.elapsed();
        assert!(first.fatal_error.is_none(), "{:?}", first.fatal_error);
        assert!(first.failed.is_empty(), "{:?}", first.failed);
        assert_eq!(first.downloaded().count(), 500);
        let audio_requests = server
            .received_requests()
            .await
            .expect("gravacao de requisicoes")
            .iter()
            .filter(|r| r.url.path() == "/audio.mp3")
            .count();
        assert_eq!(
            audio_requests,
            400 + 40,
            "400 faixas progressive + 40 respostas 503"
        );
        assert_eq!(
            first.stats.transport_retries, 40,
            "cada 503 injetado e um retry de transporte"
        );
        let folder = dir.path().join("musica").join("Perseus - Carga");
        let files = std::fs::read_dir(&folder).expect("dir").count();
        assert_eq!(files, 500 + 2, "500 faixas + archive + m3u8");
        assert!(
            elapsed < std::time::Duration::from_secs(120),
            "500 faixas em {elapsed:?}"
        );
        eprintln!(
            "carga: 500 faixas em {elapsed:?}, {:.1} MB/s, {} retries de transporte",
            first.stats.bytes_per_second / 1_000_000.0,
            first.stats.transport_retries
        );

        let again = run_download(
            client(&server),
            request,
            CancellationToken::new(),
            Reporter::silent("load"),
        )
        .await;
        assert_eq!(again.reused_count(), 500);
        assert_eq!(again.downloaded().count(), 0);
    }
}
