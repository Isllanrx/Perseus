use std::collections::HashMap;
use std::path::PathBuf;

use futures::{StreamExt as _, stream};

use crate::features::download::library::Archive;
use crate::features::download::models::{DownloadJob, DownloadOptions};
use crate::features::download::planning::{
    DownloadPlan, NOT_RETURNED_REASON, Selection, build_job, claim_unique_name, folders,
    playlist_file_name,
};
use crate::shared::config::{MAX_LIMIT, TRACK_BATCH_CONCURRENCY, TRACK_BATCH_SIZE};
use crate::shared::error::{Error, Result};
use crate::shared::soundcloud::client::SoundCloudClient;
use crate::shared::soundcloud::models::{Resource, Track};
use crate::shared::soundcloud::transcoding::REASON_FILTERED;

#[derive(Debug)]
pub struct RemoteFolder {
    pub folder: String,
    pub album: String,
    pub owner: String,
    pub single: bool,
    pub total_tracks: usize,
    pub playlist_file: Option<String>,
    pub items: Vec<RemoteItem>,
}

#[derive(Debug)]
pub enum RemoteItem {
    Ready(Box<DownloadJob>),
    Unavailable { track_id: i64, reason: String },
    Failed { track_id: i64, reason: String },
}

impl RemoteItem {
    pub fn track_id(&self) -> i64 {
        match self {
            Self::Ready(job) => job.track_id,
            Self::Unavailable { track_id, .. } | Self::Failed { track_id, .. } => *track_id,
        }
    }
}

enum Hydrated {
    Track(Box<Track>),
    Missing,
    BatchFailed(String),
}

pub async fn plan_remote(
    client: &SoundCloudClient,
    resource: Resource,
    limit: Option<usize>,
    options: &DownloadOptions,
) -> Result<Vec<RemoteFolder>> {
    if limit.is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit)) {
        return Err(Error::InvalidInput(format!(
            "limit deve estar entre 1 e {MAX_LIMIT}"
        )));
    }
    options.validate()?;

    let plans: Vec<DownloadPlan> = folders(resource)
        .into_iter()
        .map(|folder| {
            DownloadPlan::new(
                PathBuf::from(&folder.name),
                folder.album,
                folder.owner,
                Selection::of(folder.tracks, limit, None),
                folder.single,
                Archive::empty(),
            )
        })
        .collect();
    let hydrated = hydrate(client, &plans).await;

    Ok(plans
        .iter()
        .map(|plan| {
            let mut claimed = HashMap::new();
            let items = plan
                .selected
                .iter()
                .map(|track| {
                    let track = if track.is_stub() {
                        match hydrated.get(&track.id) {
                            Some(Hydrated::Track(full)) => full,
                            Some(Hydrated::BatchFailed(reason)) => {
                                return RemoteItem::Failed {
                                    track_id: track.id,
                                    reason: reason.clone(),
                                };
                            }
                            Some(Hydrated::Missing) | None => {
                                return RemoteItem::Unavailable {
                                    track_id: track.id,
                                    reason: NOT_RETURNED_REASON.to_owned(),
                                };
                            }
                        }
                    } else {
                        track
                    };
                    item(plan, track, options, &mut claimed)
                })
                .collect();
            RemoteFolder {
                folder: plan.target_dir.display().to_string(),
                album: plan.album.clone(),
                owner: plan.owner.clone(),
                single: plan.single,
                total_tracks: plan.total_tracks,
                playlist_file: (options.write_playlist_file && !plan.single)
                    .then(|| playlist_file_name(&plan.album)),
                items,
            }
        })
        .collect())
}

fn item(
    plan: &DownloadPlan,
    track: &Track,
    options: &DownloadOptions,
    claimed: &mut HashMap<String, i64>,
) -> RemoteItem {
    if !options.accepts_duration(track.duration) {
        return RemoteItem::Unavailable {
            track_id: track.id,
            reason: REASON_FILTERED.to_owned(),
        };
    }
    match build_job(plan, track, options) {
        Ok(mut job) => {
            claim_unique_name(claimed, &mut job);
            RemoteItem::Ready(Box::new(job))
        }
        Err(err) => RemoteItem::Unavailable {
            track_id: track.id,
            reason: err.to_string(),
        },
    }
}

async fn hydrate(client: &SoundCloudClient, plans: &[DownloadPlan]) -> HashMap<i64, Hydrated> {
    let stubs: Vec<i64> = plans.iter().flat_map(DownloadPlan::stub_ids).collect();
    let batches: Vec<_> = stubs
        .chunks(TRACK_BATCH_SIZE)
        .map(|chunk| async move { (chunk, client.fetch_batch(chunk).await) })
        .collect();
    let mut results = stream::iter(batches).buffer_unordered(TRACK_BATCH_CONCURRENCY);
    let mut hydrated = HashMap::with_capacity(stubs.len());
    while let Some((requested, result)) = results.next().await {
        match result {
            Ok(tracks) => {
                let mut found: HashMap<i64, Track> =
                    tracks.into_iter().map(|t| (t.id, t)).collect();
                for track_id in requested {
                    let entry = match found.remove(track_id) {
                        Some(track) if !track.is_stub() => Hydrated::Track(Box::new(track)),
                        _ => Hydrated::Missing,
                    };
                    hydrated.insert(*track_id, entry);
                }
            }
            Err(err) => {
                let reason = format!("falha ao consultar lote: {err}");
                for track_id in requested {
                    hydrated.insert(*track_id, Hydrated::BatchFailed(reason.clone()));
                }
            }
        }
    }
    hydrated
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::shared::soundcloud::models::parse_resource;
    use crate::test_support::{client, track};

    fn ready(item: &RemoteItem) -> &DownloadJob {
        match item {
            RemoteItem::Ready(job) => job,
            other => panic!("esperava faixa pronta, veio {other:?}"),
        }
    }

    fn solo(base: &str) -> serde_json::Value {
        let mut value = track(7, "Solo", "progressive", base);
        value["kind"] = json!("track");
        value
    }

    fn playlist(tracks: &serde_json::Value) -> Resource {
        parse_resource(
            json!({"kind": "playlist", "title": "Argonautas", "user": {"username": "Perseus"},
                              "tracks": tracks}),
        )
        .expect("playlist")
    }

    #[tokio::test]
    async fn mirrors_local_planning_without_touching_disk() {
        let server = MockServer::start().await;
        let uri = server.uri();
        let resource = playlist(&json!([
            track(1, "Um", "progressive", &uri),
            {"id": 2},
            track(3, "Tres", "hls", &uri),
            {"id": 4}
        ]));
        Mock::given(path("/tracks"))
            .and(query_param("ids", "2,4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([track(
                2,
                "Dois",
                "progressive",
                &uri
            )])))
            .mount(&server)
            .await;

        let folders = plan_remote(
            &client(&server),
            resource,
            None,
            &DownloadOptions::default(),
        )
        .await
        .expect("plano");

        let [folder] = folders.as_slice() else {
            panic!("uma pasta");
        };
        assert_eq!(folder.folder, "Perseus - Argonautas");
        assert_eq!(folder.playlist_file.as_deref(), Some("Argonautas.m3u8"));
        assert_eq!(folder.total_tracks, 4);
        let ids: Vec<i64> = folder.items.iter().map(RemoteItem::track_id).collect();
        assert_eq!(ids, [1, 2, 3, 4], "ordem da playlist");
        assert_eq!(
            ready(&folder.items[1]).naming.file_name(".mp3"),
            "02. Perseus - Dois.mp3"
        );
        assert_eq!(ready(&folder.items[2]).protocol, "hls");
        assert!(matches!(
            &folder.items[3],
            RemoteItem::Unavailable { reason, .. } if reason == NOT_RETURNED_REASON
        ));
    }

    #[tokio::test]
    async fn applies_limit_duration_filter_and_unique_names() {
        let server = MockServer::start().await;
        let uri = server.uri();
        let mut short = track(3, "Curta", "progressive", &uri);
        short["duration"] = json!(10_000);
        let resource = playlist(&json!([
            track(1, "Mesma", "progressive", &uri),
            track(2, "Mesma", "progressive", &uri),
            short,
            track(4, "Fora do limite", "progressive", &uri)
        ]));
        let options = DownloadOptions {
            name_template: Some("{title}".into()),
            min_duration_s: Some(60),
            write_playlist_file: false,
            ..DownloadOptions::default()
        };

        let folders = plan_remote(&client(&server), resource, Some(3), &options)
            .await
            .expect("plano");

        let folder = &folders[0];
        assert_eq!(folder.items.len(), 3);
        assert_eq!(folder.playlist_file, None);
        assert_eq!(
            ready(&folder.items[0]).naming.file_name(".mp3"),
            "Mesma.mp3"
        );
        assert_eq!(
            ready(&folder.items[1]).naming.file_name(".mp3"),
            "Mesma [2].mp3"
        );
        assert!(matches!(
            &folder.items[2],
            RemoteItem::Unavailable { reason, .. } if reason == REASON_FILTERED
        ));
    }

    #[tokio::test]
    async fn failed_batch_marks_only_its_tracks() {
        let server = MockServer::start().await;
        let uri = server.uri();
        Mock::given(path("/tracks"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;
        let resource = playlist(&json!([track(1, "Um", "progressive", &uri), {"id": 2}]));

        let folders = plan_remote(
            &client(&server),
            resource,
            None,
            &DownloadOptions::default(),
        )
        .await
        .expect("plano");

        assert!(matches!(folders[0].items[0], RemoteItem::Ready(_)));
        assert!(matches!(
            &folders[0].items[1],
            RemoteItem::Failed { reason, .. } if reason.contains("falha ao consultar lote")
        ));
    }

    #[tokio::test]
    async fn single_tracks_have_no_playlist_file() {
        let server = MockServer::start().await;
        let resource = parse_resource(solo(&server.uri())).expect("faixa");
        let folders = plan_remote(
            &client(&server),
            resource,
            None,
            &DownloadOptions::default(),
        )
        .await
        .expect("plano");
        assert_eq!(folders[0].folder, "Single Tracks");
        assert!(folders[0].single);
        assert_eq!(folders[0].playlist_file, None);
    }

    #[tokio::test]
    async fn rejects_invalid_limit_and_options() {
        let server = MockServer::start().await;
        let resource = || parse_resource(solo(&server.uri())).expect("faixa");
        let client = client(&server);
        assert!(matches!(
            plan_remote(&client, resource(), Some(0), &DownloadOptions::default()).await,
            Err(Error::InvalidInput(_))
        ));
        let options = DownloadOptions {
            name_template: Some("{number}".into()),
            ..DownloadOptions::default()
        };
        assert!(matches!(
            plan_remote(&client, resource(), None, &options).await,
            Err(Error::InvalidInput(_))
        ));
    }
}
