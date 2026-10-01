use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::events::{Event, Reporter};
use crate::features::download::{DownloadOptions, DownloadReport, DownloadRequest, run_download};
use crate::shared::config::{
    DEFAULT_DOWNLOAD_WORKERS, WATCH_MAX_BACKOFF, WATCH_MAX_INTERVAL, WATCH_MAX_TRACK_ATTEMPTS,
    WATCH_MIN_INTERVAL, default_output_dir,
};
use crate::shared::error::{Error, Result};
use crate::shared::retry::{backoff_delay, sleep_cancellable};
use crate::shared::soundcloud::client::SoundCloudClient;
use crate::shared::soundcloud::models::{Playlist, Resource};

#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub url: String,
    pub output_dir: PathBuf,
    pub interval: u64,
    pub workers: usize,
    pub max_track_attempts: u32,
    pub options: DownloadOptions,
}

impl WatchConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            output_dir: default_output_dir(),
            interval: 60,
            workers: DEFAULT_DOWNLOAD_WORKERS,
            max_track_attempts: WATCH_MAX_TRACK_ATTEMPTS,
            options: DownloadOptions::default(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if !(WATCH_MIN_INTERVAL..=WATCH_MAX_INTERVAL).contains(&self.interval) {
            return Err(Error::InvalidInput(format!(
                "interval deve estar entre {WATCH_MIN_INTERVAL} e {WATCH_MAX_INTERVAL}s"
            )));
        }
        if self.max_track_attempts < 1 {
            return Err(Error::InvalidInput(
                "max_track_attempts deve ser >= 1".into(),
            ));
        }
        self.options.validate()
    }
}

#[derive(Debug, Default)]
struct WatchState {
    settled: HashSet<i64>,
    attempts: HashMap<i64, u32>,
    checks: u64,
    consecutive_errors: u32,
}

impl WatchState {
    fn settle(
        &mut self,
        track_ids: &HashSet<i64>,
        report: &DownloadReport,
        max_attempts: u32,
    ) -> Vec<(i64, u32, String)> {
        self.settled
            .extend(report.settled_ids().intersection(track_ids));
        let mut abandoned = Vec::new();
        let mut pending: Vec<i64> = track_ids.difference(&self.settled).copied().collect();
        pending.sort_unstable();
        for track_id in pending {
            let attempts = self.attempts.entry(track_id).or_insert(0);
            *attempts += 1;
            if *attempts >= max_attempts {
                self.settled.insert(track_id);
                let reason = report
                    .failed
                    .get(&track_id)
                    .or(report.fatal_error.as_ref())
                    .cloned()
                    .unwrap_or_else(|| "nao processada".to_owned());
                abandoned.push((track_id, *attempts, reason));
            }
        }
        abandoned
    }
}

pub struct PlaylistWatcher {
    config: WatchConfig,
    client: Arc<SoundCloudClient>,
    reporter: Reporter,
    cancel: CancellationToken,
    state: WatchState,
}

impl PlaylistWatcher {
    pub fn new(
        config: WatchConfig,
        client: Arc<SoundCloudClient>,
        reporter: Reporter,
        cancel: CancellationToken,
    ) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            client,
            reporter,
            cancel,
            state: WatchState::default(),
        })
    }

    pub async fn run(mut self) -> Result<()> {
        let playlist = self.resolve_playlist().await?;
        self.reporter.emit(&Event::WatchStarted {
            title: playlist.display_title(),
            artist: playlist.artist(),
            tracks: playlist.tracks.len(),
            interval: self.config.interval,
        });
        self.sync(playlist.track_ids().into_iter().collect(), true)
            .await;

        while !self.cancel.is_cancelled() {
            if sleep_cancellable(self.next_delay(), &self.cancel)
                .await
                .is_err()
            {
                break;
            }
            self.state.checks += 1;
            let playlist = match self.resolve_playlist().await {
                Ok(playlist) => playlist,
                Err(Error::Cancelled) => break,
                Err(err) => {
                    self.state.consecutive_errors += 1;
                    self.reporter.emit(&Event::WatchCheckFailed {
                        check: self.state.checks,
                        errors: self.state.consecutive_errors,
                        reason: err.to_string(),
                    });
                    continue;
                }
            };
            self.state.consecutive_errors = 0;
            let pending: HashSet<i64> = playlist
                .track_ids()
                .into_iter()
                .filter(|id| !self.state.settled.contains(id))
                .collect();
            if pending.is_empty() {
                self.reporter.emit(&Event::WatchIdle {
                    check: self.state.checks,
                });
            } else {
                self.reporter.emit(&Event::WatchNewTracks {
                    count: pending.len(),
                });
                self.sync(pending, false).await;
            }
        }
        self.reporter.emit(&Event::WatchStopped);
        Ok(())
    }

    fn next_delay(&self) -> Duration {
        let interval = Duration::from_secs(self.config.interval);
        match self.state.consecutive_errors {
            0 => interval,
            errors => backoff_delay(errors + 1, interval, WATCH_MAX_BACKOFF),
        }
    }

    async fn resolve_playlist(&self) -> Result<Playlist> {
        let resolved = tokio::select! {
            biased;
            () = self.cancel.cancelled() => return Err(Error::Cancelled),
            resolved = self.client.resolve(&self.config.url) => resolved?,
        };
        match resolved {
            Resource::Playlist(playlist) => Ok(*playlist),
            Resource::Track(_) | Resource::Collection(_) => Err(Error::UnsupportedResource(
                "O modo monitoramento exige uma playlist, album, curtidas ou faixas de um perfil."
                    .into(),
            )),
        }
    }

    async fn sync(&mut self, track_ids: HashSet<i64>, initial: bool) {
        if track_ids.is_empty() {
            return;
        }
        let request = DownloadRequest {
            url: self.config.url.clone(),
            output_dir: self.config.output_dir.clone(),
            limit: None,
            only_track_ids: (!initial).then(|| track_ids.clone()),
            workers: self.config.workers,
            options: self.config.options.clone(),
            ..DownloadRequest::new("")
        };
        let report = run_download(
            Arc::clone(&self.client),
            request,
            self.cancel.clone(),
            self.reporter.clone(),
        )
        .await;
        if report.cancelled {
            return;
        }
        if let Some(reason) = &report.fatal_error {
            self.reporter.emit(&Event::WatchSyncFailed {
                reason: reason.clone(),
            });
        }
        for (track_id, attempts, reason) in
            self.state
                .settle(&track_ids, &report, self.config.max_track_attempts)
        {
            self.reporter.emit(&Event::WatchTrackAbandoned {
                track_id,
                attempts,
                reason,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::download::CompletedDownload;

    fn done(track_id: i64) -> CompletedDownload {
        CompletedDownload {
            track_id,
            title: String::new(),
            path: String::new(),
            size_bytes: 1,
            reused: false,
            from_library: false,
            attempts: 1,
            elapsed_seconds: 0.0,
        }
    }

    #[test]
    fn failures_are_retried_then_abandoned() {
        let mut state = WatchState::default();
        let ids = HashSet::from([1, 2, 3]);
        let mut report = DownloadReport {
            completed: vec![done(1)],
            ..DownloadReport::default()
        };
        report.unavailable.insert(2, "DRM".into());
        report.failed.insert(3, "timeout".into());

        assert!(state.settle(&ids, &report, 2).is_empty());
        assert_eq!(state.settled, HashSet::from([1, 2]));

        let abandoned = state.settle(&HashSet::from([3]), &report, 2);
        assert_eq!(abandoned, vec![(3, 2, "timeout".to_owned())]);
        assert!(state.settled.contains(&3));
    }

    #[test]
    fn validates_interval() {
        let mut config = WatchConfig::new("https://soundcloud.com/a/sets/b");
        assert!(config.validate().is_ok());
        config.interval = 5;
        assert!(config.validate().is_err());
    }
}

#[cfg(test)]
mod loop_tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::test_support::{client, collecting_reporter, mount_media, track};

    fn playlist(base: &str, ids: &[i64]) -> serde_json::Value {
        let tracks: Vec<_> = ids
            .iter()
            .map(|id| track(*id, &format!("Faixa {id}"), "progressive", base))
            .collect();
        json!({"kind": "playlist", "title": "Vigia", "user": {"username": "Perseus"}, "tracks": tracks})
    }

    fn watcher(
        server: &MockServer,
        dir: &tempfile::TempDir,
        reporter: Reporter,
        cancel: CancellationToken,
    ) -> PlaylistWatcher {
        let mut config = WatchConfig::new("https://soundcloud.com/perseus/sets/vigia");
        config.output_dir = dir.path().to_path_buf();
        config.interval = 1;
        config.options.use_library = false;
        PlaylistWatcher {
            config,
            client: client(server),
            reporter,
            cancel,
            state: WatchState::default(),
        }
    }

    async fn run_until(
        watcher: PlaylistWatcher,
        events: &Arc<Mutex<Vec<Event>>>,
        cancel: &CancellationToken,
        done: impl Fn(&[Event]) -> bool,
    ) -> Result<()> {
        let handle = tokio::spawn(watcher.run());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        while !done(&events.lock().expect("lock")) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "watch nao chegou ao estado esperado"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        cancel.cancel();
        handle.await.expect("tarefa do watch")
    }

    #[tokio::test]
    async fn downloads_tracks_added_after_the_first_sync() {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(playlist(&base, &[1])))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(playlist(&base, &[1, 2])))
            .mount(&server)
            .await;
        mount_media(&server).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let (reporter, events) = collecting_reporter();
        let cancel = CancellationToken::new();

        let outcome = run_until(
            watcher(&server, &dir, reporter, cancel.clone()),
            &events,
            &cancel,
            |seen| {
                seen.iter()
                    .any(|e| matches!(e, Event::TrackDownloaded { track_id: 2, .. }))
            },
        )
        .await;

        assert!(outcome.is_ok(), "{outcome:?}");
        let seen = events.lock().expect("lock");
        assert!(
            seen.iter()
                .any(|e| matches!(e, Event::WatchStarted { tracks: 1, .. }))
        );
        assert!(
            seen.iter()
                .any(|e| matches!(e, Event::WatchNewTracks { count: 1 }))
        );
        let first_downloads = seen
            .iter()
            .filter(|e| matches!(e, Event::TrackDownloaded { track_id: 1, .. }))
            .count();
        assert_eq!(
            first_downloads, 1,
            "faixa ja sincronizada nao e baixada de novo"
        );
        let folder = dir.path().join("Perseus - Vigia");
        assert!(folder.join("02. Perseus - Faixa 2.mp3").is_file());
    }

    #[tokio::test]
    async fn failed_check_is_reported_and_backs_off() {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(playlist(&base, &[1])))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        mount_media(&server).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let (reporter, events) = collecting_reporter();
        let cancel = CancellationToken::new();
        let mut probe = watcher(
            &server,
            &dir,
            Reporter::silent("probe"),
            CancellationToken::new(),
        );
        probe.state.consecutive_errors = 3;
        assert!(
            probe.next_delay() > Duration::from_secs(1),
            "erros seguidos aumentam a espera"
        );

        let outcome = run_until(
            watcher(&server, &dir, reporter, cancel.clone()),
            &events,
            &cancel,
            |seen| {
                seen.iter()
                    .any(|e| matches!(e, Event::WatchCheckFailed { errors: 1, .. }))
            },
        )
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(
            events
                .lock()
                .expect("lock")
                .iter()
                .any(|e| matches!(e, Event::WatchStopped))
        );
    }

    #[tokio::test]
    async fn refuses_resources_that_are_not_playlists() {
        let server = MockServer::start().await;
        Mock::given(path("/resolve"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"kind": "track", "id": 1, "title": "t"})),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tempdir");
        let cancel = CancellationToken::new();
        let err = watcher(&server, &dir, Reporter::silent("t"), cancel)
            .run()
            .await
            .expect_err("faixa avulsa nao pode ser monitorada");
        assert!(matches!(err, Error::UnsupportedResource(_)), "{err:?}");
    }
}
