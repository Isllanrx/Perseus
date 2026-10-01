use perseus_core::InspectResult;
use perseus_core::features::download::models::DownloadJob;
use perseus_core::features::download::{RemoteFolder, RemoteItem};
use perseus_core::shared::soundcloud::models::{Resource, artwork_500};
use perseus_core::shared::soundcloud::transcoding::{Container, Quality, reason_code};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Playlist,
    Track,
    Watch,
}

#[derive(Debug, Serialize)]
pub struct ConfigOut {
    pub version: &'static str,
    pub default_output_dir: String,
    pub max_workers: usize,
    pub default_workers: usize,
    pub interval_min: u64,
    pub interval_max: u64,
}

#[derive(Debug, Deserialize)]
pub struct InspectIn {
    pub url: String,
    pub mode: Mode,
}

#[derive(Debug, Deserialize)]
pub struct SearchIn {
    pub query: String,
}

#[derive(Debug, Serialize)]
pub struct TrackOut {
    pub id: i64,
    pub position: usize,
    pub title: String,
    pub artist: String,
    pub duration_ms: Option<i64>,
    pub artwork_url: Option<String>,
    pub permalink_url: Option<String>,
    pub available: bool,
    pub reason: Option<String>,
    pub reason_code: Option<&'static str>,
    pub group: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InspectOut {
    pub kind: &'static str,
    pub url: String,
    pub playlist_context: Option<String>,
    pub title: String,
    pub artist: String,
    pub artwork_url: Option<String>,
    pub permalink_url: Option<String>,
    pub total_tracks: usize,
    pub tracks: Vec<TrackOut>,
}

impl InspectOut {
    pub fn new(result: &InspectResult, url: String, playlist_context: Option<String>) -> Self {
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
        let first_artwork = || tracks.first().and_then(|t| t.artwork_url.clone());
        let (kind, artwork_url) = match &result.resource {
            Resource::Track(track) => ("track", track.best_artwork_url()),
            Resource::Playlist(playlist) => (
                playlist.source.code(),
                artwork_500(playlist.artwork_url.as_deref()).or_else(first_artwork),
            ),
            Resource::Collection(collection) => (
                collection.kind.code(),
                collection
                    .playlists
                    .iter()
                    .find_map(|p| artwork_500(p.artwork_url.as_deref()))
                    .or_else(first_artwork),
            ),
        };
        Self {
            kind,
            url,
            playlist_context,
            title: result.resource.display_title(),
            artist: result.resource.artist(),
            artwork_url,
            permalink_url: result.resource.permalink_url().map(str::to_owned),
            total_tracks: result.total_tracks,
            tracks,
        }
    }
}

fn enabled() -> bool {
    true
}

fn playlist_mode() -> Mode {
    Mode::Playlist
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanIn {
    pub url: String,
    #[serde(default = "playlist_mode")]
    pub mode: Mode,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub quality: Quality,
    #[serde(default)]
    pub name_template: Option<String>,
    #[serde(default)]
    pub min_duration_s: Option<u32>,
    #[serde(default)]
    pub max_duration_s: Option<u32>,
    #[serde(default = "enabled")]
    pub write_playlist_file: bool,
    #[serde(default)]
    pub original_artwork: bool,
}

#[derive(Debug, Serialize)]
pub struct PlanOut {
    pub url: String,
    pub title: String,
    pub folders: Vec<FolderOut>,
}

#[derive(Debug, Serialize)]
pub struct FolderOut {
    pub folder: String,
    pub album: String,
    pub owner: String,
    pub single: bool,
    pub total_tracks: usize,
    pub playlist_file: Option<String>,
    pub items: Vec<ItemOut>,
}

impl From<RemoteFolder> for FolderOut {
    fn from(folder: RemoteFolder) -> Self {
        Self {
            folder: folder.folder,
            album: folder.album,
            owner: folder.owner,
            single: folder.single,
            total_tracks: folder.total_tracks,
            playlist_file: folder.playlist_file,
            items: folder.items.into_iter().map(ItemOut::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ItemOut {
    Ready(Box<ReadyOut>),
    Unavailable {
        track_id: i64,
        reason: String,
        reason_code: Option<&'static str>,
    },
    Failed {
        track_id: i64,
        reason: String,
        reason_code: Option<&'static str>,
    },
}

impl From<RemoteItem> for ItemOut {
    fn from(item: RemoteItem) -> Self {
        match item {
            RemoteItem::Ready(job) => Self::Ready(Box::new(ReadyOut::from(*job))),
            RemoteItem::Unavailable { track_id, reason } => Self::Unavailable {
                track_id,
                reason_code: reason_code(&reason),
                reason,
            },
            RemoteItem::Failed { track_id, reason } => Self::Failed {
                track_id,
                reason_code: reason_code(&reason),
                reason,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReadyOut {
    pub track_id: i64,
    pub file_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: usize,
    pub total_tracks: usize,
    pub duration_ms: Option<i64>,
    pub permalink_url: Option<String>,
    pub artwork_url: Option<String>,
    pub artwork_fallback_url: Option<String>,
    pub transcoding_url: String,
    pub track_authorization: Option<String>,
    pub protocol: String,
    pub container: &'static str,
    pub estimated_kbps: u32,
    pub tags: TagsOut,
}

#[derive(Debug, Serialize)]
pub struct TagsOut {
    pub artist: Option<String>,
    pub genre: Option<String>,
    pub isrc: Option<String>,
    pub label: Option<String>,
    pub composer: Option<String>,
    pub copyright: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u16>,
}

fn container_code(container: Container) -> &'static str {
    match container {
        Container::Mp3 => "mp3",
        Container::Mp4 => "mp4",
        Container::Ogg => "ogg",
    }
}

impl From<DownloadJob> for ReadyOut {
    fn from(job: DownloadJob) -> Self {
        Self {
            track_id: job.track_id,
            file_name: job.naming.file_name(job.format.extension),
            title: job.title,
            artist: job.artist,
            album: job.album,
            track_number: job.track_number,
            total_tracks: job.total_tracks,
            duration_ms: job.duration_ms,
            permalink_url: job.permalink_url,
            artwork_url: job.artwork_url,
            artwork_fallback_url: job.artwork_fallback_url,
            transcoding_url: job.transcoding_url,
            track_authorization: job.track_authorization,
            protocol: job.protocol,
            container: container_code(job.format.container),
            estimated_kbps: job.estimated_kbps,
            tags: TagsOut {
                artist: job.tags.artist,
                genre: job.tags.genre,
                isrc: job.tags.isrc,
                label: job.tags.label,
                composer: job.tags.composer,
                copyright: job.tags.copyright,
                album_artist: job.tags.album_artist,
                year: job.tags.year,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamIn {
    pub transcoding_url: String,
    #[serde(default)]
    pub track_authorization: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum StreamOut {
    Progressive { url: String },
    Hls { parts: Vec<String> },
}
