pub mod dto;
mod error;
mod limit;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use perseus_core::features::download::{DownloadOptions, plan_remote, segment_urls};
use perseus_core::shared::config::{
    DEFAULT_DOWNLOAD_WORKERS, MAX_URL_LENGTH, WATCH_MAX_INTERVAL, WATCH_MIN_INTERVAL,
};
use perseus_core::shared::soundcloud::models::Resource;
use perseus_core::shared::soundcloud::urls::playlist_context;
use perseus_core::{SoundCloudClient, VERSION, inspect_url};
use regex::Regex;
use serde::Serialize;
use tower::util::MapRequest;
use tower_http::compression::CompressionLayer;

use crate::dto::{
    ConfigOut, FolderOut, InspectIn, InspectOut, Mode, PlanIn, PlanOut, SearchIn, StreamIn,
    StreamOut,
};
use crate::error::ApiError;
use crate::limit::RateLimiter;

pub const FUNCTION_PATH: &str = "/api/perseus";
pub const WEB_MAX_WORKERS: usize = 6;
pub const WEB_MAX_TRACKS: usize = 1000;
const INSPECT_LIMIT: usize = 500;
const SEARCH_LIMIT: usize = 20;
const MAX_QUERY_LENGTH: usize = 200;
const MAX_BODY_BYTES: usize = 16 * 1024;
const REQUESTS_PER_MINUTE: u32 = 150;
const PUBLIC_CACHE: &str = "public, max-age=0, s-maxage=120, stale-while-revalidate=600";

static TRANSCODING_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/media/soundcloud:tracks:\d+/[0-9A-Za-z-]+/stream/(progressive|hls)$")
        .expect("regex valida")
});
static TRACK_AUTHORIZATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9A-Za-z._-]{1,4096}$").expect("regex valida"));

struct AppState {
    client: Arc<SoundCloudClient>,
    limiter: RateLimiter,
}

pub type App = MapRequest<Router, fn(Request) -> Request>;

pub fn app(client: Arc<SoundCloudClient>) -> App {
    app_with_limit(client, REQUESTS_PER_MINUTE)
}

fn app_with_limit(client: Arc<SoundCloudClient>, requests_per_minute: u32) -> App {
    let state = Arc::new(AppState {
        client,
        limiter: RateLimiter::new(requests_per_minute, Duration::from_secs(60)),
    });
    let router = Router::new()
        .route("/api/config", get(config))
        .route("/api/inspect", get(inspect))
        .route("/api/search", get(search))
        .route("/api/plan", post(plan))
        .route("/api/stream", post(stream))
        .fallback(|| async { ApiError::not_found() })
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            rate_limit,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(CompressionLayer::new())
        .with_state(state);
    MapRequest::new(router, vercel_route as fn(Request) -> Request)
}

fn vercel_route(mut request: Request) -> Request {
    if request.uri().path() != FUNCTION_PATH {
        return request;
    }
    let query = request.uri().query().unwrap_or_default().to_owned();
    let route = url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "route")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty() && value.bytes().all(|b| b.is_ascii_lowercase()));
    if let Some(route) = route
        && let Ok(uri) = format!("/api/{route}?{query}").parse::<Uri>()
    {
        *request.uri_mut() = uri;
    }
    request
}

fn client_key(headers: &HeaderMap) -> String {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    header("x-real-ip")
        .or_else(|| header("x-forwarded-for"))
        .unwrap_or_else(|| "local".to_owned())
}

async fn rate_limit(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    match state
        .limiter
        .check(&client_key(request.headers()), Instant::now())
    {
        Ok(()) => next.run(request).await,
        Err(retry_after) => ApiError::rate_limited(retry_after).into_response(),
    }
}

fn json_with_cache<T: Serialize>(value: &T, cache: &'static str) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

fn bad_request(rejection: &impl std::fmt::Display) -> ApiError {
    ApiError::invalid(
        "invalid_request",
        format!("Requisicao invalida: {rejection}"),
    )
}

fn check_url(url: &str) -> Result<&str, ApiError> {
    let url = url.trim();
    if url.is_empty() || url.len() > MAX_URL_LENGTH {
        return Err(ApiError::invalid(
            "url_length",
            format!("A URL deve ter entre 1 e {MAX_URL_LENGTH} caracteres."),
        ));
    }
    Ok(url)
}

async fn config() -> Response {
    json_with_cache(
        &ConfigOut {
            version: VERSION,
            default_output_dir: String::new(),
            max_workers: WEB_MAX_WORKERS,
            default_workers: DEFAULT_DOWNLOAD_WORKERS,
            interval_min: WATCH_MIN_INTERVAL,
            interval_max: WATCH_MAX_INTERVAL,
        },
        "public, max-age=300, s-maxage=86400",
    )
}

async fn target(
    client: &SoundCloudClient,
    url: &str,
    mode: Mode,
) -> Result<(String, Option<String>), ApiError> {
    let canonical = client.canonicalize(url).await?;
    let context = playlist_context(&canonical);
    let target = match &context {
        Some(context) if mode != Mode::Track => context.clone(),
        _ => canonical,
    };
    Ok((target, context))
}

async fn inspect(
    State(state): State<Arc<AppState>>,
    query: Result<Query<InspectIn>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Query(input) = query.map_err(|err| bad_request(&err))?;
    let (url, context) = target(&state.client, check_url(&input.url)?, input.mode).await?;
    let result = inspect_url(&state.client, &url, INSPECT_LIMIT).await?;
    Ok(json_with_cache(
        &InspectOut::new(&result, url, context),
        PUBLIC_CACHE,
    ))
}

async fn search(
    State(state): State<Arc<AppState>>,
    query: Result<Query<SearchIn>, QueryRejection>,
) -> Result<Response, ApiError> {
    let Query(input) = query.map_err(|err| bad_request(&err))?;
    let text = input.query.trim();
    if text.is_empty() || text.chars().count() > MAX_QUERY_LENGTH {
        return Err(ApiError::invalid(
            "query_length",
            format!("A busca deve ter entre 1 e {MAX_QUERY_LENGTH} caracteres."),
        ));
    }
    let hits = state.client.search(text, SEARCH_LIMIT).await?;
    Ok(json_with_cache(&hits, PUBLIC_CACHE))
}

fn selected_count(resource: &Resource, limit: Option<usize>) -> usize {
    let take = |tracks: usize| limit.map_or(tracks, |limit| tracks.min(limit));
    match resource {
        Resource::Track(_) => 1,
        Resource::Playlist(playlist) => take(playlist.tracks.len()),
        Resource::Collection(collection) => collection
            .playlists
            .iter()
            .map(|playlist| take(playlist.tracks.len()))
            .sum(),
    }
}

async fn plan(
    State(state): State<Arc<AppState>>,
    body: Result<Json<PlanIn>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = body.map_err(|err| bad_request(&err))?;
    if input.mode == Mode::Watch {
        return Err(ApiError::invalid(
            "watch_unsupported",
            "O monitoramento so existe no app desktop.",
        ));
    }
    let options = DownloadOptions {
        quality: input.quality,
        name_template: input
            .name_template
            .map(|template| template.trim().to_owned())
            .filter(|template| !template.is_empty()),
        min_duration_s: input.min_duration_s,
        max_duration_s: input.max_duration_s,
        write_playlist_file: input.write_playlist_file,
        original_artwork: input.original_artwork,
        sync_removed: false,
        use_library: false,
        max_kbps: None,
    };
    options.validate()?;
    let (url, _) = target(&state.client, check_url(&input.url)?, input.mode).await?;
    let resource = state.client.resolve(&url).await?;
    let title = resource.display_title();
    if selected_count(&resource, input.limit) > WEB_MAX_TRACKS {
        return Err(ApiError::invalid(
            "too_many_tracks",
            format!(
                "A versao web baixa ate {WEB_MAX_TRACKS} faixas por vez; use o limite ou o app desktop."
            ),
        ));
    }
    let folders = plan_remote(&state.client, resource, input.limit, &options).await?;
    Ok(json_with_cache(
        &PlanOut {
            url,
            title,
            folders: folders.into_iter().map(FolderOut::from).collect(),
        },
        "no-store",
    ))
}

async fn stream(
    State(state): State<Arc<AppState>>,
    body: Result<Json<StreamIn>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = body.map_err(|err| bad_request(&err))?;
    let untrusted = || ApiError::invalid("untrusted_url", "URL de stream nao reconhecida.");
    let parsed = url::Url::parse(&input.transcoding_url).map_err(|_| untrusted())?;
    let protocol = TRANSCODING_PATH
        .captures(parsed.path())
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
        .filter(|_| parsed.query().is_none() && state.client.http().is_trusted(parsed.as_str()))
        .ok_or_else(untrusted)?;
    let authorization = input
        .track_authorization
        .as_deref()
        .filter(|value| !value.is_empty());
    if authorization.is_some_and(|value| !TRACK_AUTHORIZATION.is_match(value)) {
        return Err(ApiError::invalid(
            "invalid_input",
            "Autorizacao de faixa invalida.",
        ));
    }
    let url = state
        .client
        .stream_url(parsed.as_str(), authorization)
        .await?;
    let out = if protocol == "hls" {
        StreamOut::Hls {
            parts: segment_urls(state.client.http(), &url).await?,
        }
    } else {
        StreamOut::Progressive { url }
    };
    Ok(json_with_cache(&out, "no-store"))
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, StatusCode};
    use perseus_core::shared::soundcloud::client::SoundCloudClient;
    use serde_json::{Value, json};
    use tower::ServiceExt as _;
    use wiremock::matchers::{path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const CLIENT_ID: &str = "abcdefghijklmnopqrstuvwxyz012345";
    const PLAYLIST: &str = "https://soundcloud.com/perseus/sets/argonautas";

    fn client(server: &MockServer) -> Arc<SoundCloudClient> {
        Arc::new(SoundCloudClient::for_tests(
            &server.uri(),
            Some(CLIENT_ID.into()),
        ))
    }

    fn media_url(base: &str, id: i64, protocol: &str) -> String {
        format!("{base}/media/soundcloud:tracks:{id}/0f1e-2d3c/stream/{protocol}")
    }

    fn track(base: &str, id: i64, title: &str, protocol: &str, mime: &str) -> Value {
        json!({"kind": "track", "id": id, "title": title, "user": {"username": "Perseus"}, "duration": 180_000,
               "permalink_url": format!("https://soundcloud.com/perseus/t{id}"),
               "track_authorization": "eyJhbGciOiJIUzI1NiJ9.e30.sig",
               "media": {"transcodings": [{"url": media_url(base, id, protocol), "preset": "mp3_0_0",
                                           "format": {"protocol": protocol, "mime_type": mime}}]}})
    }

    async fn mount_playlist(server: &MockServer, tracks: Value) {
        Mock::given(path("/resolve"))
            .and(query_param("url", PLAYLIST))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "playlist", "title": "Argonautas", "user": {"username": "Perseus"},
                "permalink_url": PLAYLIST, "tracks": tracks
            })))
            .mount(server)
            .await;
    }

    async fn send(
        app: App,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let request = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-forwarded-for", "203.0.113.7, 10.0.0.1")
            .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
            .expect("requisicao");
        let response = app.oneshot(request).await.expect("resposta");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("corpo");
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, headers, value)
    }

    #[tokio::test]
    async fn config_exposes_web_limits() {
        let server = MockServer::start().await;
        let (status, headers, body) =
            send(app(client(&server)), Method::GET, "/api/config", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["version"], VERSION);
        assert_eq!(body["max_workers"], WEB_MAX_WORKERS);
        assert_eq!(body["default_output_dir"], "");
        assert!(
            headers[header::CACHE_CONTROL]
                .to_str()
                .expect("ascii")
                .contains("max-age")
        );
    }

    #[tokio::test]
    async fn vercel_rewrite_and_original_path_reach_the_same_route() {
        let server = MockServer::start().await;
        let (status, _, body) = send(
            app(client(&server)),
            Method::GET,
            "/api/perseus?route=config",
            None,
        )
        .await;
        assert_eq!(
            (status, body["max_workers"].as_u64()),
            (StatusCode::OK, Some(6))
        );
        let (status, _, body) = send(
            app(client(&server)),
            Method::GET,
            "/api/perseus?route=../etc",
            None,
        )
        .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::NOT_FOUND, Some("route_not_found"))
        );
    }

    #[tokio::test]
    async fn inspect_matches_the_desktop_contract() {
        let server = MockServer::start().await;
        let base = server.uri();
        mount_playlist(
            &server,
            json!([track(&base, 1, "Um", "progressive", "audio/mpeg")]),
        )
        .await;
        let uri = format!("/api/inspect?url={}&mode=playlist", urlencode(PLAYLIST));
        let (status, headers, body) = send(app(client(&server)), Method::GET, &uri, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["kind"], "playlist");
        assert_eq!(body["title"], "Argonautas");
        assert_eq!(body["tracks"][0]["available"], true);
        assert_eq!(body["tracks"][0]["position"], 1);
        assert_eq!(headers[header::CACHE_CONTROL], PUBLIC_CACHE);
    }

    #[tokio::test]
    async fn invalid_links_become_translatable_errors() {
        let server = MockServer::start().await;
        let (status, headers, body) = send(
            app(client(&server)),
            Method::GET,
            "/api/inspect?url=https%3A%2F%2Fevil.example%2Fx&mode=playlist",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            (body["kind"].as_str(), body["code"].as_str()),
            (Some("invalid_input"), Some("invalid_url"))
        );
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");

        let (status, _, body) = send(
            app(client(&server)),
            Method::GET,
            "/api/inspect?mode=playlist",
            None,
        )
        .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("invalid_request"))
        );
    }

    #[tokio::test]
    async fn plan_returns_everything_the_browser_needs() {
        let server = MockServer::start().await;
        let base = server.uri();
        let drm = json!({"kind": "track", "id": 3, "title": "Tres", "user": {"username": "Perseus"},
                         "media": {"transcodings": [{"url": media_url(&base, 3, "hls"),
                                   "format": {"protocol": "ctr-encrypted-hls", "mime_type": "audio/mp4"}}]}});
        mount_playlist(
            &server,
            json!([
                track(&base, 1, "Um", "progressive", "audio/mpeg"),
                track(&base, 2, "Dois", "hls", "audio/mp4; codecs=\"mp4a.40.2\""),
                drm
            ]),
        )
        .await;
        let (status, headers, body) = send(
            app(client(&server)),
            Method::POST,
            "/api/plan",
            Some(
                json!({"url": PLAYLIST, "mode": "playlist", "limit": null, "quality": "compatible",
                        "name_template": null, "min_duration_s": null, "max_duration_s": null,
                        "write_playlist_file": true, "original_artwork": false}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        let folder = &body["folders"][0];
        assert_eq!(folder["folder"], "Perseus - Argonautas");
        assert_eq!(folder["playlist_file"], "Argonautas.m3u8");
        let one = &folder["items"][0];
        assert_eq!(one["status"], "ready");
        assert_eq!(one["file_name"], "01. Perseus - Um.mp3");
        assert_eq!(one["container"], "mp3");
        assert_eq!(one["tags"]["album_artist"], "Perseus");
        assert_eq!(folder["items"][1]["container"], "mp4");
        assert_eq!(folder["items"][2]["status"], "unavailable");
        assert_eq!(folder["items"][2]["reason_code"], "drm");
    }

    #[tokio::test]
    async fn plan_rejects_watch_bad_fields_and_huge_selections() {
        let server = MockServer::start().await;
        let stubs: Vec<Value> = (1..=1001).map(|id| json!({"id": id})).collect();
        mount_playlist(&server, json!(stubs)).await;
        let cases = [
            (
                json!({"url": PLAYLIST, "mode": "watch"}),
                "watch_unsupported",
            ),
            (
                json!({"url": PLAYLIST, "output_dir": "C:/"}),
                "invalid_request",
            ),
            (json!({"url": PLAYLIST}), "too_many_tracks"),
            (
                json!({"url": PLAYLIST, "name_template": "{number}"}),
                "invalid_input",
            ),
        ];
        for (body, code) in cases {
            let (status, _, body) =
                send(app(client(&server)), Method::POST, "/api/plan", Some(body)).await;
            assert_eq!(
                (status, body["code"].as_str()),
                (StatusCode::BAD_REQUEST, Some(code))
            );
        }
        Mock::given(path("/tracks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let (status, _, body) = send(
            app(client(&server)),
            Method::POST,
            "/api/plan",
            Some(json!({"url": PLAYLIST, "limit": 10})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["folders"][0]["items"].as_array().map(Vec::len),
            Some(10)
        );
    }

    #[tokio::test]
    async fn stream_signs_progressive_and_expands_hls() {
        let server = MockServer::start().await;
        let base = server.uri();
        Mock::given(path(
            "/media/soundcloud:tracks:1/0f1e-2d3c/stream/progressive",
        ))
        .and(query_param("track_authorization", "eyJ.e30.sig"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"url": format!("{base}/audio.mp3?Policy=x")})),
        )
        .mount(&server)
        .await;
        Mock::given(path("/media/soundcloud:tracks:2/0f1e-2d3c/stream/hls"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"url": format!("{base}/hls/playlist.m3u8")})),
            )
            .mount(&server)
            .await;
        Mock::given(path("/hls/playlist.m3u8"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:2.0,\ns0.m4s\n#EXTINF:2.0,\ns1.m4s\n#EXT-X-ENDLIST\n",
            ))
            .mount(&server)
            .await;

        let (status, headers, body) = send(
            app(client(&server)),
            Method::POST,
            "/api/stream",
            Some(json!({"transcoding_url": media_url(&base, 1, "progressive"), "track_authorization": "eyJ.e30.sig"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            body,
            json!({"protocol": "progressive", "url": format!("{base}/audio.mp3?Policy=x")})
        );

        let (status, _, body) = send(
            app(client(&server)),
            Method::POST,
            "/api/stream",
            Some(json!({"transcoding_url": media_url(&base, 2, "hls")})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["parts"],
            json!([
                format!("{base}/hls/init.mp4"),
                format!("{base}/hls/s0.m4s"),
                format!("{base}/hls/s1.m4s")
            ])
        );
    }

    #[tokio::test]
    async fn stream_refuses_anything_but_transcoding_urls() {
        let server = MockServer::start().await;
        let base = server.uri();
        let cases = [
            json!({"transcoding_url": format!("{base}/users/1/likes")}),
            json!({"transcoding_url": format!("{}?client_id=x", media_url(&base, 1, "hls"))}),
            json!({"transcoding_url": "https://evil.example/media/soundcloud:tracks:1/a/stream/hls"}),
            json!({"transcoding_url": media_url(&base, 1, "hls"), "track_authorization": "a b"}),
            json!({"transcoding_url": "not a url"}),
        ];
        for body in cases {
            let (status, _, response) = send(
                app(client(&server)),
                Method::POST,
                "/api/stream",
                Some(body),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        }
    }

    #[tokio::test]
    async fn compresses_json_for_clients_that_accept_gzip() {
        let server = MockServer::start().await;
        let request = axum::http::Request::builder()
            .uri("/api/config")
            .header("accept-encoding", "gzip")
            .body(Body::empty())
            .expect("requisicao");
        let response = app(client(&server))
            .oneshot(request)
            .await
            .expect("resposta");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
    }

    #[tokio::test]
    async fn rate_limit_answers_429_with_retry_after() {
        let server = MockServer::start().await;
        let app = app_with_limit(client(&server), 1);
        let (status, _, _) = send(app.clone(), Method::GET, "/api/config", None).await;
        assert_eq!(status, StatusCode::OK);
        let (status, headers, body) = send(app, Method::GET, "/api/config", None).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body["kind"], "rate_limited");
        assert!(headers.contains_key(header::RETRY_AFTER));
    }

    #[test]
    fn client_key_prefers_the_edge_headers() {
        let mut headers = HeaderMap::new();
        assert_eq!(client_key(&headers), "local");
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.1, 10.0.0.2"),
        );
        assert_eq!(client_key(&headers), "198.51.100.1");
        headers.insert("x-real-ip", HeaderValue::from_static("198.51.100.9"));
        assert_eq!(client_key(&headers), "198.51.100.9");
    }

    fn urlencode(value: &str) -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    }
}
