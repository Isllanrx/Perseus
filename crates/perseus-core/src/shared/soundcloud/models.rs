use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::shared::error::{Error, Result};

const ARTWORK_SUFFIXES: [&str; 7] = [
    "-large.",
    "-t300x300.",
    "-badge.",
    "-small.",
    "-tiny.",
    "-original.",
    "-crop.",
];

pub fn artwork_500(url: Option<&str>) -> Option<String> {
    artwork_variant(url, "-t500x500.")
}

fn artwork_variant(url: Option<&str>, variant: &str) -> Option<String> {
    let url = url.filter(|u| !u.is_empty())?;
    for suffix in ARTWORK_SUFFIXES {
        if url.contains(suffix) {
            return Some(url.replacen(suffix, variant, 1));
        }
    }
    Some(url.to_owned())
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct User {
    pub id: Option<i64>,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscodingFormat {
    pub protocol: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub mime_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Transcoding {
    pub url: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub preset: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub quality: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub snipped: bool,
    pub format: TranscodingFormat,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PublisherMetadata {
    pub artist: Option<String>,
    pub album_title: Option<String>,
    pub isrc: Option<String>,
    pub publisher: Option<String>,
    pub writer_composer: Option<String>,
    pub p_line: Option<String>,
    pub c_line: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Media {
    #[serde(default, deserialize_with = "null_as_default")]
    pub transcodings: Vec<Transcoding>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Track {
    pub id: i64,
    pub title: Option<String>,
    pub user: Option<User>,
    pub duration: Option<i64>,
    pub artwork_url: Option<String>,
    pub created_at: Option<String>,
    pub permalink_url: Option<String>,
    pub media: Option<Media>,
    pub policy: Option<String>,
    pub track_authorization: Option<String>,
    pub genre: Option<String>,
    pub label_name: Option<String>,
    pub release_date: Option<String>,
    pub publisher_metadata: Option<PublisherMetadata>,
}

impl Track {
    pub fn is_stub(&self) -> bool {
        self.title.is_none()
    }

    pub fn display_title(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| format!("Track {}", self.id))
    }

    pub fn artist(&self) -> String {
        artist_of(self.user.as_ref())
    }

    pub fn transcodings(&self) -> &[Transcoding] {
        self.media
            .as_ref()
            .map_or(&[], |media| media.transcodings.as_slice())
    }

    pub fn best_artwork_url(&self) -> Option<String> {
        artwork_500(self.raw_artwork_url())
    }

    pub fn original_artwork_url(&self) -> Option<String> {
        artwork_variant(self.raw_artwork_url(), "-original.")
    }

    fn raw_artwork_url(&self) -> Option<&str> {
        let avatar = self.user.as_ref().and_then(|u| u.avatar_url.as_deref());
        self.artwork_url
            .as_deref()
            .filter(|u| !u.is_empty())
            .or(avatar)
    }

    pub fn year(&self) -> Option<u16> {
        [self.release_date.as_deref(), self.created_at.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|date| date.get(..4).and_then(|year| year.parse().ok()))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaylistSource {
    #[default]
    Playlist,
    Likes,
    Uploads,
    PopularTracks,
    Reposts,
    Related,
}

impl PlaylistSource {
    pub fn code(self) -> &'static str {
        match self {
            Self::Playlist => "playlist",
            Self::Likes => "likes",
            Self::Uploads => "uploads",
            Self::PopularTracks => "popular",
            Self::Reposts => "reposts",
            Self::Related => "related",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionKind {
    Albums,
    Playlists,
}

impl CollectionKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Albums => "albums",
            Self::Playlists => "playlists",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Collection {
    pub kind: CollectionKind,
    pub user: User,
    pub permalink_url: String,
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Playlist {
    #[serde(skip)]
    pub source: PlaylistSource,
    pub title: Option<String>,
    pub user: Option<User>,
    pub track_count: Option<i64>,
    pub permalink_url: Option<String>,
    pub artwork_url: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub tracks: Vec<Track>,
}

impl Playlist {
    pub fn display_title(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| "SoundCloud Playlist".to_owned())
    }

    pub fn artist(&self) -> String {
        artist_of(self.user.as_ref())
    }

    pub fn total_tracks(&self) -> usize {
        self.track_count
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n > 0)
            .unwrap_or(self.tracks.len())
    }

    pub fn track_ids(&self) -> Vec<i64> {
        self.tracks.iter().map(|track| track.id).collect()
    }
}

#[derive(Debug, Clone)]
pub enum Resource {
    Track(Box<Track>),
    Playlist(Box<Playlist>),
    Collection(Box<Collection>),
}

impl Resource {
    pub fn display_title(&self) -> String {
        match self {
            Self::Track(track) => track.display_title(),
            Self::Playlist(playlist) => playlist.display_title(),
            Self::Collection(collection) => match collection.kind {
                CollectionKind::Albums => "Albums".to_owned(),
                CollectionKind::Playlists => "Playlists".to_owned(),
            },
        }
    }

    pub fn artist(&self) -> String {
        match self {
            Self::Track(track) => track.artist(),
            Self::Playlist(playlist) => playlist.artist(),
            Self::Collection(collection) => artist_of(Some(&collection.user)),
        }
    }

    pub fn permalink_url(&self) -> Option<&str> {
        match self {
            Self::Track(track) => track.permalink_url.as_deref(),
            Self::Playlist(playlist) => playlist.permalink_url.as_deref(),
            Self::Collection(collection) => Some(&collection.permalink_url),
        }
    }
}

fn artist_of(user: Option<&User>) -> String {
    user.and_then(|u| u.username.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown Artist".to_owned())
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub kind: &'static str,
    pub title: String,
    pub subtitle: String,
    pub url: String,
    pub artwork_url: Option<String>,
    pub duration_ms: Option<i64>,
    pub track_count: Option<i64>,
}

impl SearchHit {
    pub fn from_value(value: &Value) -> Option<Self> {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
        };
        let url = crate::shared::soundcloud::urls::normalize_url(text("permalink_url")?).ok()?;
        let user = value.get("user");
        let username = user
            .and_then(|u| u.get("username"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let (kind, title, subtitle, artwork) = match text("kind")? {
            "track" => ("track", text("title")?, username, text("artwork_url")),
            "playlist" => {
                let kind = if value.get("is_album").and_then(Value::as_bool) == Some(true) {
                    "album"
                } else {
                    "playlist"
                };
                (kind, text("title")?, username, text("artwork_url"))
            }
            "user" => (
                "user",
                text("username")?,
                text("full_name").unwrap_or_default().to_owned(),
                text("avatar_url"),
            ),
            _ => return None,
        };
        let avatar = user
            .and_then(|u| u.get("avatar_url"))
            .and_then(Value::as_str);
        Some(Self {
            kind,
            title: title.to_owned(),
            subtitle,
            url,
            artwork_url: artwork_500(artwork.or(avatar)),
            duration_ms: value.get("duration").and_then(Value::as_i64),
            track_count: value.get("track_count").and_then(Value::as_i64),
        })
    }
}

pub fn parse_resource(data: Value) -> Result<Resource> {
    let kind = data
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("desconhecido")
        .to_owned();
    let invalid =
        |err: serde_json::Error| Error::api(format!("Resposta inesperada da API ({err})."), None);
    match kind.as_str() {
        "track" => Ok(Resource::Track(Box::new(
            serde_json::from_value(data).map_err(invalid)?,
        ))),
        "playlist" | "system-playlist" => Ok(Resource::Playlist(Box::new(
            serde_json::from_value(data).map_err(invalid)?,
        ))),
        other => Err(Error::UnsupportedResource(format!(
            "A URL aponta para um recurso nao suportado ({other}). Use o link de uma faixa ou playlist/album."
        ))),
    }
}

pub fn parse_tracks(data: Value) -> Result<Vec<Track>> {
    let Value::Array(items) = data else {
        return Err(Error::api("Lote de faixas com formato inesperado.", None));
    };
    Ok(items
        .into_iter()
        .filter_map(|item| {
            serde_json::from_value::<Track>(item)
                .inspect_err(|err| tracing::debug!(error = %err, "faixa ignorada no lote"))
                .ok()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn artwork_upgrade() {
        assert_eq!(
            artwork_500(Some("https://i1.sndcdn.com/a-large.jpg")).as_deref(),
            Some("https://i1.sndcdn.com/a-t500x500.jpg")
        );
        assert_eq!(
            artwork_500(Some("https://i1.sndcdn.com/a.jpg")).as_deref(),
            Some("https://i1.sndcdn.com/a.jpg")
        );
        assert_eq!(artwork_500(None), None);
    }

    #[test]
    fn parses_playlist_with_stubs_and_nulls() {
        let data = json!({
            "kind": "playlist", "id": 9, "title": "Mix", "user": {"username": "dj"}, "track_count": null,
            "artwork_url": null,
            "tracks": [
                {"id": 1, "title": "One", "media": {"transcodings": null}, "user": null},
                {"id": 2, "kind": "track"}
            ]
        });
        let Resource::Playlist(playlist) = parse_resource(data).expect("playlist") else {
            panic!("tipo errado")
        };
        assert_eq!(playlist.total_tracks(), 2);
        assert!(!playlist.tracks[0].is_stub());
        assert!(playlist.tracks[1].is_stub());
        assert_eq!(playlist.tracks[0].artist(), "Unknown Artist");
        assert_eq!(playlist.artist(), "dj");
    }

    #[test]
    fn rejects_unsupported_kinds() {
        assert!(matches!(
            parse_resource(json!({"kind": "user", "id": 1})),
            Err(Error::UnsupportedResource(_))
        ));
        assert!(matches!(
            parse_resource(json!([])),
            Err(Error::UnsupportedResource(_))
        ));
    }

    #[test]
    fn batch_skips_malformed_items() {
        let tracks =
            parse_tracks(json!([{"id": 1, "title": "a"}, {"title": "sem id"}])).expect("lote");
        assert_eq!(tracks.len(), 1);
    }
}
