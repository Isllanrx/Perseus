#![allow(
    clippy::needless_pass_by_value,
    reason = "o macro #[tauri::command] exige argumentos por valor"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use perseus_core::shared::config::{
    DEFAULT_DOWNLOAD_WORKERS, MAX_DOWNLOAD_WORKERS, WATCH_MAX_INTERVAL, WATCH_MIN_INTERVAL,
    default_output_dir,
};
use perseus_core::shared::soundcloud::models::{Resource, SearchHit, artwork_500};
use perseus_core::shared::soundcloud::transcoding::reason_code;
use perseus_core::shared::soundcloud::urls::playlist_context;
use perseus_core::{VERSION, inspect_url};
use tauri::{AppHandle, Emitter as _, State};
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::dto::{
    CommandError, CommandErrorKind, ConfigOut, InspectOut, JobEvent, JobIn, JobOut, Mode, TrackOut,
};
use crate::jobs::{EVENT_CHANNEL, EventEmitter, JobManager};

const INSPECT_LIMIT: usize = 500;
const SEARCH_LIMIT: usize = 20;

type CommandResult<T> = Result<T, CommandError>;

#[tauri::command]
pub fn get_config() -> ConfigOut {
    ConfigOut {
        version: VERSION,
        default_output_dir: default_output_dir().display().to_string(),
        max_workers: MAX_DOWNLOAD_WORKERS,
        default_workers: DEFAULT_DOWNLOAD_WORKERS,
        interval_min: WATCH_MIN_INTERVAL,
        interval_max: WATCH_MAX_INTERVAL,
    }
}

#[tauri::command]
pub async fn inspect(
    url: String,
    mode: Mode,
    jobs: State<'_, JobManager>,
) -> CommandResult<InspectOut> {
    let client = jobs.client();
    let canonical = client.canonicalize(url.trim()).await?;
    let context = playlist_context(&canonical);
    let target = match &context {
        Some(context) if mode != Mode::Track => context.clone(),
        _ => canonical,
    };
    let result = inspect_url(client, &target, INSPECT_LIMIT).await?;
    let tracks: Vec<TrackOut> = result
        .preview
        .iter()
        .enumerate()
        .map(|(index, preview)| TrackOut {
            id: preview.track.id,
            position: index + 1,
            title: preview.track.display_title(),
            artist: preview.track.artist(),
            duration_ms: preview.track.duration,
            artwork_url: preview.track.best_artwork_url(),
            permalink_url: preview.track.permalink_url.clone(),
            available: preview.unavailable_reason.is_none(),
            reason: preview.unavailable_reason.clone(),
            reason_code: preview.unavailable_reason.as_deref().and_then(reason_code),
            group: preview.group.clone(),
        })
        .collect();
    let (kind, artwork) = match &result.resource {
        Resource::Track(track) => ("track", track.best_artwork_url()),
        Resource::Playlist(playlist) => (
            playlist.source.code(),
            artwork_500(playlist.artwork_url.as_deref())
                .or_else(|| tracks.first().and_then(|t| t.artwork_url.clone())),
        ),
        Resource::Collection(collection) => (
            collection.kind.code(),
            collection
                .playlists
                .iter()
                .find_map(|p| artwork_500(p.artwork_url.as_deref()))
                .or_else(|| tracks.first().and_then(|t| t.artwork_url.clone())),
        ),
    };
    Ok(InspectOut {
        kind,
        url: target,
        playlist_context: context,
        title: result.resource.display_title(),
        artist: result.resource.artist(),
        artwork_url: artwork,
        permalink_url: result.resource.permalink_url().map(str::to_owned),
        total_tracks: result.total_tracks,
        tracks,
    })
}

#[tauri::command]
pub async fn search(query: String, jobs: State<'_, JobManager>) -> CommandResult<Vec<SearchHit>> {
    Ok(jobs.client().search(&query, SEARCH_LIMIT).await?)
}

#[tauri::command]
pub fn list_jobs(jobs: State<'_, JobManager>) -> Vec<JobOut> {
    jobs.list()
}

#[tauri::command]
pub fn create_job(
    spec: JobIn,
    app: AppHandle,
    jobs: State<'_, JobManager>,
) -> CommandResult<JobOut> {
    let emit: EventEmitter = Arc::new(move |event: &JobEvent| {
        if let Err(err) = app.emit(EVENT_CHANNEL, event) {
            tracing::debug!(error = %err, "evento nao entregue ao webview");
        }
    });
    jobs.create(spec.validated()?, emit)
}

#[tauri::command]
pub fn cancel_job(id: String, jobs: State<'_, JobManager>) -> CommandResult<JobOut> {
    let job = jobs.get(&id).ok_or_else(CommandError::not_found)?;
    job.cancel();
    Ok(job.snapshot())
}

#[tauri::command]
pub fn job_events(
    id: String,
    after: u64,
    jobs: State<'_, JobManager>,
) -> CommandResult<Vec<JobEvent>> {
    Ok(jobs
        .get(&id)
        .ok_or_else(CommandError::not_found)?
        .events_after(after))
}

#[tauri::command]
pub fn open_folder(id: String, app: AppHandle, jobs: State<'_, JobManager>) -> CommandResult<()> {
    let job = jobs.get(&id).ok_or_else(CommandError::not_found)?;
    let dir = job
        .target_dir()
        .filter(|dir| Path::new(dir).is_dir())
        .ok_or_else(|| {
            CommandError::of(
                CommandErrorKind::Conflict,
                "folder_missing",
                "A pasta desta tarefa ainda nao existe.",
            )
        })?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|err| {
            CommandError::of(
                CommandErrorKind::Internal,
                "open_folder_failed",
                format!("Nao foi possivel abrir a pasta: {err}"),
            )
        })
}

#[tauri::command]
pub async fn pick_output_dir(
    title: Option<String>,
    initial: Option<String>,
    app: AppHandle,
) -> CommandResult<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title.unwrap_or_else(|| "Perseus".to_owned()));
    if let Some(dir) = initial.map(PathBuf::from).filter(|dir| dir.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let folder = rx.await.map_err(|_| {
        CommandError::of(
            CommandErrorKind::Internal,
            "internal",
            "Seletor de pasta encerrado.",
        )
    })?;
    Ok(folder
        .and_then(|path| path.into_path().ok())
        .map(|path| path.display().to_string()))
}
