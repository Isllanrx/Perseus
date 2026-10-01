use std::sync::Arc;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Level {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Planned {
        album: String,
        artist: String,
        selected: usize,
        total: usize,
        target_dir: String,
    },
    ResolveFailed {
        url: String,
        reason: String,
    },
    TrackProgress {
        track_id: i64,
        bytes: u64,
        fraction: Option<f64>,
    },
    TrackDownloaded {
        track_id: i64,
        file: String,
        bytes: u64,
        attempts: u32,
        elapsed_s: f64,
        protocol: String,
    },
    TrackReused {
        track_id: i64,
        file: String,
    },
    TrackCopied {
        track_id: i64,
        file: String,
        source: String,
    },
    TrackMoved {
        track_id: i64,
        file: String,
    },
    PlaylistFileWritten {
        file: String,
        tracks: usize,
    },
    TrackRetry {
        track_id: i64,
        title: String,
        attempt: u32,
        max: u32,
        reason: String,
        delay_s: f64,
    },
    TrackUnavailable {
        track_id: i64,
        reason: String,
    },
    TrackFailed {
        track_id: i64,
        reason: String,
    },
    Finished {
        downloaded: usize,
        reused: usize,
        failed: usize,
        unavailable: usize,
        bytes: u64,
        elapsed_s: f64,
    },
    Cancelled,
    WatchStarted {
        title: String,
        artist: String,
        tracks: usize,
        interval: u64,
    },
    WatchNewTracks {
        count: usize,
    },
    WatchIdle {
        check: u64,
    },
    WatchCheckFailed {
        check: u64,
        errors: u32,
        reason: String,
    },
    WatchSyncFailed {
        reason: String,
    },
    WatchTrackAbandoned {
        track_id: i64,
        attempts: u32,
        reason: String,
    },
    WatchStopped,
}

impl Event {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Planned { .. } => "download_planned",
            Self::ResolveFailed { .. } => "resolve_failed",
            Self::TrackProgress { .. } => "track_progress",
            Self::TrackDownloaded { .. } => "track_downloaded",
            Self::TrackReused { .. } => "track_reused",
            Self::TrackCopied { .. } => "track_copied",
            Self::TrackMoved { .. } => "track_moved",
            Self::PlaylistFileWritten { .. } => "playlist_file_written",
            Self::TrackRetry { .. } => "download_retry",
            Self::TrackUnavailable { .. } => "track_unavailable",
            Self::TrackFailed { .. } => "track_failed",
            Self::Finished { .. } => "download_finished",
            Self::Cancelled => "download_cancelled",
            Self::WatchStarted { .. } => "watch_started",
            Self::WatchNewTracks { .. } => "watch_new_tracks",
            Self::WatchIdle { .. } => "watch_idle",
            Self::WatchCheckFailed { .. } => "watch_check_failed",
            Self::WatchSyncFailed { .. } => "watch_sync_failed",
            Self::WatchTrackAbandoned { .. } => "watch_track_abandoned",
            Self::WatchStopped => "watch_stopped",
        }
    }

    pub fn level(&self) -> Level {
        match self {
            Self::TrackProgress { .. } => Level::Debug,
            Self::TrackRetry { .. }
            | Self::TrackUnavailable { .. }
            | Self::TrackFailed { .. }
            | Self::Cancelled
            | Self::WatchCheckFailed { .. } => Level::Warning,
            Self::ResolveFailed { .. }
            | Self::WatchSyncFailed { .. }
            | Self::WatchTrackAbandoned { .. } => Level::Error,
            _ => Level::Info,
        }
    }

    pub fn track_id(&self) -> Option<i64> {
        match self {
            Self::TrackProgress { track_id, .. }
            | Self::TrackDownloaded { track_id, .. }
            | Self::TrackReused { track_id, .. }
            | Self::TrackCopied { track_id, .. }
            | Self::TrackMoved { track_id, .. }
            | Self::TrackRetry { track_id, .. }
            | Self::TrackUnavailable { track_id, .. }
            | Self::TrackFailed { track_id, .. }
            | Self::WatchTrackAbandoned { track_id, .. } => Some(*track_id),
            _ => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Planned {
                album,
                artist,
                selected,
                total,
                target_dir,
            } => {
                format!("'{album}' por '{artist}': {selected} de {total} faixa(s) -> {target_dir}")
            }
            Self::ResolveFailed { url, reason } => format!("Falha ao resolver {url}: {reason}"),
            Self::TrackProgress {
                track_id,
                bytes,
                fraction,
            } => match fraction {
                Some(f) => format!("Faixa {track_id}: {:.0}%", f * 100.0),
                None => format!("Faixa {track_id}: {bytes} bytes"),
            },
            Self::TrackDownloaded { file, .. } => format!("Concluido: {file}"),
            Self::TrackReused { file, .. } => format!("Ja existe: {file}"),
            Self::TrackCopied { file, source, .. } => {
                format!("Copiada da biblioteca local: {file} (de {source})")
            }
            Self::TrackMoved { file, .. } => {
                format!("Saiu da playlist; movida para Removed: {file}")
            }
            Self::PlaylistFileWritten { file, tracks } => {
                format!("Playlist {file} gravada com {tracks} faixa(s)")
            }
            Self::TrackRetry {
                title,
                attempt,
                max,
                reason,
                delay_s,
                ..
            } => {
                format!(
                    "Tentativa {attempt}/{max} falhou para '{title}': {reason}. Nova tentativa em {delay_s:.1}s"
                )
            }
            Self::TrackUnavailable { track_id, reason } => {
                format!("Faixa {track_id} indisponivel: {reason}")
            }
            Self::TrackFailed { track_id, reason } => format!("Faixa {track_id} falhou: {reason}"),
            Self::Finished {
                downloaded,
                reused,
                failed,
                unavailable,
                elapsed_s,
                ..
            } => format!(
                "Download finalizado: {downloaded} baixada(s), {reused} reaproveitada(s), {failed} com falha, \
                 {unavailable} indisponivel(is) em {elapsed_s:.1}s"
            ),
            Self::Cancelled => "Download cancelado pelo usuario".to_owned(),
            Self::WatchStarted {
                title,
                artist,
                tracks,
                interval,
            } => {
                format!(
                    "Monitorando '{title}' por '{artist}': {tracks} faixas, intervalo {interval}s"
                )
            }
            Self::WatchNewTracks { count } => {
                format!("{count} faixa(s) nova(s) ou pendente(s) detectada(s)")
            }
            Self::WatchIdle { check } => format!("Verificacao #{check}: nenhuma novidade"),
            Self::WatchCheckFailed {
                check,
                errors,
                reason,
            } => {
                format!("Verificacao #{check} falhou ({errors} erro(s) seguidos): {reason}")
            }
            Self::WatchSyncFailed { reason } => format!("Sincronizacao falhou: {reason}"),
            Self::WatchTrackAbandoned {
                track_id,
                attempts,
                reason,
            } => {
                format!("Desistindo da faixa {track_id} apos {attempts} tentativa(s): {reason}")
            }
            Self::WatchStopped => "Monitoramento encerrado".to_owned(),
        }
    }
}

pub type Sink = Arc<dyn Fn(&Event) + Send + Sync>;

#[derive(Clone)]
pub struct Reporter {
    run_id: Arc<str>,
    sink: Option<Sink>,
}

impl Reporter {
    pub fn new(run_id: impl Into<Arc<str>>, sink: Sink) -> Self {
        Self {
            run_id: run_id.into(),
            sink: Some(sink),
        }
    }

    pub fn silent(run_id: impl Into<Arc<str>>) -> Self {
        Self {
            run_id: run_id.into(),
            sink: None,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn emit(&self, event: &Event) {
        let (run_id, name, track_id) = (&*self.run_id, event.name(), event.track_id());
        match event.level() {
            Level::Debug => tracing::debug!(run_id, event = name, track_id, "{}", event.message()),
            Level::Info => tracing::info!(run_id, event = name, track_id, "{}", event.message()),
            Level::Warning => tracing::warn!(run_id, event = name, track_id, "{}", event.message()),
            Level::Error => tracing::error!(run_id, event = name, track_id, "{}", event.message()),
        }
        if let Some(sink) = &self.sink {
            sink(event);
        }
    }
}

pub fn new_run_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}
