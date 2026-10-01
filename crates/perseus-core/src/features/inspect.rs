use crate::shared::error::Result;
use crate::shared::soundcloud::client::SoundCloudClient;
use crate::shared::soundcloud::models::{Resource, Track};
use crate::shared::soundcloud::transcoding::choose_transcoding;

#[derive(Debug, Clone)]
pub struct TrackPreview {
    pub track: Track,
    pub unavailable_reason: Option<String>,
    pub group: Option<String>,
}

impl TrackPreview {
    fn of(track: Track, group: Option<String>) -> Self {
        let unavailable_reason = choose_transcoding(&track).err().map(|err| err.to_string());
        Self {
            track,
            unavailable_reason,
            group,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InspectResult {
    pub resource: Resource,
    pub preview: Vec<TrackPreview>,
    pub total_tracks: usize,
}

pub async fn inspect_url(
    client: &SoundCloudClient,
    url: &str,
    preview_limit: usize,
) -> Result<InspectResult> {
    let resource = client.resolve(url).await?;
    let (preview, total_tracks) = match &resource {
        Resource::Track(track) => (vec![TrackPreview::of((**track).clone(), None)], 1),
        Resource::Playlist(playlist) => {
            let head = &playlist.tracks[..playlist.tracks.len().min(preview_limit)];
            let tracks = client.hydrate(head).await?;
            (
                tracks
                    .into_iter()
                    .map(|t| TrackPreview::of(t, None))
                    .collect(),
                playlist.total_tracks(),
            )
        }
        Resource::Collection(collection) => {
            let mut preview = Vec::new();
            let mut total = 0;
            for playlist in &collection.playlists {
                total += playlist.total_tracks();
                let room = preview_limit.saturating_sub(preview.len());
                if room == 0 {
                    continue;
                }
                let head = &playlist.tracks[..playlist.tracks.len().min(room)];
                let group = Some(playlist.display_title());
                preview.extend(
                    client
                        .hydrate(head)
                        .await?
                        .into_iter()
                        .map(|t| TrackPreview::of(t, group.clone())),
                );
            }
            (preview, total)
        }
    };
    Ok(InspectResult {
        resource,
        preview,
        total_tracks,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::test_support::{client, track};

    async fn resolving(body: serde_json::Value) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn single_track_reports_why_it_is_unavailable() {
        let server = resolving(json!({"kind": "track", "id": 9, "title": "Protegida", "media": {"transcodings": [
            {"url": "https://x/y", "format": {"protocol": "ctr-encrypted-hls", "mime_type": "audio/mp4"}}]}}))
        .await;
        let result = inspect_url(&client(&server), "https://soundcloud.com/p/protegida", 10)
            .await
            .expect("inspecao");
        assert_eq!(result.total_tracks, 1);
        let reason = result.preview[0]
            .unavailable_reason
            .as_deref()
            .expect("motivo");
        assert!(reason.contains("DRM"), "{reason}");
    }

    #[tokio::test]
    async fn playlist_preview_hydrates_stubs_up_to_the_limit() {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "playlist", "title": "Argonautas", "user": {"username": "Perseus"}, "track_count": 3,
                "tracks": [track(1, "Primeira", "progressive", &base), {"id": 2}, {"id": 3}]
            })))
            .mount(&server)
            .await;
        Mock::given(path("/tracks"))
            .and(query_param("ids", "2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([track(2, "Segunda", "hls", &base)])),
            )
            .expect(1)
            .mount(&server)
            .await;

        let result = inspect_url(&client(&server), "https://soundcloud.com/p/sets/a", 2)
            .await
            .expect("inspecao");
        assert_eq!(result.total_tracks, 3);
        let titles: Vec<_> = result
            .preview
            .iter()
            .map(|p| p.track.title.as_deref().unwrap_or_default())
            .collect();
        assert_eq!(titles, ["Primeira", "Segunda"]);
        assert!(
            result
                .preview
                .iter()
                .all(|p| p.unavailable_reason.is_none() && p.group.is_none())
        );
    }

    #[tokio::test]
    async fn collection_groups_preview_by_album_and_counts_every_track() {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "user", "id": 7, "username": "Perseus", "permalink_url": "https://soundcloud.com/perseus"
            })))
            .mount(&server)
            .await;
        let album = |id: i64, title: &str, tracks: Vec<serde_json::Value>| {
            json!({"kind": "playlist", "id": id, "title": title, "is_album": true, "track_count": tracks.len(),
                   "user": {"username": "Perseus"}, "tracks": tracks})
        };
        Mock::given(path("/users/7/albums"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "collection": [
                    album(10, "Ecclesia", vec![track(1, "Um", "progressive", &base), track(2, "Dois", "progressive", &base)]),
                    album(11, "Soulhack", vec![track(3, "Tres", "progressive", &base)]),
                ],
                "next_href": null
            })))
            .mount(&server)
            .await;

        let result = inspect_url(&client(&server), "https://soundcloud.com/perseus/albums", 2)
            .await
            .expect("inspecao");
        assert!(matches!(result.resource, Resource::Collection(_)));
        assert_eq!(
            result.total_tracks, 3,
            "o limite corta a previa, nao a contagem"
        );
        let groups: Vec<_> = result.preview.iter().map(|p| p.group.as_deref()).collect();
        assert_eq!(groups, [Some("Ecclesia"), Some("Ecclesia")]);
    }
}
