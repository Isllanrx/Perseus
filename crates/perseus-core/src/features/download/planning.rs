use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::events::{Event, Reporter};
use crate::features::download::library::Archive;
use crate::features::download::models::{DownloadJob, DownloadOptions, DownloadRequest, TrackTags};
use crate::features::download::naming::{
    FileNaming, SINGLES_FOLDER, TemplateValues, playlist_folder_name, render_template,
    track_display_name,
};
use crate::shared::error::{Error, Result};
use crate::shared::filesystem::{ensure_within, sanitize};
use crate::shared::soundcloud::models::{Playlist, Resource, Track};
use crate::shared::soundcloud::transcoding::{
    audio_format_for, choose_transcoding_with, estimated_kbps,
};

pub use crate::shared::soundcloud::transcoding::REASON_NOT_RETURNED as NOT_RETURNED_REASON;

#[derive(Debug)]
pub struct DownloadPlan {
    pub target_dir: PathBuf,
    pub album: String,
    pub owner: String,
    pub numbering: HashMap<i64, usize>,
    pub order: Vec<i64>,
    pub total_tracks: usize,
    pub selected: Vec<Track>,
    pub single: bool,
    pub archive: Archive,
}

impl DownloadPlan {
    pub fn stub_ids(&self) -> Vec<i64> {
        self.selected
            .iter()
            .filter(|t| t.is_stub())
            .map(|t| t.id)
            .collect()
    }
}

pub(crate) struct Folder {
    pub name: String,
    pub album: String,
    pub owner: String,
    pub tracks: Vec<Track>,
    pub single: bool,
}

pub(crate) fn folders(resource: Resource) -> Vec<Folder> {
    match resource {
        Resource::Track(track) => vec![Folder {
            name: SINGLES_FOLDER.to_owned(),
            album: track.display_title(),
            owner: track.artist(),
            tracks: vec![*track],
            single: true,
        }],
        Resource::Playlist(playlist) => vec![playlist_folder(*playlist)],
        Resource::Collection(collection) => collection
            .playlists
            .into_iter()
            .map(playlist_folder)
            .collect(),
    }
}

fn playlist_folder(playlist: Playlist) -> Folder {
    let album = playlist.display_title();
    let owner = playlist.artist();
    Folder {
        name: playlist_folder_name(&owner, &album),
        album,
        owner,
        tracks: playlist.tracks,
        single: false,
    }
}

pub(crate) struct Selection {
    pub numbering: HashMap<i64, usize>,
    pub order: Vec<i64>,
    pub total_tracks: usize,
    pub selected: Vec<Track>,
}

impl Selection {
    pub fn of(tracks: Vec<Track>, limit: Option<usize>, only: Option<&HashSet<i64>>) -> Self {
        let order: Vec<i64> = tracks.iter().map(|t| t.id).collect();
        let numbering = order
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, index + 1))
            .collect();
        let total_tracks = tracks.len();
        let selected = tracks
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .filter(|t| only.is_none_or(|only| only.contains(&t.id)))
            .collect();
        Self {
            numbering,
            order,
            total_tracks,
            selected,
        }
    }
}

pub async fn plan_download(
    resource: Resource,
    request: &DownloadRequest,
    reporter: &Reporter,
) -> Result<Vec<DownloadPlan>> {
    let mut plans = Vec::new();
    for folder in folders(resource) {
        plans.push(build_plan(request, reporter, folder).await?);
    }
    Ok(plans)
}

async fn build_plan(
    request: &DownloadRequest,
    reporter: &Reporter,
    folder: Folder,
) -> Result<DownloadPlan> {
    let selection = Selection::of(
        folder.tracks,
        request.limit,
        request.only_track_ids.as_ref(),
    );
    let target_dir = ensure_within(&request.output_dir, &request.output_dir.join(folder.name))?;
    tokio::fs::create_dir_all(&target_dir).await?;
    let archive = Archive::load(&target_dir);
    reporter.emit(&Event::Planned {
        album: folder.album.clone(),
        artist: folder.owner.clone(),
        selected: selection.selected.len(),
        total: selection.total_tracks,
        target_dir: target_dir.display().to_string(),
    });
    Ok(DownloadPlan::new(
        target_dir,
        folder.album,
        folder.owner,
        selection,
        folder.single,
        archive,
    ))
}

impl DownloadPlan {
    pub(crate) fn new(
        target_dir: PathBuf,
        album: String,
        owner: String,
        selection: Selection,
        single: bool,
        archive: Archive,
    ) -> Self {
        Self {
            target_dir,
            album,
            owner,
            numbering: selection.numbering,
            order: selection.order,
            total_tracks: selection.total_tracks.max(1),
            selected: selection.selected,
            single,
            archive,
        }
    }
}

pub fn playlist_file_name(album: &str) -> String {
    format!("{}.m3u8", sanitize(Some(album), "playlist"))
}

pub fn claim_unique_name(claimed: &mut HashMap<String, i64>, job: &mut DownloadJob) {
    let extension = job.format.extension;
    if let FileNaming::Template { stem } = &mut job.naming {
        let taken = |name: &str| {
            claimed
                .get(&name.to_lowercase())
                .is_some_and(|owner| *owner != job.track_id)
        };
        if taken(&format!("{stem}{extension}")) {
            *stem = format!("{stem} [{}]", job.track_id);
        }
    }
    claimed.insert(job.naming.file_name(extension).to_lowercase(), job.track_id);
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

pub fn build_job(
    plan: &DownloadPlan,
    track: &Track,
    options: &DownloadOptions,
) -> Result<DownloadJob> {
    let transcoding = choose_transcoding_with(track, options.quality)?;
    let format = audio_format_for(&transcoding.format.mime_type).ok_or_else(|| {
        Error::StreamUnavailable(format!(
            "formato nao suportado: {}",
            transcoding.format.mime_type
        ))
    })?;
    let title = track.display_title();
    let uploader = track.artist();
    let publisher = track.publisher_metadata.clone().unwrap_or_default();
    let track_number = plan.numbering.get(&track.id).copied().unwrap_or(1);
    let naming = match &options.name_template {
        Some(template) => {
            let width = plan.total_tracks.to_string().len().max(2);
            let number = format!("{track_number:0width$}");
            let artist = non_empty(publisher.artist.as_deref()).unwrap_or_else(|| uploader.clone());
            let values = TemplateValues {
                number: &number,
                artist: &artist,
                title: &title,
                album: &plan.album,
                year: track.year(),
                id: track.id,
                genre: track.genre.as_deref(),
                uploader: &uploader,
            };
            FileNaming::Template {
                stem: render_template(template, &values),
            }
        }
        None => FileNaming::Numbered {
            number: track_number,
            total: plan.total_tracks,
            display: track_display_name(&uploader, &title),
        },
    };
    let best_artwork = track.best_artwork_url();
    let (artwork_url, artwork_fallback_url) = if options.original_artwork {
        (track.original_artwork_url(), best_artwork)
    } else {
        (best_artwork, None)
    };
    Ok(DownloadJob {
        track_id: track.id,
        title,
        artist: uploader,
        album: plan.album.clone(),
        track_number,
        total_tracks: plan.total_tracks,
        duration_ms: track.duration,
        permalink_url: track.permalink_url.clone(),
        artwork_url,
        artwork_fallback_url,
        transcoding_url: transcoding.url.clone(),
        protocol: transcoding.format.protocol.clone(),
        format,
        estimated_kbps: estimated_kbps(transcoding),
        track_authorization: track.track_authorization.clone(),
        output_dir: plan.target_dir.clone(),
        naming,
        tags: TrackTags {
            artist: non_empty(publisher.artist.as_deref()),
            genre: non_empty(track.genre.as_deref()),
            isrc: non_empty(publisher.isrc.as_deref()),
            label: non_empty(track.label_name.as_deref())
                .or_else(|| non_empty(publisher.publisher.as_deref())),
            composer: non_empty(publisher.writer_composer.as_deref()),
            copyright: non_empty(publisher.p_line.as_deref())
                .or_else(|| non_empty(publisher.c_line.as_deref())),
            album_artist: (!plan.single).then(|| sanitize(Some(&plan.owner), "Unknown Artist")),
            year: track.year(),
        },
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::*;
    use crate::shared::soundcloud::models::parse_resource;
    use crate::shared::soundcloud::transcoding::Quality;

    fn playlist() -> Resource {
        parse_resource(json!({
            "kind": "playlist", "title": "Mix", "user": {"username": "DJ"},
            "tracks": [{"id": 10, "title": "a"}, {"id": 20}, {"id": 30, "title": "c"}]
        }))
        .expect("playlist")
    }

    fn request_in(dir: &tempfile::TempDir) -> DownloadRequest {
        let mut request = DownloadRequest::new("x");
        request.output_dir = dir.path().to_path_buf();
        request
    }

    async fn single_plan(resource: Resource, request: &DownloadRequest) -> DownloadPlan {
        plan_download(resource, request, &Reporter::silent("t"))
            .await
            .expect("plano")
            .pop()
            .expect("um plano")
    }

    #[tokio::test]
    async fn numbers_by_full_playlist_position() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut request = request_in(&dir);
        request.only_track_ids = Some(HashSet::from([30]));
        let plan = single_plan(playlist(), &request).await;
        assert_eq!(plan.selected.len(), 1);
        assert_eq!(plan.numbering[&30], 3);
        assert_eq!(plan.order, [10, 20, 30]);
        assert_eq!(plan.total_tracks, 3);
        assert!(plan.target_dir.ends_with("DJ - Mix"));
        assert!(plan.target_dir.is_dir());
    }

    #[tokio::test]
    async fn limit_and_stub_detection() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut request = request_in(&dir);
        request.limit = Some(2);
        let plan = single_plan(playlist(), &request).await;
        assert_eq!(plan.selected.len(), 2);
        assert_eq!(plan.stub_ids(), vec![20]);
    }

    #[tokio::test]
    async fn single_track_goes_to_singles_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let track =
            parse_resource(json!({"kind": "track", "id": 7, "title": "Solo"})).expect("faixa");
        let plan = single_plan(track, &request_in(&dir)).await;
        assert!(plan.target_dir.ends_with(SINGLES_FOLDER));
        assert!(plan.single);
        assert_eq!(
            (plan.album.as_str(), plan.total_tracks, plan.numbering[&7]),
            ("Solo", 1, 1)
        );
    }

    #[tokio::test]
    async fn collections_get_one_folder_per_playlist() {
        use crate::shared::soundcloud::models::{Collection, CollectionKind, User};
        let dir = tempfile::tempdir().expect("tempdir");
        let album = |title: &str, id: i64| -> Playlist {
            serde_json::from_value(
                json!({"kind": "playlist", "title": title, "user": {"username": "Band"},
                                          "tracks": [{"id": id, "title": "x"}]}),
            )
            .expect("album")
        };
        let collection = Resource::Collection(Box::new(Collection {
            kind: CollectionKind::Albums,
            user: User::default(),
            permalink_url: "https://soundcloud.com/band/albums".into(),
            playlists: vec![album("Um", 1), album("Dois", 2)],
        }));
        let plans = plan_download(collection, &request_in(&dir), &Reporter::silent("t"))
            .await
            .expect("planos");
        let folders: Vec<_> = plans
            .iter()
            .map(|p| {
                p.target_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .collect();
        assert_eq!(
            folders,
            [Some("Band - Um".into()), Some("Band - Dois".into())]
        );
    }

    fn rich_track() -> Track {
        serde_json::from_value(json!({
            "id": 5, "title": "Song", "user": {"username": "uploader"}, "genre": "Techno",
            "created_at": "2020-01-01T00:00:00Z", "release_date": "2003-06-02T00:00:00Z", "label_name": "Label X",
            "artwork_url": "https://i1.sndcdn.com/artworks-1-large.jpg",
            "publisher_metadata": {"artist": "Real Artist", "isrc": "DEP960300042", "writer_composer": "Composer",
                                   "p_line": "2003 Label X"},
            "media": {"transcodings": [
                {"url": "https://api-v2.soundcloud.com/mp3", "preset": "mp3_1_0", "format": {"protocol": "progressive", "mime_type": "audio/mpeg"}},
                {"url": "https://api-v2.soundcloud.com/aac", "preset": "aac_160k", "format": {"protocol": "hls", "mime_type": "audio/mp4"}}
            ]}
        }))
        .expect("faixa")
    }

    #[tokio::test]
    async fn build_job_applies_options_and_rich_tags() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = single_plan(playlist(), &request_in(&dir)).await;
        let track = rich_track();

        let job = build_job(&plan, &track, &DownloadOptions::default()).expect("job");
        assert_eq!(job.format.extension, ".mp3");
        assert_eq!(job.naming.file_name(".mp3"), "01. uploader - Song.mp3");
        assert_eq!(job.tags.artist.as_deref(), Some("Real Artist"));
        assert_eq!(job.tags.isrc.as_deref(), Some("DEP960300042"));
        assert_eq!(job.tags.year, Some(2003), "release_date vence created_at");
        assert_eq!(job.tags.album_artist.as_deref(), Some("DJ"));
        assert!(
            job.artwork_url
                .as_deref()
                .is_some_and(|u| u.contains("t500x500"))
        );

        let options = DownloadOptions {
            quality: Quality::Best,
            name_template: Some("{artist} - {title} ({year})".into()),
            original_artwork: true,
            ..DownloadOptions::default()
        };
        let job = build_job(&plan, &track, &options).expect("job");
        assert_eq!(job.format.extension, ".m4a");
        assert_eq!(job.estimated_kbps, 160);
        assert_eq!(
            job.naming.file_name(".m4a"),
            "Real Artist - Song (2003).m4a"
        );
        assert!(
            job.artwork_url
                .as_deref()
                .is_some_and(|u| u.contains("-original."))
        );
        assert!(
            job.artwork_fallback_url
                .as_deref()
                .is_some_and(|u| u.contains("t500x500"))
        );
    }

    #[tokio::test]
    async fn build_job_rejects_drm_only_tracks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plan = single_plan(playlist(), &request_in(&dir)).await;
        let track: Track = serde_json::from_value(json!({"id": 1, "title": "t", "media": {"transcodings": [
            {"url": "https://api-v2.soundcloud.com/x", "format": {"protocol": "ctr-encrypted-hls", "mime_type": "audio/mp4"}}]}}))
        .expect("track");
        assert!(matches!(
            build_job(&plan, &track, &DownloadOptions::default()),
            Err(Error::StreamUnavailable(_))
        ));
    }
}
