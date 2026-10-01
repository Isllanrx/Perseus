#![forbid(unsafe_code)]

use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, ValueEnum};
use perseus_core::events::new_run_id;
use perseus_core::shared::config::{
    DEFAULT_DOWNLOAD_WORKERS, MAX_DOWNLOAD_WORKERS, MAX_LIMIT, WATCH_MAX_INTERVAL,
    WATCH_MIN_INTERVAL, default_output_dir,
};
use perseus_core::shared::error::ErrorKind;
use perseus_core::shared::filesystem::atomic_write;
use perseus_core::shared::format::{format_bytes, format_duration};
use perseus_core::shared::soundcloud::models::{CollectionKind, Resource};
use perseus_core::shared::soundcloud::transcoding::Quality;
use perseus_core::shared::soundcloud::urls::{is_valid_client_id, playlist_context};
use perseus_core::{
    DownloadOptions, DownloadReport, DownloadRequest, Error, InspectResult, PlaylistWatcher,
    Reporter, SoundCloudClient, WatchConfig, inspect_url, run_download,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

const MAX_REASON_ROWS: usize = 25;
const INSPECT_PREVIEW: usize = 10;
const SEARCH_RESULTS: usize = 10;

#[derive(Clone, Copy, ValueEnum)]
enum LogFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum QualityArg {
    #[value(help = "MP3 (toca em qualquer player)")]
    Compatible,
    #[value(help = "Maior bitrate aberto (ex.: AAC 160 kbps)")]
    Best,
}

#[derive(Parser)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "flags de CLI sao booleanos por natureza"
)]
#[command(
    name = "perseus-cli",
    version,
    about = "Baixa faixas e playlists publicas do SoundCloud sem conta de usuario."
)]
struct Args {
    #[arg(
        help = "URL de faixa, playlist, album, perfil (e abas: tracks, popular-tracks, reposts, likes, albums, sets), relacionadas (.../recommended) ou shortlink on.soundcloud.com"
    )]
    url: Option<String>,
    #[arg(
        short,
        long,
        conflicts_with = "url",
        help = "Buscar no SoundCloud em vez de informar URL (escolhe o resultado; sem terminal, o primeiro)"
    )]
    search: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value = "compatible",
        help = "Qualidade: compativel (MP3) ou melhor bitrate disponivel"
    )]
    quality: QualityArg,
    #[arg(
        long,
        help = "Template do nome do arquivo: {number} {artist} {title} {album} {year} {id} {genre} {uploader}"
    )]
    name_template: Option<String>,
    #[arg(long, help = "Pular faixas mais curtas que N segundos")]
    min_duration: Option<u32>,
    #[arg(
        long,
        help = "Pular faixas mais longas que N segundos (ex.: 900 pula mixes)"
    )]
    max_duration: Option<u32>,
    #[arg(long, help = "Nao gerar o arquivo .m3u8 da playlist")]
    no_m3u: bool,
    #[arg(long, help = "Capa na resolucao original enviada pelo artista")]
    original_artwork: bool,
    #[arg(
        long,
        help = "Mover para Removed/ as faixas que sairam da playlist (nunca apaga)"
    )]
    sync: bool,
    #[arg(
        long,
        help = "Nao reaproveitar faixas ja baixadas em outras pastas (biblioteca local)"
    )]
    no_library: bool,
    #[arg(long, help = "Limite de banda em kbit/s")]
    max_kbps: Option<u32>,
    #[arg(
        short,
        long,
        help = "Diretorio de destino (padrao: pasta de musicas do usuario/Perseus)"
    )]
    output: Option<PathBuf>,
    #[arg(short, long, help = "Monitorar a playlist e baixar faixas novas")]
    watch: bool,
    #[arg(short, long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(WATCH_MIN_INTERVAL..=WATCH_MAX_INTERVAL), help = "Intervalo de verificacao em segundos (--watch)")]
    interval: u64,
    #[arg(short, long, value_parser = clap::value_parser!(u64).range(1..=MAX_LIMIT as u64), help = "Baixar apenas as N primeiras faixas")]
    limit: Option<u64>,
    #[arg(long, default_value_t = DEFAULT_DOWNLOAD_WORKERS as u64, value_parser = clap::value_parser!(u64).range(1..=MAX_DOWNLOAD_WORKERS as u64), help = "Downloads simultaneos")]
    workers: u64,
    #[arg(long, help = "Apenas exibir metadados, sem baixar")]
    info: bool,
    #[arg(long, env = "SOUNDCLOUD_CLIENT_ID", value_parser = parse_client_id, hide_env_values = true, help = "client_id manual (padrao: descoberta automatica)")]
    client_id: Option<String>,
    #[arg(
        short,
        long,
        conflicts_with = "track_only",
        help = "Se a URL tiver ?in=, baixar a playlist inteira"
    )]
    playlist: bool,
    #[arg(long, help = "Se a URL tiver ?in=, baixar so a faixa")]
    track_only: bool,
    #[arg(short, long, help = "Logs em nivel DEBUG (inclui progresso por faixa)")]
    verbose: bool,
    #[arg(
        long,
        value_enum,
        default_value = "text",
        help = "Formato dos logs no terminal"
    )]
    log_format: LogFormat,
    #[arg(long, help = "Grava logs JSON neste arquivo")]
    log_file: Option<PathBuf>,
    #[arg(long, help = "Grava o relatorio final do download em JSON")]
    report_json: Option<PathBuf>,
}

fn parse_client_id(value: &str) -> Result<String, String> {
    if is_valid_client_id(value) {
        Ok(value.to_owned())
    } else {
        Err("client_id deve ter 32 caracteres alfanumericos".into())
    }
}

mod exit {
    pub const OK: u8 = 0;
    pub const PARTIAL: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const FAILURE: u8 = 3;
    pub const INTERRUPTED: u8 = 130;
}

fn init_logging(args: &Args) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let level = if args.verbose { "debug" } else { "info" };
    let filter = || {
        EnvFilter::new(format!(
            "warn,lofty=error,perseus_core={level},perseus_cli={level}"
        ))
    };
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false);
    let (file_layer, guard) = match &args.log_file {
        Some(path) => {
            let dir = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), PathBuf::from);
            let name = path.file_name().map_or_else(
                || "perseus.log".into(),
                |n| n.to_string_lossy().into_owned(),
            );
            let (writer, guard) =
                tracing_appender::non_blocking(tracing_appender::rolling::never(dir, name));
            (
                Some(
                    tracing_subscriber::fmt::layer()
                        .json()
                        .with_writer(writer)
                        .with_filter(filter()),
                ),
                Some(guard),
            )
        }
        None => (None, None),
    };
    let registry = tracing_subscriber::registry().with(file_layer);
    match args.log_format {
        LogFormat::Json => registry.with(stderr.json().with_filter(filter())).init(),
        LogFormat::Text => registry.with(stderr.compact().with_filter(filter())).init(),
    }
    guard
}

fn prompt(question: &str) -> Option<String> {
    eprint!("{question}");
    std::io::stderr().flush().ok()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok()?;
    Some(line.trim().to_owned())
}

fn choose_target(url: String, args: &Args) -> String {
    let Some(context) = playlist_context(&url) else {
        return url;
    };
    if args.track_only {
        return url;
    }
    if args.playlist || args.watch || !std::io::stdin().is_terminal() {
        tracing::info!("Usando a playlist de origem da URL: {context}");
        return context;
    }
    eprintln!("\nEsta faixa foi compartilhada a partir da playlist {context}");
    eprintln!("  [1] Baixar a playlist completa\n  [2] Baixar apenas esta faixa");
    if prompt("Escolha [1]: ").as_deref() == Some("2") {
        url
    } else {
        context
    }
}

fn download_options(args: &Args) -> DownloadOptions {
    DownloadOptions {
        quality: match args.quality {
            QualityArg::Compatible => Quality::Compatible,
            QualityArg::Best => Quality::Best,
        },
        name_template: args.name_template.clone(),
        min_duration_s: args.min_duration,
        max_duration_s: args.max_duration,
        write_playlist_file: !args.no_m3u,
        original_artwork: args.original_artwork,
        sync_removed: args.sync,
        use_library: !args.no_library,
        max_kbps: args.max_kbps,
    }
}

async fn search_url(client: &SoundCloudClient, query: &str) -> Result<String, Error> {
    let hits = client.search(query, SEARCH_RESULTS).await?;
    if hits.is_empty() {
        return Err(Error::NotFound(format!(
            "Nada encontrado para \"{query}\"."
        )));
    }
    println!("\nResultados para \"{query}\":");
    for (index, hit) in hits.iter().enumerate() {
        let kind = match hit.kind {
            "track" => "faixa",
            "album" => "album",
            "playlist" => "playlist",
            _ => "perfil",
        };
        println!(
            "{:>4}  [{kind:<8}] {} — {}",
            index + 1,
            truncate(&hit.title, 60),
            truncate(&hit.subtitle, 30)
        );
    }
    let chosen = if std::io::stdin().is_terminal() {
        prompt("Escolha [1]: ")
            .and_then(|answer| answer.parse::<usize>().ok())
            .filter(|n| (1..=hits.len()).contains(n))
            .unwrap_or(1)
    } else {
        1
    };
    Ok(hits[chosen - 1].url.clone())
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        format!("{}...", text.chars().take(max - 3).collect::<String>())
    }
}

fn render_inspect(result: &InspectResult) {
    let kind = match &result.resource {
        Resource::Track(_) => "Faixa",
        Resource::Playlist(_) => "Playlist",
        Resource::Collection(collection) => match collection.kind {
            CollectionKind::Albums => "Albuns",
            CollectionKind::Playlists => "Playlists",
        },
    };
    println!(
        "\n{kind}: {} ({})",
        result.resource.display_title(),
        result.resource.artist()
    );
    println!(
        "{:>4}  {:<50}  {:<24}  {:>8}  Status",
        "#", "Titulo", "Artista", "Duracao"
    );
    let mut group = None;
    for (index, preview) in result.preview.iter().enumerate() {
        if preview.group.is_some() && preview.group != group {
            group.clone_from(&preview.group);
            println!("\n  == {} ==", preview.group.as_deref().unwrap_or_default());
        }
        let track = &preview.track;
        let status = preview.unavailable_reason.as_deref().unwrap_or("OK");
        println!(
            "{:>4}  {:<50}  {:<24}  {:>8}  {status}",
            index + 1,
            truncate(&track.display_title(), 50),
            truncate(&track.artist(), 24),
            format_duration(track.duration),
        );
    }
    let remaining = result.total_tracks.saturating_sub(result.preview.len());
    if remaining > 0 {
        println!("... e mais {remaining} faixa(s).");
    }
}

fn render_reasons(title: &str, reasons: &std::collections::BTreeMap<i64, String>) {
    if reasons.is_empty() {
        return;
    }
    println!("\n{title}");
    for (track_id, reason) in reasons.iter().take(MAX_REASON_ROWS) {
        println!("  {track_id:>12}  {reason}");
    }
    if reasons.len() > MAX_REASON_ROWS {
        println!("  ... e mais {}", reasons.len() - MAX_REASON_ROWS);
    }
}

fn render_report(report: &DownloadReport) {
    if let Some(error) = &report.fatal_error {
        println!("\n[ERRO] {error}");
    }
    println!("\nResumo");
    let rows = [
        ("Baixadas", report.downloaded().count().to_string()),
        (
            "Ja existentes (reaproveitadas)",
            report.reused_count().to_string(),
        ),
        ("Indisponiveis", report.unavailable.len().to_string()),
        ("Falharam", report.failed.len().to_string()),
        ("Transferido", format_bytes(report.transferred_bytes())),
        ("Tempo total", format!("{:.1}s", report.elapsed_seconds)),
        (
            "Velocidade media",
            format!("{}/s", format_bytes(report.stats.bytes_per_second as u64)),
        ),
        ("Novas tentativas", report.stats.retries.to_string()),
        (
            "Retries de transporte",
            report.stats.transport_retries.to_string(),
        ),
        (
            "Copiadas da biblioteca local",
            report.stats.library_hits.to_string(),
        ),
        (
            "Movidas para Removed (sync)",
            report.stats.moved_removed.to_string(),
        ),
        (
            "Destino",
            report.target_dir.clone().unwrap_or_else(|| "-".into()),
        ),
        ("Run ID", report.run_id.clone()),
    ];
    for (label, value) in rows {
        println!("  {label:<32} {value}");
    }
    render_reasons("Faixas indisponiveis", &report.unavailable);
    render_reasons(
        "Faixas com falha (execute novamente para tentar de novo)",
        &report.failed,
    );
}

fn report_exit(report: &DownloadReport) -> u8 {
    if report.cancelled {
        exit::INTERRUPTED
    } else if report.fatal_error.is_some() {
        exit::FAILURE
    } else if report.ok() {
        exit::OK
    } else {
        exit::PARTIAL
    }
}

fn error_exit(err: &Error) -> u8 {
    match err.kind() {
        ErrorKind::InvalidInput => exit::USAGE,
        ErrorKind::Cancelled => exit::INTERRUPTED,
        _ => exit::FAILURE,
    }
}

async fn execute(args: Args, run_id: String, cancel: CancellationToken) -> Result<u8, Error> {
    let options = download_options(&args);
    options.validate()?;
    let client = Arc::new(SoundCloudClient::new(args.client_id.clone())?);
    let raw = match (&args.url, &args.search) {
        (Some(url), _) => url.clone(),
        (None, Some(query)) => search_url(&client, query).await?,
        (None, None) if std::io::stdin().is_terminal() => {
            prompt("URL do SoundCloud: ").unwrap_or_default()
        }
        (None, None) => return Err(Error::InvalidUrl("Informe a URL ou --search.".into())),
    };
    let url = choose_target(client.canonicalize(&raw).await?, &args);
    let output_dir = args.output.clone().unwrap_or_else(default_output_dir);
    let workers = usize::try_from(args.workers).unwrap_or(DEFAULT_DOWNLOAD_WORKERS);
    let reporter = Reporter::silent(run_id);

    if args.info {
        render_inspect(&inspect_url(&client, &url, INSPECT_PREVIEW).await?);
        return Ok(exit::OK);
    }

    if args.watch {
        let config = WatchConfig {
            url,
            output_dir,
            interval: args.interval,
            workers,
            options,
            ..WatchConfig::new("")
        };
        PlaylistWatcher::new(config, client, reporter, cancel.clone())?
            .run()
            .await?;
        return Ok(if cancel.is_cancelled() {
            exit::INTERRUPTED
        } else {
            exit::OK
        });
    }

    render_inspect(&inspect_url(&client, &url, INSPECT_PREVIEW).await?);
    let request = DownloadRequest {
        url,
        output_dir,
        limit: args.limit.and_then(|l| usize::try_from(l).ok()),
        only_track_ids: None,
        workers,
        options,
        ..DownloadRequest::new("")
    };
    let report = run_download(client, request, cancel, reporter).await;
    render_report(&report);
    if let Some(path) = &args.report_json {
        let json = serde_json::to_vec_pretty(&report.to_json())
            .map_err(|err| Error::InvalidInput(err.to_string()))?;
        atomic_write(path, &json)?;
        tracing::info!("Relatorio gravado em {}", path.display());
    }
    Ok(report_exit(&report))
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    let _guard = init_logging(&args);
    let run_id = new_run_id();
    tracing::info!(run_id, "Perseus {}", perseus_core::VERSION);

    let cancel = CancellationToken::new();
    let on_signal = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("\nInterrompido pelo usuario. Finalizando...");
            on_signal.cancel();
        }
    });

    let code = match execute(args, run_id.clone(), cancel).await {
        Ok(code) => code,
        Err(err) => {
            tracing::error!(run_id, "{err}");
            error_exit(&err)
        }
    };
    ExitCode::from(code)
}
