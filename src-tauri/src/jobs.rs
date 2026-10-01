use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use perseus_core::events::{Event, Level, new_run_id};
use perseus_core::shared::config::default_output_dir;
use perseus_core::shared::soundcloud::transcoding::reason_code;
use perseus_core::shared::soundcloud::urls::playlist_context;
use perseus_core::{
    DownloadReport, DownloadRequest, PlaylistWatcher, Reporter, SoundCloudClient, WatchConfig,
    run_download,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::dto::{
    CommandError, CommandErrorKind, JobCounts, JobEvent, JobIn, JobOut, JobState, Mode,
};

pub const EVENT_CHANNEL: &str = "perseus://job";

pub type EventEmitter = Arc<dyn Fn(&JobEvent) + Send + Sync>;
const MAX_ACTIVE_JOBS: usize = 3;
const MAX_RETAINED_JOBS: usize = 50;
const MAX_EVENTS_PER_JOB: usize = 5000;

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct JobInner {
    state: JobState,
    title: Option<String>,
    target_dir: Option<String>,
    finished_at: Option<f64>,
    error: Option<String>,
    error_code: Option<&'static str>,
    report: Option<Value>,
    counts: JobCounts,
    seq: u64,
    events: VecDeque<JobEvent>,
}

pub struct Job {
    pub id: String,
    spec: JobIn,
    created_at: f64,
    cancel: CancellationToken,
    emit: EventEmitter,
    inner: Mutex<JobInner>,
}

impl Job {
    fn new(spec: JobIn, emit: EventEmitter) -> Self {
        Self {
            id: new_run_id(),
            spec,
            created_at: now(),
            cancel: CancellationToken::new(),
            emit,
            inner: Mutex::new(JobInner {
                state: JobState::Running,
                title: None,
                target_dir: None,
                finished_at: None,
                error: None,
                error_code: None,
                report: None,
                counts: JobCounts::default(),
                seq: 0,
                events: VecDeque::new(),
            }),
        }
    }

    pub fn active(&self) -> bool {
        lock(&self.inner).state == JobState::Running
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn target_dir(&self) -> Option<PathBuf> {
        lock(&self.inner).target_dir.as_ref().map(PathBuf::from)
    }

    pub fn snapshot(&self) -> JobOut {
        Self::snapshot_of(&self.id, &self.spec, self.created_at, &lock(&self.inner))
    }

    fn snapshot_of(id: &str, spec: &JobIn, created_at: f64, inner: &JobInner) -> JobOut {
        JobOut {
            id: id.to_owned(),
            url: spec.url.clone(),
            mode: spec.mode,
            state: inner.state,
            title: inner.title.clone(),
            target_dir: inner.target_dir.clone(),
            created_at,
            finished_at: inner.finished_at,
            error: inner.error.clone(),
            error_code: inner.error_code,
            counts: inner.counts.clone(),
            report: inner.report.clone(),
        }
    }

    pub fn events_after(&self, after: u64) -> Vec<JobEvent> {
        lock(&self.inner)
            .events
            .iter()
            .filter(|event| event.id > after)
            .cloned()
            .collect()
    }

    fn publish(&self, update: impl FnOnce(&mut JobInner) -> Vec<(&'static str, Value)>) {
        let emitted: Vec<JobEvent> = {
            let mut inner = lock(&self.inner);
            let items = update(&mut inner);
            items
                .into_iter()
                .map(|(event, data)| {
                    inner.seq += 1;
                    let item = JobEvent {
                        job_id: self.id.clone(),
                        id: inner.seq,
                        event,
                        data,
                    };
                    if inner.events.len() == MAX_EVENTS_PER_JOB {
                        inner.events.pop_front();
                    }
                    inner.events.push_back(item.clone());
                    item
                })
                .collect()
        };
        for item in &emitted {
            (self.emit)(item);
        }
    }

    fn state_event(&self, inner: &JobInner) -> (&'static str, Value) {
        let snapshot = Self::snapshot_of(&self.id, &self.spec, self.created_at, inner);
        (
            "state",
            serde_json::to_value(snapshot).unwrap_or(Value::Null),
        )
    }

    fn log(level: Level, message: &str, detail: &Value) -> (&'static str, Value) {
        (
            "log",
            json!({ "ts": now(), "level": level, "message": message, "detail": detail }),
        )
    }

    fn on_event(&self, event: &Event) {
        self.publish(|inner| {
            let mut out = Vec::new();
            if !matches!(event, Event::TrackProgress { .. }) {
                out.push(Self::log(event.level(), &event.message(), &event_detail(event)));
            }
            match event {
                Event::Planned { album, selected, target_dir, .. } => {
                    inner.title = Some(album.clone());
                    inner.target_dir = Some(target_dir.clone());
                    inner.counts.selected = Some(*selected);
                    out.push(self.state_event(inner));
                }
                Event::TrackProgress { track_id, bytes, fraction } => out.push((
                    "track",
                    json!({ "track_id": track_id, "status": "downloading", "reason": null, "reason_code": null, "bytes": bytes, "progress": fraction }),
                )),
                Event::TrackDownloaded { track_id, bytes, .. } => {
                    inner.counts.downloaded += 1;
                    out.push(Self::track(*track_id, "done", None, Some(*bytes)));
                    out.push(self.state_event(inner));
                }
                Event::TrackCopied { track_id, .. } => {
                    inner.counts.reused += 1;
                    out.push(Self::track(*track_id, "copied", None, None));
                    out.push(self.state_event(inner));
                }
                Event::TrackReused { track_id, .. } => {
                    inner.counts.reused += 1;
                    out.push(Self::track(*track_id, "reused", None, None));
                    out.push(self.state_event(inner));
                }
                Event::TrackRetry { track_id, reason, .. } => out.push(Self::track(*track_id, "retrying", Some(reason), None)),
                Event::TrackUnavailable { track_id, reason } => {
                    inner.counts.unavailable += 1;
                    out.push(Self::track(*track_id, "unavailable", Some(reason), None));
                    out.push(self.state_event(inner));
                }
                Event::TrackFailed { track_id, reason } => {
                    inner.counts.failed += 1;
                    out.push(Self::track(*track_id, "failed", Some(reason), None));
                    out.push(self.state_event(inner));
                }
                Event::WatchStarted { .. }
                | Event::WatchNewTracks { .. }
                | Event::WatchIdle { .. }
                | Event::WatchCheckFailed { .. }
                | Event::WatchSyncFailed { .. }
                | Event::WatchTrackAbandoned { .. }
                | Event::WatchStopped => out.push((
                    "watch",
                    json!({ "event": event.name(), "message": event.message(), "detail": event_detail(event) }),
                )),
                Event::ResolveFailed { .. }
                | Event::Finished { .. }
                | Event::Cancelled
                | Event::TrackMoved { .. }
                | Event::PlaylistFileWritten { .. } => {}
            }
            out
        });
    }

    fn track(
        track_id: i64,
        status: &str,
        reason: Option<&String>,
        bytes: Option<u64>,
    ) -> (&'static str, Value) {
        (
            "track",
            json!({
                "track_id": track_id,
                "status": status,
                "reason": reason,
                "reason_code": reason.and_then(|r| reason_code(r)),
                "bytes": bytes,
                "progress": null,
            }),
        )
    }

    fn finish(&self, state: JobState, report: Option<&DownloadReport>, error: Option<JobError>) {
        self.publish(|inner| {
            let mut out = Vec::new();
            if let Some(error) = &error {
                let detail =
                    json!({ "event": "job_failed", "code": error.code, "reason": error.message });
                out.push(Self::log(Level::Error, &error.message, &detail));
            }
            inner.state = state;
            inner.error_code = error.as_ref().map(|e| e.code);
            inner.error = error.map(|e| e.message);
            inner.finished_at = Some(now());
            if let Some(report) = report {
                inner.counts = JobCounts {
                    downloaded: report.downloaded().count(),
                    reused: report.reused_count(),
                    failed: report.failed.len(),
                    unavailable: report.unavailable.len(),
                    selected: inner.counts.selected,
                };
                inner.target_dir = report.target_dir.clone().or(inner.target_dir.take());
                inner.report = Some(report.to_json());
            }
            out.push(self.state_event(inner));
            out.push(("end", json!({})));
            out
        });
    }
}

struct JobError {
    code: &'static str,
    message: String,
}

impl From<&perseus_core::Error> for JobError {
    fn from(err: &perseus_core::Error) -> Self {
        Self {
            code: err.code(),
            message: err.to_string(),
        }
    }
}

fn event_detail(event: &Event) -> Value {
    let mut detail = serde_json::to_value(event).unwrap_or(Value::Null);
    let code = detail
        .get("reason")
        .and_then(Value::as_str)
        .and_then(reason_code);
    if let (Some(code), Some(fields)) = (code, detail.as_object_mut()) {
        fields.insert("reason_code".to_owned(), Value::from(code));
    }
    detail
}

pub struct JobManager {
    client: Arc<SoundCloudClient>,
    jobs: Mutex<HashMap<String, Arc<Job>>>,
}

impl JobManager {
    pub fn new(client: Arc<SoundCloudClient>) -> Self {
        Self {
            client,
            jobs: Mutex::new(HashMap::new()),
        }
    }

    pub fn client(&self) -> &Arc<SoundCloudClient> {
        &self.client
    }

    pub fn get(&self, id: &str) -> Option<Arc<Job>> {
        lock(&self.jobs).get(id).cloned()
    }

    pub fn list(&self) -> Vec<JobOut> {
        let mut jobs: Vec<JobOut> = lock(&self.jobs)
            .values()
            .map(|job| job.snapshot())
            .collect();
        jobs.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
        jobs
    }

    pub fn cancel_all(&self) {
        for job in lock(&self.jobs).values() {
            job.cancel();
        }
    }

    pub fn create(&self, spec: JobIn, emit: EventEmitter) -> Result<JobOut, CommandError> {
        let job = {
            let mut jobs = lock(&self.jobs);
            if jobs.values().filter(|job| job.active()).count() >= MAX_ACTIVE_JOBS {
                return Err(CommandError::of(
                    CommandErrorKind::TooManyJobs,
                    "too_many_jobs",
                    format!("Limite de {MAX_ACTIVE_JOBS} tarefas simultaneas atingido."),
                ));
            }
            let job = Arc::new(Job::new(spec, emit));
            jobs.insert(job.id.clone(), Arc::clone(&job));
            Self::evict_finished(&mut jobs);
            job
        };
        let snapshot = job.snapshot();
        let client = Arc::clone(&self.client);
        tauri::async_runtime::spawn(run(client, job));
        Ok(snapshot)
    }

    fn evict_finished(jobs: &mut HashMap<String, Arc<Job>>) {
        let mut finished: Vec<(f64, String)> = jobs
            .values()
            .filter(|job| !job.active())
            .map(|job| (job.created_at, job.id.clone()))
            .collect();
        finished.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, id) in finished.into_iter().skip(MAX_RETAINED_JOBS) {
            jobs.remove(&id);
        }
    }
}

async fn run(client: Arc<SoundCloudClient>, job: Arc<Job>) {
    let sink_job = Arc::clone(&job);
    let reporter = Reporter::new(
        job.id.clone(),
        Arc::new(move |event: &Event| sink_job.on_event(event)),
    );
    let spec = job.spec.clone();

    let result = async {
        let mut url = client.canonicalize(&spec.url).await?;
        if spec.mode != Mode::Track
            && let Some(context) = playlist_context(&url)
        {
            url = context;
        }
        let output_dir = spec
            .output_dir
            .clone()
            .map_or_else(default_output_dir, PathBuf::from);

        if spec.mode == Mode::Watch {
            let config = WatchConfig {
                url,
                output_dir,
                interval: spec.interval,
                workers: spec.workers,
                options: spec.options(),
                ..WatchConfig::new("")
            };
            PlaylistWatcher::new(config, Arc::clone(&client), reporter, job.cancel.clone())?
                .run()
                .await?;
            return Ok(None);
        }
        let request = DownloadRequest {
            url,
            output_dir,
            limit: spec.limit,
            only_track_ids: None,
            workers: spec.workers,
            options: spec.options(),
            ..DownloadRequest::new("")
        };
        Ok::<_, perseus_core::Error>(Some(
            run_download(Arc::clone(&client), request, job.cancel.clone(), reporter).await,
        ))
    }
    .await;

    match result {
        Ok(None) => {
            let state = if job.cancel.is_cancelled() {
                JobState::Cancelled
            } else {
                JobState::Completed
            };
            job.finish(state, None, None);
        }
        Ok(Some(report)) => {
            let state = if report.cancelled {
                JobState::Cancelled
            } else if report.fatal_error.is_some() {
                JobState::Failed
            } else if report.ok() {
                JobState::Completed
            } else {
                JobState::Partial
            };
            let error = report.fatal_error.clone().map(|message| JobError {
                code: report.fatal_error_code.unwrap_or("api"),
                message,
            });
            job.finish(state, Some(&report), error);
        }
        Err(perseus_core::Error::Cancelled) => job.finish(JobState::Cancelled, None, None),
        Err(err) => {
            tracing::error!(run_id = %job.id, error = %err, "job falhou");
            job.finish(JobState::Failed, None, Some(JobError::from(&err)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const ID: &str = "abcdefghijklmnopqrstuvwxyz012345";

    fn collector() -> (EventEmitter, Arc<Mutex<Vec<JobEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        (
            Arc::new(move |event: &JobEvent| lock(&sink).push(event.clone())),
            events,
        )
    }

    fn spec(url: &str, dir: &tempfile::TempDir) -> JobIn {
        serde_json::from_value(json!({
            "url": url,
            "output_dir": dir.path().display().to_string(),
            "workers": 2,
            "use_library": false,
        }))
        .expect("JobIn valido")
    }

    async fn playlist_server(delay: Duration) -> MockServer {
        let server = MockServer::start().await;
        let body = json!({"kind": "playlist", "title": "Argonautas", "user": {"username": "Perseus"}, "tracks": [
            {"id": 12, "title": "Protegida", "media": {"transcodings": [{"url": "https://api-v2.soundcloud.com/x",
             "format": {"protocol": "ctr-encrypted-hls", "mime_type": "audio/mp4"}}]}}]});
        Mock::given(path("/resolve"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(body)
                    .set_delay(delay),
            )
            .mount(&server)
            .await;
        server
    }

    async fn wait_finished(manager: &JobManager, id: &str) -> JobOut {
        for _ in 0..200 {
            let job = manager.get(id).expect("job existe").snapshot();
            if job.state != JobState::Running {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("job {id} nao terminou");
    }

    #[allow(
        clippy::too_many_lines,
        reason = "um exemplo por variante; a lista e o proprio contrato"
    )]
    fn logged_event_samples() -> Vec<Event> {
        let samples = vec![
            Event::Planned {
                album: "Argonautas".into(),
                artist: "Perseus".into(),
                selected: 3,
                total: 12,
                target_dir: "C:/Musica/Perseus - Argonautas".into(),
            },
            Event::ResolveFailed {
                url: "https://soundcloud.com/a/b".into(),
                reason: "Recurso nao encontrado".into(),
            },
            Event::TrackDownloaded {
                track_id: 1,
                file: "01. Perseus - Um.mp3".into(),
                bytes: 4_000_000,
                attempts: 2,
                elapsed_s: 1.5,
                protocol: "hls".into(),
            },
            Event::TrackReused {
                track_id: 2,
                file: "02. Perseus - Dois.mp3".into(),
            },
            Event::TrackCopied {
                track_id: 3,
                file: "03. Perseus - Tres.mp3".into(),
                source: "C:/Outra/Tres.mp3".into(),
            },
            Event::TrackMoved {
                track_id: 4,
                file: "04. Perseus - Quatro.mp3".into(),
            },
            Event::PlaylistFileWritten {
                file: "Argonautas.m3u8".into(),
                tracks: 12,
            },
            Event::TrackRetry {
                track_id: 5,
                title: "Cinco".into(),
                attempt: 1,
                max: 3,
                reason: "timeout".into(),
                delay_s: 1.2,
            },
            Event::TrackUnavailable {
                track_id: 6,
                reason: perseus_core::shared::soundcloud::transcoding::REASON_DRM.into(),
            },
            Event::TrackFailed {
                track_id: 7,
                reason: "falha de rede".into(),
            },
            Event::Finished {
                downloaded: 9,
                reused: 1,
                failed: 1,
                unavailable: 1,
                bytes: 40_000_000,
                elapsed_s: 12.3,
            },
            Event::Cancelled,
            Event::WatchStarted {
                title: "Argonautas".into(),
                artist: "Perseus".into(),
                tracks: 12,
                interval: 60,
            },
            Event::WatchNewTracks { count: 2 },
            Event::WatchIdle { check: 3 },
            Event::WatchCheckFailed {
                check: 4,
                errors: 2,
                reason: "HTTP 503".into(),
            },
            Event::WatchSyncFailed {
                reason: "sem espaco".into(),
            },
            Event::WatchTrackAbandoned {
                track_id: 8,
                attempts: 3,
                reason: "timeout".into(),
            },
            Event::WatchStopped,
        ];
        for event in &samples {
            match event {
                Event::Planned { .. }
                | Event::ResolveFailed { .. }
                | Event::TrackDownloaded { .. }
                | Event::TrackReused { .. }
                | Event::TrackCopied { .. }
                | Event::TrackMoved { .. }
                | Event::PlaylistFileWritten { .. }
                | Event::TrackRetry { .. }
                | Event::TrackUnavailable { .. }
                | Event::TrackFailed { .. }
                | Event::Finished { .. }
                | Event::Cancelled
                | Event::WatchStarted { .. }
                | Event::WatchNewTracks { .. }
                | Event::WatchIdle { .. }
                | Event::WatchCheckFailed { .. }
                | Event::WatchSyncFailed { .. }
                | Event::WatchTrackAbandoned { .. }
                | Event::WatchStopped => {}
                Event::TrackProgress { .. } => unreachable!("progresso nao e registrado"),
            }
        }
        samples
    }

    fn contracts_dir() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("contracts")
    }

    #[test]
    fn contract_log_event_details_match_the_shared_fixture() {
        let mut details: Vec<Value> = logged_event_samples().iter().map(event_detail).collect();
        details.push(json!({ "event": "job_failed", "code": "insufficient_space", "reason": "Espaco insuficiente" }));
        let rendered = format!(
            "{}\n",
            serde_json::to_string_pretty(&details).expect("json")
        );
        let path = contracts_dir().join("log-events.json");
        if std::env::var_os("UPDATE_CONTRACTS").is_some() {
            std::fs::create_dir_all(contracts_dir()).expect("pasta de contratos");
            std::fs::write(&path, &rendered).expect("gravar contrato");
        }
        let stored = std::fs::read_to_string(&path)
            .expect("contracts/log-events.json (UPDATE_CONTRACTS=1 para gerar)");
        assert_eq!(
            stored.replace("\r\n", "\n"),
            rendered,
            "evento mudou: atualize o contrato e as traducoes do front"
        );
    }

    #[test]
    fn contract_job_in_fixture_is_accepted_by_the_backend() {
        let raw = std::fs::read_to_string(contracts_dir().join("job-in.json"))
            .expect("contracts/job-in.json");
        let spec: JobIn = serde_json::from_str(&raw).expect("payload do front desserializa");
        spec.validated()
            .expect("payload do front passa na validacao do IPC");
    }

    #[tokio::test]
    async fn job_lifecycle_emits_ordered_events() {
        let server = playlist_server(Duration::ZERO).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = JobManager::new(Arc::new(SoundCloudClient::for_tests(
            &server.uri(),
            Some(ID.into()),
        )));
        let (emit, events) = collector();

        let created = manager
            .create(
                spec("https://soundcloud.com/perseus/sets/argonautas", &dir),
                emit,
            )
            .expect("job");
        assert_eq!(created.state, JobState::Running);
        let done = wait_finished(&manager, &created.id).await;

        assert_eq!(done.state, JobState::Completed);
        assert_eq!(done.title.as_deref(), Some("Argonautas"));
        assert_eq!(
            (done.counts.unavailable, done.counts.selected),
            (1, Some(1))
        );
        assert!(
            done.target_dir
                .as_deref()
                .is_some_and(|d| d.ends_with("Perseus - Argonautas"))
        );
        assert_eq!(
            done.report.as_ref().expect("relatorio")["summary"]["ok"],
            true
        );

        let events = lock(&events).clone();
        let ids: Vec<u64> = events.iter().map(|e| e.id).collect();
        assert!(
            ids.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "ids sequenciais: {ids:?}"
        );
        assert_eq!(events.last().map(|e| e.event), Some("end"));
        assert!(events.iter().any(|e| e.event == "track"
            && e.data["status"] == "unavailable"
            && e.data["reason_code"] == "drm"));
        assert!(events.iter().any(|e| e.event == "log"
            && e.data["detail"]["event"] == "track_unavailable"
            && e.data["detail"]["reason_code"] == "drm"));
        assert!(events.iter().any(|e| {
            e.event == "log"
                && e.data["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("indisponivel"))
        }));

        let job = manager.get(&created.id).expect("job");
        assert_eq!(job.events_after(0).len(), events.len());
        assert_eq!(job.events_after(ids[ids.len() - 2]).len(), 1);
    }

    #[tokio::test]
    async fn invalid_url_fails_the_job_with_the_domain_message() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = JobManager::new(Arc::new(SoundCloudClient::for_tests(
            "http://127.0.0.1:9",
            Some(ID.into()),
        )));
        let (emit, _) = collector();
        let created = manager
            .create(spec("https://evil.example/a/b", &dir), emit)
            .expect("job");
        let done = wait_finished(&manager, &created.id).await;
        assert_eq!(done.state, JobState::Failed);
        assert!(done.error.expect("erro").contains("Dominio nao suportado"));
    }

    #[tokio::test]
    async fn enforces_concurrency_limit_and_cancels() {
        let server = playlist_server(Duration::from_secs(30)).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let manager = JobManager::new(Arc::new(SoundCloudClient::for_tests(
            &server.uri(),
            Some(ID.into()),
        )));
        let url = "https://soundcloud.com/perseus/sets/argonautas";

        let ids: Vec<String> = (0..MAX_ACTIVE_JOBS)
            .map(|_| {
                manager
                    .create(spec(url, &dir), collector().0)
                    .expect("job")
                    .id
            })
            .collect();
        let rejected = manager
            .create(spec(url, &dir), collector().0)
            .expect_err("limite");
        assert!(matches!(rejected.kind, CommandErrorKind::TooManyJobs));

        manager.cancel_all();
        for id in &ids {
            assert_eq!(wait_finished(&manager, id).await.state, JobState::Cancelled);
        }
        assert_eq!(manager.list().len(), MAX_ACTIVE_JOBS);
        assert!(
            manager.create(spec(url, &dir), collector().0).is_ok(),
            "vagas liberadas apos cancelar"
        );
        manager.cancel_all();
        assert!(manager.get("inexistente").is_none());
    }
}
