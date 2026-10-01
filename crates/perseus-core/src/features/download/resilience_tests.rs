use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::events::Reporter;
use crate::features::download::{DownloadReport, DownloadRequest, run_download};
use crate::shared::config::ARCHIVE_FILE;
use crate::shared::filesystem::atomic_write;
use crate::shared::soundcloud::client::SoundCloudClient;
use crate::test_support::{
    LIVE_BYTES, RawRequest, RawResponse, RawServer, client, mount_media, mp3_frames, track,
};

const FOLDER: &str = "Perseus - Argonautas";

fn request_in(dir: &tempfile::TempDir) -> DownloadRequest {
    let mut request = DownloadRequest::new("https://soundcloud.com/perseus/sets/argonautas");
    request.output_dir = dir.path().join("musica");
    request.library_path = Some(dir.path().join("library.json"));
    std::fs::create_dir_all(&request.output_dir).expect("pasta");
    request
}

fn playlist(tracks: &[serde_json::Value]) -> serde_json::Value {
    json!({"kind": "playlist", "title": "Argonautas", "user": {"username": "Perseus"}, "tracks": tracks})
}

fn progressive(base: &str, count: i64) -> Vec<serde_json::Value> {
    (1..=count)
        .map(|id| track(id, &format!("Faixa {id}"), "progressive", base))
        .collect()
}

async fn wiremock_with(tracks: impl FnOnce(&str) -> Vec<serde_json::Value>) -> MockServer {
    let server = MockServer::start().await;
    let body = playlist(&tracks(&server.uri()));
    Mock::given(path("/resolve"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn download(client: Arc<SoundCloudClient>, request: DownloadRequest) -> DownloadReport {
    run_download(
        client,
        request,
        CancellationToken::new(),
        Reporter::silent("t"),
    )
    .await
}

fn part_files(dir: &std::path::Path) -> Vec<String> {
    walk(dir)
        .into_iter()
        .filter(|name| {
            let path = std::path::Path::new(name);
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("part"))
                || name.to_lowercase().contains(".tmp")
        })
        .collect()
}

fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            names.extend(walk(&path));
        } else {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names
}

fn audio_payload(bytes: &[u8]) -> &[u8] {
    if bytes.len() >= 10 && &bytes[..3] == b"ID3" {
        let size = bytes[6..10]
            .iter()
            .fold(0_usize, |acc, b| (acc << 7) | usize::from(*b & 0x7F));
        &bytes[10 + size..]
    } else {
        bytes
    }
}

fn assert_clean_success(report: &DownloadReport, expected: usize) {
    assert!(report.fatal_error.is_none(), "{:?}", report.fatal_error);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.completed.len(), expected);
}

async fn raw_playlist(
    count: i64,
    audio: impl Fn(&RawRequest, usize) -> RawResponse + Send + Sync + 'static,
) -> RawServer {
    let audio_calls = AtomicUsize::new(0);
    let base = Arc::new(std::sync::OnceLock::<String>::new());
    let base_for_handler = Arc::clone(&base);
    let server = RawServer::start(move |request| {
        let base = base_for_handler.get().expect("base");
        match request.path.as_str() {
            "/resolve" => RawResponse::json(&playlist(&progressive(base, count))),
            "/progressive" => RawResponse::json(&json!({"url": format!("{base}/audio.mp3")})),
            "/audio.mp3" => audio(request, audio_calls.fetch_add(1, Ordering::SeqCst)),
            _ => RawResponse::Full {
                status: 404,
                content_type: "text/plain",
                headers: Vec::new(),
                body: Vec::new(),
            },
        }
    })
    .await;
    base.set(server.base.clone()).expect("base unica");
    server
}

#[tokio::test]
async fn connection_dropped_mid_file_resumes_with_range() {
    let full = mp3_frames(200);
    let half = full.len() / 2;
    let body = full.clone();
    let server = raw_playlist(1, move |request, _| match request.range_from {
        Some(from) => RawResponse::partial(&body, from),
        None => RawResponse::Truncated {
            declared: body.len(),
            body: body[..half].to_vec(),
        },
    })
    .await;
    let dir = tempfile::tempdir().expect("tempdir");

    let report = download(server.client(), request_in(&dir)).await;

    assert_clean_success(&report, 1);
    let requests = server.stats.requests.lock().expect("lock").clone();
    let audio: Vec<_> = requests.iter().filter(|r| r.path == "/audio.mp3").collect();
    assert_eq!(audio.len(), 2, "uma queda e uma retomada");
    assert_eq!(
        audio[1].range_from,
        Some(half as u64),
        "retoma do byte em que parou"
    );
    let saved = std::fs::read(
        dir.path()
            .join("musica")
            .join(FOLDER)
            .join("01. Perseus - Faixa 1.mp3"),
    )
    .expect("arquivo");
    assert!(
        audio_payload(&saved) == full.as_slice(),
        "audio final identico ao original, sem duplicar o trecho ja baixado"
    );
    assert!(part_files(dir.path()).is_empty());
}

#[tokio::test]
async fn corrupted_partial_file_is_discarded_instead_of_trusted() {
    let full = mp3_frames(200);
    let body = full.clone();
    let server = raw_playlist(1, move |request, _| match request.range_from {
        Some(from) => RawResponse::partial(&body, from),
        None => RawResponse::audio(body.clone()),
    })
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = dir.path().join("musica").join(FOLDER);
    std::fs::create_dir_all(&folder).expect("pasta");
    std::fs::write(
        folder.join("01. Perseus - Faixa 1.mp3.part"),
        vec![0x42; 5000],
    )
    .expect("parcial");

    let report = download(server.client(), request_in(&dir)).await;

    assert_clean_success(&report, 1);
    let saved = std::fs::read(folder.join("01. Perseus - Faixa 1.mp3")).expect("arquivo");
    assert!(
        audio_payload(&saved) == full.as_slice(),
        "o lixo do parcial nao pode sobreviver no arquivo final"
    );
    assert!(part_files(dir.path()).is_empty());
}

#[tokio::test]
async fn honors_retry_after_on_rate_limit() {
    let server = wiremock_with(|base| progressive(base, 1)).await;
    Mock::given(path("/audio.mp3"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");

    let started = Instant::now();
    let report = download(client(&server), request_in(&dir)).await;

    assert_clean_success(&report, 1);
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "respeitou Retry-After"
    );
    assert_eq!(report.stats.transport_retries, 1);
}

#[tokio::test]
async fn cancel_interrupts_a_hanging_transfer_promptly() {
    let server = wiremock_with(|base| progressive(base, 3)).await;
    Mock::given(path("/audio.mp3"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(60)))
        .mount(&server)
        .await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        trigger.cancel();
    });

    let started = Instant::now();
    let report = run_download(
        client(&server),
        request_in(&dir),
        cancel,
        Reporter::silent("t"),
    )
    .await;

    assert!(report.cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "cancelou em {:?}",
        started.elapsed()
    );
    assert!(
        report.failed.is_empty(),
        "cancelamento nao e falha: {:?}",
        report.failed
    );
    assert!(
        part_files(dir.path()).is_empty(),
        "{:?}",
        part_files(dir.path())
    );
}

#[tokio::test]
async fn chaos_of_random_server_errors_still_completes() {
    let server = wiremock_with(|base| progressive(base, 24)).await;
    let errors = [500_u16, 502, 503, 504];
    for (index, status) in errors.iter().enumerate() {
        Mock::given(path("/audio.mp3"))
            .respond_with(ResponseTemplate::new(*status))
            .up_to_n_times(2 + index as u64)
            .mount(&server)
            .await;
    }
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");

    let report = download(client(&server), request_in(&dir)).await;

    assert_clean_success(&report, 24);
    assert_eq!(report.stats.transport_retries, 2 + 3 + 4 + 5);
}

#[tokio::test]
async fn permanent_cdn_error_fails_only_that_track() {
    let server = MockServer::start().await;
    let base = server.uri();
    let mut tracks = progressive(&base, 2);
    tracks.push(json!({"id": 3, "title": "Quebrada", "user": {"username": "Perseus"},
        "media": {"transcodings": [{"url": format!("{base}/gone"), "format": {"protocol": "progressive", "mime_type": "audio/mpeg"}}]}}));
    Mock::given(path("/resolve"))
        .respond_with(ResponseTemplate::new(200).set_body_json(playlist(&tracks)))
        .mount(&server)
        .await;
    Mock::given(path("/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");

    let report = download(client(&server), request_in(&dir)).await;

    assert!(report.fatal_error.is_none());
    assert_eq!(report.completed.len(), 2);
    assert!(report.failed.contains_key(&3) || report.unavailable.contains_key(&3));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_jobs_on_the_same_folder_do_not_corrupt_files() {
    let server = wiremock_with(|base| progressive(base, 12)).await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let (a, b) = (request_in(&dir), request_in(&dir));

    let (first, second) = tokio::join!(download(client(&server), a), download(client(&server), b));

    for report in [&first, &second] {
        assert!(report.fatal_error.is_none(), "{:?}", report.fatal_error);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
    }
    let folder = dir.path().join("musica").join(FOLDER);
    for id in 1..=12 {
        let file = folder.join(format!("{id:02}. Perseus - Faixa {id}.mp3"));
        let bytes = std::fs::read(&file).expect("faixa presente");
        assert!(
            audio_payload(&bytes) == mp3_frames(200).as_slice(),
            "{} corrompido",
            file.display()
        );
    }
    assert!(
        part_files(dir.path()).is_empty(),
        "{:?}",
        part_files(dir.path())
    );
    let archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join(ARCHIVE_FILE)).expect("archive"))
            .expect("json");
    assert_eq!(archive["tracks"].as_object().expect("tracks").len(), 12);
}

#[tokio::test]
async fn api_concurrency_stays_within_the_limit() {
    const STUBS: i64 = 400;
    let base = Arc::new(std::sync::OnceLock::<String>::new());
    let shared = Arc::clone(&base);
    let server = RawServer::start(move |request| {
        let base = shared.get().expect("base");
        match request.path.as_str() {
            "/resolve" => {
                let stubs: Vec<_> = (1..=STUBS).map(|id| json!({"id": id})).collect();
                RawResponse::json(&playlist(&stubs))
            }
            "/tracks" => {
                let ids: Vec<i64> = request
                    .query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("ids="))
                    .unwrap_or_default()
                    .split("%2C")
                    .flat_map(|chunk| chunk.split(','))
                    .filter_map(|id| id.parse().ok())
                    .collect();
                let tracks: Vec<_> = ids
                    .iter()
                    .map(|id| json!({"id": id, "title": format!("Faixa {id}"), "duration": 1_000_000,
                        "media": {"transcodings": [{"url": format!("{base}/progressive"), "format": {"protocol": "progressive", "mime_type": "audio/mpeg"}}]}}))
                    .collect();
                RawResponse::Delayed(Duration::from_millis(40), Box::new(RawResponse::json(&json!(tracks))))
            }
            _ => RawResponse::json(&json!({})),
        }
    })
    .await;
    base.set(server.base.clone()).expect("base unica");
    let dir = tempfile::tempdir().expect("tempdir");
    let mut request = request_in(&dir);
    request.options.max_duration_s = Some(1);

    let report = download(server.client(), request).await;

    assert!(report.fatal_error.is_none(), "{:?}", report.fatal_error);
    assert_eq!(
        report.unavailable.len(),
        STUBS as usize,
        "todas hidratadas e filtradas"
    );
    assert_eq!(server.stats.count("/tracks"), 8, "400 ids em lotes de 50");
    let peak = server.stats.max_inflight.load(Ordering::SeqCst);
    assert!(peak <= 8, "pico de {peak} requisicoes simultaneas a API");
}

#[tokio::test]
async fn lost_archive_is_rebuilt_without_downloading_again() {
    let server = wiremock_with(|base| progressive(base, 3)).await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    assert_clean_success(&download(client(&server), request_in(&dir)).await, 3);
    let archive = dir.path().join("musica").join(FOLDER).join(ARCHIVE_FILE);
    std::fs::remove_file(&archive).expect("apaga archive");

    let again = download(client(&server), request_in(&dir)).await;

    assert_eq!(
        again.reused_count(),
        3,
        "arquivos no disco bastam para restaurar"
    );
    assert!(archive.is_file(), "archive reconstruido");
}

#[tokio::test]
async fn truncated_template_names_are_still_unique() {
    let long = "Mix ".repeat(80);
    let server = wiremock_with(|base| {
        vec![
            track(1, &long, "progressive", base),
            track(2, &long, "progressive", base),
        ]
    })
    .await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut request = request_in(&dir);
    request.options.name_template = Some("{title} [{id}]".into());

    let report = download(client(&server), request).await;

    assert_eq!(report.downloaded().count(), 2);
    let folder = dir.path().join("musica").join(FOLDER);
    let audio = walk(&folder)
        .into_iter()
        .filter(|n| {
            std::path::Path::new(n)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mp3"))
        })
        .count();
    assert_eq!(audio, 2, "dois arquivos, mesmo com o id truncado");
}

#[tokio::test]
async fn changing_the_name_template_reuses_files_through_the_archive() {
    let server = wiremock_with(|base| progressive(base, 3)).await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    assert_clean_success(&download(client(&server), request_in(&dir)).await, 3);
    let mut renamed = request_in(&dir);
    renamed.options.name_template = Some("{artist} - {title} [{id}]".into());

    let again = download(client(&server), renamed).await;

    assert_eq!(
        again.reused_count(),
        3,
        "nenhum download novo so por trocar o nome"
    );
    assert_eq!(again.downloaded().count(), 0);
}

#[tokio::test]
async fn legacy_and_future_archive_versions_are_read() {
    let server = wiremock_with(|base| progressive(base, 1)).await;
    mount_media(&server).await;
    for (label, content) in [
        (
            "sem versao",
            json!({"tracks": {"1": {"file": "Minha faixa.mp3", "title": "t", "artist": "a"}}}),
        ),
        (
            "versao futura",
            json!({"version": 9, "extra": true, "tracks": {"1": {"file": "Minha faixa.mp3", "title": "t", "artist": "a", "novo": 1}}}),
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = dir.path().join("musica").join(FOLDER);
        std::fs::create_dir_all(&folder).expect("pasta");
        std::fs::write(folder.join(ARCHIVE_FILE), content.to_string()).expect("archive");
        std::fs::write(folder.join("Minha faixa.mp3"), mp3_frames(200)).expect("faixa");

        let report = download(client(&server), request_in(&dir)).await;

        assert!(
            report.fatal_error.is_none(),
            "{label}: {:?}",
            report.fatal_error
        );
        assert!(report.failed.is_empty(), "{label}: {:?}", report.failed);
        assert_eq!(
            report.reused_count(),
            1,
            "{label}: archive lido e faixa reaproveitada"
        );
    }
}

#[test]
#[allow(
    clippy::permissions_set_readonly_false,
    reason = "arquivo temporario do teste; so desfaz o somente-leitura"
)]
fn failed_atomic_write_keeps_the_previous_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("library.json");
    std::fs::write(&target, b"anterior").expect("escrita");
    let mut permissions = std::fs::metadata(&target).expect("meta").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&target, permissions.clone()).expect("somente leitura");

    let result = atomic_write(&target, b"novo");

    permissions.set_readonly(false);
    std::fs::set_permissions(&target, permissions).expect("restaura");
    if result.is_err() {
        assert_eq!(
            std::fs::read(&target).expect("leitura"),
            b"anterior",
            "nada pela metade"
        );
    } else {
        assert_eq!(std::fs::read(&target).expect("leitura"), b"novo");
    }
    let leftovers: Vec<_> = walk(dir.path())
        .into_iter()
        .filter(|n| n != "library.json")
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporarios esquecidos: {leftovers:?}"
    );
}

#[tokio::test]
async fn same_title_with_title_only_template_never_reuses_the_wrong_track() {
    let server = MockServer::start().await;
    let base = server.uri();
    let tracks = vec![
        track(1, "Intro", "progressive", &base),
        track(2, "Intro", "progressive", &base),
    ];
    Mock::given(path("/resolve"))
        .respond_with(ResponseTemplate::new(200).set_body_json(playlist(&tracks)))
        .mount(&server)
        .await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut request = request_in(&dir);
    request.options.name_template = Some("{title}".into());

    let report = download(client(&server), request.clone()).await;

    assert_eq!(
        report.downloaded().count(),
        2,
        "duas faixas distintas, dois downloads"
    );
    let folder = dir.path().join("musica").join(FOLDER);
    let audio: Vec<_> = walk(&folder)
        .into_iter()
        .filter(|n| {
            std::path::Path::new(n)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mp3"))
        })
        .collect();
    assert_eq!(audio.len(), 2, "{audio:?}");

    let again = download(client(&server), request).await;
    assert_eq!(
        again.reused_count(),
        2,
        "cada faixa reaproveita o proprio arquivo"
    );
}

async fn timed_download(workers: usize, tracks: i64, delay: Duration) -> Duration {
    let server = wiremock_with(|base| progressive(base, tracks)).await;
    Mock::given(path("/audio.mp3"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "audio/mpeg")
                .set_body_bytes(mp3_frames(200))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut request = request_in(&dir);
    request.workers = workers;
    let started = Instant::now();
    assert_clean_success(&download(client(&server), request).await, tracks as usize);
    started.elapsed()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "desempenho"]
async fn perf_scalability_workers_multiply_throughput() {
    let delay = Duration::from_millis(150);
    let mut rows = Vec::new();
    for workers in [1, 4, 8, 16] {
        let elapsed = timed_download(workers, 48, delay).await;
        rows.push((workers, elapsed));
        eprintln!(
            "escalabilidade: {workers:>2} workers -> {elapsed:?} ({:.1} faixas/s)",
            48.0 / elapsed.as_secs_f64()
        );
    }
    let speedup = rows[0].1.as_secs_f64() / rows[3].1.as_secs_f64();
    assert!(
        speedup >= 6.0,
        "16 workers deveriam render >= 6x de 1 worker; rendeu {speedup:.1}x"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "desempenho"]
async fn perf_spike_of_failures_is_absorbed() {
    let server = wiremock_with(|base| progressive(base, 200)).await;
    Mock::given(path("/audio.mp3"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(120)
        .mount(&server)
        .await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut request = request_in(&dir);
    request.workers = 16;

    let started = Instant::now();
    let report = download(client(&server), request).await;

    assert_clean_success(&report, 200);
    assert_eq!(
        report.stats.transport_retries + report.stats.retries,
        120,
        "cada 503 foi absorvido por uma das camadas de retry"
    );
    eprintln!(
        "pico: 120 x 503 absorvidos em {:?} ({} no transporte, {} na faixa)",
        started.elapsed(),
        report.stats.transport_retries,
        report.stats.retries
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "desempenho; rode sozinho com --test-threads=1"]
async fn perf_soak_repeated_runs_do_not_leak_or_slow_down() {
    let server = wiremock_with(|base| progressive(base, 40)).await;
    mount_media(&server).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let client = client(&server);
    assert_clean_success(&download(Arc::clone(&client), request_in(&dir)).await, 40);

    let mut samples = Vec::new();
    for round in 0..60 {
        let started = Instant::now();
        let report = download(Arc::clone(&client), request_in(&dir)).await;
        assert_eq!(report.reused_count(), 40, "rodada {round}");
        samples.push((started.elapsed(), LIVE_BYTES.load(Ordering::Relaxed)));
    }
    let (early_time, early_bytes) = samples[9];
    let (late_time, late_bytes) = samples[59];
    eprintln!(
        "soak: rodada 10 {early_time:?} / {early_bytes} B vivos; rodada 60 {late_time:?} / {late_bytes} B vivos"
    );
    assert!(
        late_bytes <= early_bytes + 2 * 1024 * 1024,
        "memoria cresceu {} B em 50 rodadas",
        late_bytes.saturating_sub(early_bytes)
    );
    assert!(
        late_time <= early_time * 4 + Duration::from_millis(200),
        "degradou: {early_time:?} -> {late_time:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "desempenho"]
async fn perf_volume_five_thousand_stub_playlist_hydrates_in_batches() {
    const STUBS: i64 = 5000;
    let base = Arc::new(std::sync::OnceLock::<String>::new());
    let shared = Arc::clone(&base);
    let server = RawServer::start(move |request| {
        let base = shared.get().expect("base");
        match request.path.as_str() {
            "/resolve" => {
                let stubs: Vec<_> = (1..=STUBS).map(|id| json!({"id": id})).collect();
                RawResponse::json(&playlist(&stubs))
            }
            "/tracks" => {
                let ids: Vec<i64> = request
                    .query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("ids="))
                    .unwrap_or_default()
                    .split("%2C")
                    .flat_map(|chunk| chunk.split(','))
                    .filter_map(|id| id.parse().ok())
                    .collect();
                let tracks: Vec<_> = ids
                    .iter()
                    .map(|id| json!({"id": id, "title": format!("Faixa {id}"), "duration": 1_000_000,
                        "media": {"transcodings": [{"url": format!("{base}/progressive"), "format": {"protocol": "progressive", "mime_type": "audio/mpeg"}}]}}))
                    .collect();
                RawResponse::json(&json!(tracks))
            }
            _ => RawResponse::json(&json!({})),
        }
    })
    .await;
    base.set(server.base.clone()).expect("base unica");
    let dir = tempfile::tempdir().expect("tempdir");
    let mut request = request_in(&dir);
    request.options.max_duration_s = Some(1);

    let started = Instant::now();
    let report = download(server.client(), request).await;

    assert_eq!(report.unavailable.len(), STUBS as usize);
    assert_eq!(server.stats.count("/tracks"), 100);
    eprintln!(
        "volume: {STUBS} faixas hidratadas em 100 lotes em {:?}",
        started.elapsed()
    );
    assert!(started.elapsed() < Duration::from_secs(30));
}
