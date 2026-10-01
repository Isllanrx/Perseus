use std::collections::{HashMap, HashSet};

use futures::{StreamExt as _, TryStreamExt as _, stream};
use serde_json::Value;
use tokio::sync::Semaphore;
use url::Url;

use crate::shared::config::{
    MAX_API_BYTES, MAX_COLLECTION_PLAYLISTS, MAX_CONCURRENT_API_REQUESTS, MAX_LIMIT, MAX_PAGES,
    MAX_RELATED_TRACKS, MAX_SEARCH_QUERY, MAX_SEARCH_RESULTS, MAX_SHORTLINK_HOPS, PAGE_SIZE,
    SHORTLINK_HOSTS, TRACK_BATCH_CONCURRENCY, TRACK_BATCH_SIZE, WEB_HOSTS,
};
use crate::shared::error::{Error, Result};
use crate::shared::http::{Http, read_limited};
use crate::shared::soundcloud::Endpoints;
use crate::shared::soundcloud::auth::ClientIdProvider;
use crate::shared::soundcloud::models::{
    Collection, CollectionKind, Playlist, PlaylistSource, Resource, SearchHit, Track, User,
    parse_resource, parse_tracks,
};
use crate::shared::soundcloud::urls::{
    ProfileSection, is_shortlink, normalize_url, profile_section, related_track,
};

pub const NOT_FOUND_MESSAGE: &str =
    "Recurso nao encontrado: removido, privado sem link secreto ou indisponivel na sua regiao.";

fn track_of(item: &Value) -> Option<Track> {
    let value = if item.get("kind").and_then(Value::as_str) == Some("track") {
        item
    } else {
        item.get("track")?
    };
    serde_json::from_value(value.clone()).ok()
}

fn playlist_of(item: &Value) -> Option<Playlist> {
    let value = if item.get("kind").and_then(Value::as_str) == Some("playlist") {
        item
    } else {
        item.get("playlist")?
    };
    serde_json::from_value(value.clone()).ok()
}

fn dedupe_tracks(tracks: Vec<Track>) -> Vec<Track> {
    let mut seen = HashSet::new();
    tracks.into_iter().filter(|t| seen.insert(t.id)).collect()
}

fn parse_user(data: Value) -> Result<User> {
    serde_json::from_value(data)
        .map_err(|err| Error::api(format!("Perfil com formato inesperado ({err})."), None))
}

fn virtual_playlist(
    source: PlaylistSource,
    title: String,
    user: User,
    url: &str,
    tracks: Vec<Track>,
) -> Resource {
    Resource::Playlist(Box::new(Playlist {
        source,
        title: Some(title),
        user: Some(user),
        track_count: i64::try_from(tracks.len()).ok(),
        permalink_url: Some(url.to_owned()),
        artwork_url: None,
        tracks,
    }))
}

fn endpoint(url: &str) -> String {
    Url::parse(url).map_or_else(|_| url.to_owned(), |u| u.path().to_owned())
}

pub struct SoundCloudClient {
    http: Http,
    endpoints: Endpoints,
    auth: ClientIdProvider,
    api_permits: Semaphore,
}

impl SoundCloudClient {
    pub fn new(explicit_client_id: Option<String>) -> Result<Self> {
        let http = Http::new()?;
        let endpoints = Endpoints::default();
        let auth = ClientIdProvider::new(http.clone(), endpoints.clone(), explicit_client_id)?;
        Ok(Self {
            http,
            endpoints,
            auth,
            api_permits: Semaphore::new(MAX_CONCURRENT_API_REQUESTS),
        })
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn for_tests(base: &str, client_id: Option<String>) -> Self {
        let http = Http::for_tests();
        let endpoints = Endpoints {
            api_base: base.to_owned(),
            web_base: base.to_owned(),
        };
        let auth = ClientIdProvider::new(http.clone(), endpoints.clone(), client_id)
            .expect("client_id valido")
            .with_cache_path(None);
        Self {
            http,
            endpoints,
            auth,
            api_permits: Semaphore::new(MAX_CONCURRENT_API_REQUESTS),
        }
    }

    pub fn http(&self) -> &Http {
        &self.http
    }

    pub async fn client_id(&self) -> Result<String> {
        self.auth.client_id().await
    }

    pub async fn canonicalize(&self, raw_url: &str) -> Result<String> {
        let url = normalize_url(raw_url)?;
        if is_shortlink(&url) {
            self.expand_shortlink(&url).await
        } else {
            Ok(url)
        }
    }

    pub async fn resolve(&self, url: &str) -> Result<Resource> {
        if let Some((profile, section)) = profile_section(url) {
            let user = self.resolve_user(&profile).await?;
            return self.profile_resource(url, user, section).await;
        }
        if let Some(track_url) = related_track(url) {
            return self.related_resource(url, &track_url).await;
        }
        let data = self.resolve_raw(url).await?;
        if data.get("kind").and_then(Value::as_str) == Some("user") {
            let user = parse_user(data)?;
            return self
                .profile_resource(url, user, ProfileSection::Uploads)
                .await;
        }
        parse_resource(data)
    }

    async fn resolve_raw(&self, url: &str) -> Result<Value> {
        self.get_json(
            &format!("{}/resolve", self.endpoints.api_base),
            &[("url", url)],
        )
        .await
    }

    async fn resolve_user(&self, profile_url: &str) -> Result<User> {
        let data = self.resolve_raw(profile_url).await?;
        if data.get("kind").and_then(Value::as_str) != Some("user") {
            return Err(Error::UnsupportedResource(
                "O link nao pertence a um perfil do SoundCloud.".into(),
            ));
        }
        parse_user(data)
    }

    async fn profile_resource(
        &self,
        url: &str,
        user: User,
        section: ProfileSection,
    ) -> Result<Resource> {
        let user_id = user
            .id
            .ok_or_else(|| Error::api("Perfil sem id na resposta da API.", None))?;
        let api = &self.endpoints.api_base;
        let (source, title, path) = match section {
            ProfileSection::Likes => (
                PlaylistSource::Likes,
                "Likes",
                format!("{api}/users/{user_id}/track_likes"),
            ),
            ProfileSection::Uploads => (
                PlaylistSource::Uploads,
                "Tracks",
                format!("{api}/users/{user_id}/tracks"),
            ),
            ProfileSection::PopularTracks => (
                PlaylistSource::PopularTracks,
                "Popular tracks",
                format!("{api}/users/{user_id}/toptracks"),
            ),
            ProfileSection::Reposts => (
                PlaylistSource::Reposts,
                "Reposts",
                format!("{api}/stream/users/{user_id}/reposts"),
            ),
            ProfileSection::Albums | ProfileSection::Playlists => {
                let (kind, path) = if section == ProfileSection::Albums {
                    (
                        CollectionKind::Albums,
                        format!("{api}/users/{user_id}/albums"),
                    )
                } else {
                    (
                        CollectionKind::Playlists,
                        format!("{api}/users/{user_id}/playlists_without_albums"),
                    )
                };
                let playlists = self
                    .paginate(&path, MAX_COLLECTION_PLAYLISTS, playlist_of)
                    .await?;
                return Ok(Resource::Collection(Box::new(Collection {
                    kind,
                    user,
                    permalink_url: url.to_owned(),
                    playlists,
                })));
            }
        };
        let tracks = dedupe_tracks(self.paginate(&path, MAX_LIMIT, track_of).await?);
        Ok(virtual_playlist(
            source,
            title.to_owned(),
            user,
            url,
            tracks,
        ))
    }

    async fn related_resource(&self, url: &str, track_url: &str) -> Result<Resource> {
        let Resource::Track(track) = parse_resource(self.resolve_raw(track_url).await?)? else {
            return Err(Error::UnsupportedResource(
                "Relacionadas exigem o link de uma faixa.".into(),
            ));
        };
        let path = format!("{}/tracks/{}/related", self.endpoints.api_base, track.id);
        let tracks = dedupe_tracks(self.paginate(&path, MAX_RELATED_TRACKS, track_of).await?);
        let title = format!("Related - {}", track.display_title());
        Ok(virtual_playlist(
            PlaylistSource::Related,
            title,
            track.user.clone().unwrap_or_default(),
            url,
            tracks,
        ))
    }

    pub async fn fetch_likes(&self, user_id: i64) -> Result<Vec<Track>> {
        let path = format!("{}/users/{user_id}/track_likes", self.endpoints.api_base);
        Ok(dedupe_tracks(
            self.paginate(&path, MAX_LIMIT, track_of).await?,
        ))
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > MAX_SEARCH_QUERY {
            return Err(Error::InvalidInput(format!(
                "A busca deve ter entre 1 e {MAX_SEARCH_QUERY} caracteres."
            )));
        }
        let limit = limit.clamp(1, MAX_SEARCH_RESULTS).to_string();
        let data = self
            .get_json(
                &format!("{}/search", self.endpoints.api_base),
                &[("q", query), ("limit", &limit)],
            )
            .await?;
        Ok(data
            .get("collection")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(SearchHit::from_value).collect())
            .unwrap_or_default())
    }

    async fn paginate<T>(
        &self,
        first_url: &str,
        max_items: usize,
        extract: impl Fn(&Value) -> Option<T>,
    ) -> Result<Vec<T>> {
        let api_prefix = format!("{}/", self.endpoints.api_base);
        let page_size = PAGE_SIZE.to_string();
        let mut url = first_url.to_owned();
        let mut params: Vec<(&str, &str)> =
            vec![("limit", page_size.as_str()), ("linked_partitioning", "1")];
        let mut items = Vec::new();
        for _ in 0..MAX_PAGES {
            let page = self.get_json(&url, &params).await?;
            let collection = page
                .get("collection")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            for value in collection {
                if let Some(item) = extract(value) {
                    items.push(item);
                    if items.len() >= max_items {
                        return Ok(items);
                    }
                }
            }
            match page
                .get("next_href")
                .and_then(Value::as_str)
                .filter(|next| !next.is_empty())
            {
                Some(next) if next.starts_with(&api_prefix) => {
                    next.clone_into(&mut url);
                    params.clear();
                }
                Some(_) => {
                    return Err(Error::api(
                        "Paginacao da API com endereco inesperado.",
                        None,
                    ));
                }
                None => return Ok(items),
            }
        }
        tracing::warn!(pages = MAX_PAGES, "limite de paginas atingido");
        Ok(items)
    }

    pub async fn fetch_batch(&self, track_ids: &[i64]) -> Result<Vec<Track>> {
        if track_ids.len() > TRACK_BATCH_SIZE {
            return Err(Error::InvalidInput(format!(
                "lote com {} ids excede {TRACK_BATCH_SIZE}",
                track_ids.len()
            )));
        }
        let ids = track_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let url = format!("{}/tracks", self.endpoints.api_base);
        parse_tracks(self.get_json(&url, &[("ids", &ids)]).await?)
    }

    pub async fn fetch_tracks(&self, track_ids: &[i64]) -> Result<Vec<Track>> {
        let requests: Vec<_> = track_ids
            .chunks(TRACK_BATCH_SIZE)
            .map(|chunk| self.fetch_batch(chunk))
            .collect();
        let batches: Vec<Vec<Track>> = stream::iter(requests)
            .buffered(TRACK_BATCH_CONCURRENCY)
            .try_collect()
            .await?;
        let mut found: HashMap<i64, Track> = batches
            .into_iter()
            .flatten()
            .map(|track| (track.id, track))
            .collect();
        Ok(track_ids.iter().filter_map(|id| found.remove(id)).collect())
    }

    pub async fn hydrate(&self, tracks: &[Track]) -> Result<Vec<Track>> {
        let stubs: Vec<i64> = tracks
            .iter()
            .filter(|t| t.is_stub())
            .map(|t| t.id)
            .collect();
        if stubs.is_empty() {
            return Ok(tracks.to_vec());
        }
        let mut full: HashMap<i64, Track> = self
            .fetch_tracks(&stubs)
            .await?
            .into_iter()
            .map(|t| (t.id, t))
            .collect();
        Ok(tracks
            .iter()
            .map(|t| full.remove(&t.id).unwrap_or_else(|| t.clone()))
            .collect())
    }

    pub async fn stream_url(
        &self,
        transcoding_url: &str,
        track_authorization: Option<&str>,
    ) -> Result<String> {
        self.http.ensure_trusted(transcoding_url)?;
        let params: Vec<(&str, &str)> = track_authorization
            .map(|auth| ("track_authorization", auth))
            .into_iter()
            .collect();
        let data = self.get_json(transcoding_url, &params).await?;
        let url = data
            .get("url")
            .and_then(Value::as_str)
            .filter(|u| !u.is_empty())
            .ok_or_else(|| Error::StreamUnavailable("API nao retornou URL de stream.".into()))?;
        self.http.ensure_trusted(url)?;
        Ok(url.to_owned())
    }

    async fn expand_shortlink(&self, url: &str) -> Result<String> {
        let mut current =
            Url::parse(url).map_err(|err| Error::InvalidUrl(format!("URL malformada: {err}")))?;
        for _ in 0..MAX_SHORTLINK_HOPS {
            let response = self
                .http
                .get_no_redirect(current.as_str())
                .await
                .map_err(|err| Error::api(format!("Falha ao expandir shortlink: {err}"), None))?;
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let Some(location) = location.filter(|_| response.status().is_redirection()) else {
                break;
            };
            current = current
                .join(&location)
                .map_err(|err| Error::InvalidUrl(format!("Redirecionamento invalido: {err}")))?;
            let host = current.host_str().unwrap_or_default().to_ascii_lowercase();
            if WEB_HOSTS.contains(&host.as_str()) {
                return normalize_url(current.as_str());
            }
            if !SHORTLINK_HOSTS.contains(&host.as_str()) {
                return Err(Error::InvalidUrl(format!(
                    "Shortlink redireciona para dominio nao suportado: {host}"
                )));
            }
        }
        Err(Error::InvalidUrl(
            "Nao foi possivel expandir o shortlink do SoundCloud.".into(),
        ))
    }

    async fn get_json(&self, url: &str, params: &[(&str, &str)]) -> Result<Value> {
        let path = endpoint(url);
        for attempt in 1..=2 {
            let client_id = self.auth.client_id().await?;
            let mut query = params.to_vec();
            query.push(("client_id", &client_id));
            let (status, body) = {
                let _permit = self
                    .api_permits
                    .acquire()
                    .await
                    .map_err(|_| Error::Cancelled)?;
                let response =
                    self.http.get(url, &query).await.map_err(|err| {
                        Error::api(format!("Falha de rede em {path}: {err}"), None)
                    })?;
                let status = response.status().as_u16();
                let body = if response.status().is_success() {
                    Some(read_limited(response, MAX_API_BYTES, "resposta da API").await?)
                } else {
                    None
                };
                (status, body)
            };
            if matches!(status, 401 | 403)
                && attempt == 1
                && self.auth.refresh(&client_id).await? != client_id
            {
                continue;
            }
            return match (status, body) {
                (404, _) => Err(Error::NotFound(NOT_FOUND_MESSAGE.into())),
                (401 | 403, _) => Err(Error::api(
                    format!(
                        "Acesso negado em {path} (HTTP {status}); a faixa pode estar bloqueada na sua regiao."
                    ),
                    Some(status),
                )),
                (_, Some(body)) => serde_json::from_slice(&body)
                    .map_err(|_| Error::api(format!("Resposta nao-JSON em {path}"), Some(status))),
                _ => Err(Error::api(
                    format!("API respondeu HTTP {status} em {path}"),
                    Some(status),
                )),
            };
        }
        unreachable!("o laco sempre retorna na segunda tentativa")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const ID: &str = "abcdefghijklmnopqrstuvwxyz012345";
    const FRESH: &str = "ZYXWVUTSRQPONMLKJIHGFEDCBA987654";

    #[tokio::test]
    async fn resolves_and_hydrates_in_order() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/resolve"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "playlist", "title": "Mix", "tracks": [{"id": 3}, {"id": 1, "title": "one"}, {"id": 2}]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/tracks"))
            .and(query_param("ids", "3,2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!([{"id": 2, "title": "two"}, {"id": 3, "title": "three"}])),
            )
            .mount(&server)
            .await;

        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let Resource::Playlist(playlist) = client
            .resolve("https://soundcloud.com/a/sets/b")
            .await
            .expect("resolve")
        else {
            panic!("esperava playlist");
        };
        let titles: Vec<_> = client
            .hydrate(&playlist.tracks)
            .await
            .expect("hydrate")
            .iter()
            .map(Track::display_title)
            .collect();
        assert_eq!(titles, ["three", "one", "two"]);
    }

    #[tokio::test]
    async fn maps_not_found_and_refreshes_rejected_client_id() {
        let server = MockServer::start().await;
        Mock::given(path("/resolve"))
            .and(query_param("client_id", ID))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(path("/search/tracks"))
            .and(query_param("client_id", ID))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(path("/search/tracks"))
            .and(query_param("client_id", FRESH))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(path("/"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(format!(r#"<script>client_id:"{FRESH}"</script>"#)),
            )
            .mount(&server)
            .await;
        Mock::given(path("/resolve"))
            .and(query_param("client_id", FRESH))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let err = client
            .resolve("https://soundcloud.com/a/b")
            .await
            .expect_err("404");
        assert!(matches!(err, Error::NotFound(_)));
        assert_eq!(client.client_id().await.expect("id"), FRESH);
    }

    #[tokio::test]
    async fn stream_url_must_point_to_trusted_hosts() {
        let server = MockServer::start().await;
        Mock::given(path("/stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"url": "https://evil.example/a.mp3"})),
            )
            .mount(&server)
            .await;
        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let err = client
            .stream_url(&format!("{}/stream", server.uri()), Some("tok"))
            .await
            .expect_err("host proibido");
        assert!(matches!(err, Error::UntrustedUrl(_)));
    }

    #[tokio::test]
    async fn retries_transient_statuses() {
        let server = MockServer::start().await;
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "0"))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(path("/resolve"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"kind": "track", "id": 5, "title": "x"})),
            )
            .mount(&server)
            .await;
        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        assert!(matches!(
            client.resolve("https://soundcloud.com/a/b").await,
            Ok(Resource::Track(_))
        ));
    }

    #[tokio::test]
    async fn forbidden_with_still_valid_token_is_a_resource_error() {
        let server = MockServer::start().await;
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(path("/search/tracks"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let err = client
            .resolve("https://soundcloud.com/a/b")
            .await
            .expect_err("403");
        assert!(
            matches!(
                err,
                Error::Api {
                    status: Some(403),
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(!err.is_retryable());
        assert_eq!(client.client_id().await.expect("id"), ID);
    }

    #[tokio::test]
    async fn maps_unexpected_statuses_to_api_errors() {
        let server = MockServer::start().await;
        Mock::given(path("/resolve"))
            .respond_with(ResponseTemplate::new(418))
            .mount(&server)
            .await;
        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let err = client
            .resolve("https://soundcloud.com/a/b")
            .await
            .expect_err("418");
        assert!(matches!(
            err,
            Error::Api {
                status: Some(418),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn hydrate_keeps_stubs_the_api_did_not_return() {
        let server = MockServer::start().await;
        Mock::given(path("/tracks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let stub: Track = serde_json::from_value(json!({"id": 9})).expect("stub");
        let hydrated = client.hydrate(&[stub]).await.expect("hydrate");
        assert!(hydrated[0].is_stub());
        assert!(client.fetch_tracks(&[9]).await.expect("lote").is_empty());
    }

    #[tokio::test]
    async fn caches_discovered_client_id() {
        let server = MockServer::start().await;
        Mock::given(path("/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("client_id={FRESH}&x")),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path("/search/tracks"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = dir.path().join("client_id.json");
        let endpoints = Endpoints {
            api_base: server.uri(),
            web_base: server.uri(),
        };
        let provider = || {
            ClientIdProvider::new(Http::for_tests(), endpoints.clone(), None)
                .expect("provider")
                .with_cache_path(Some(cache.clone()))
        };

        assert_eq!(provider().client_id().await.expect("descoberta"), FRESH);
        assert!(
            std::fs::read_to_string(&cache)
                .expect("cache")
                .contains(FRESH)
        );
        assert_eq!(provider().client_id().await.expect("cache"), FRESH);
    }

    #[test]
    fn explicit_client_id_is_validated() {
        assert!(matches!(
            ClientIdProvider::new(
                Http::for_tests(),
                Endpoints::default(),
                Some("curto".into())
            ),
            Err(Error::InvalidInput(_))
        ));
    }

    async fn likes_server(next_href: impl FnOnce(&str) -> String) -> MockServer {
        let server = MockServer::start().await;
        let uri = server.uri();
        Mock::given(path("/resolve"))
            .and(query_param("url", "https://soundcloud.com/miu"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"kind": "user", "id": 42, "username": "Miu", "avatar_url": null}),
            ))
            .mount(&server)
            .await;
        Mock::given(path("/users/42/track_likes"))
            .and(query_param("linked_partitioning", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "collection": [
                    {"kind": "like", "track": {"id": 3, "title": "tres"}},
                    {"kind": "like", "track": {"id": 2, "title": "dois"}},
                    {"kind": "like", "playlist": {"id": 9}}
                ],
                "next_href": next_href(&uri)
            })))
            .mount(&server)
            .await;
        Mock::given(path("/users/42/track_likes"))
            .and(query_param("offset", "cursor-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "collection": [
                    {"kind": "like", "track": {"id": 2, "title": "dois"}},
                    {"kind": "like", "track": {"id": 1, "title": "um"}}
                ],
                "next_href": null
            })))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn likes_url_becomes_a_virtual_playlist_across_pages() {
        let server =
            likes_server(|uri| format!("{uri}/users/42/track_likes?offset=cursor-2&limit=200"))
                .await;
        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let Resource::Playlist(likes) = client
            .resolve("https://soundcloud.com/miu/likes")
            .await
            .expect("curtidas")
        else {
            panic!("esperava playlist virtual");
        };
        assert_eq!(likes.source, PlaylistSource::Likes);
        assert_eq!(likes.display_title(), "Likes");
        assert_eq!(likes.artist(), "Miu");
        assert_eq!(likes.track_ids(), [3, 2, 1], "ordem do site, sem repetidas");
        assert_eq!(likes.total_tracks(), 3);
    }

    #[tokio::test]
    async fn likes_pagination_refuses_foreign_next_href() {
        let server =
            likes_server(|_| "https://evil.example/users/42/track_likes?offset=x".to_owned()).await;
        let client = SoundCloudClient::for_tests(&server.uri(), Some(ID.into()));
        let err = client
            .resolve("https://soundcloud.com/miu/likes")
            .await
            .expect_err("next_href externo");
        assert!(err.to_string().contains("Paginacao"), "{err}");
    }
}
