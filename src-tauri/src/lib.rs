mod commands;
mod dto;
mod jobs;

use std::sync::Arc;

use perseus_core::SoundCloudClient;
use perseus_core::shared::config::log_dir;
use tauri::{Manager as _, RunEvent};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::jobs::JobManager;

const LOG_FILTER: &str = "warn,lofty=error,perseus_core=info,perseus_lib=info";
const MAX_LOG_FILES: usize = 7;

fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter =
        || EnvFilter::try_from_env("PERSEUS_LOG").unwrap_or_else(|_| EnvFilter::new(LOG_FILTER));
    let appender = log_dir().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("perseus")
            .filename_suffix("log")
            .max_log_files(MAX_LOG_FILES)
            .build(dir)
            .ok()
    });
    let (file_layer, guard) = match appender {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
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
    let console = cfg!(debug_assertions).then(|| {
        tracing_subscriber::fmt::layer()
            .compact()
            .with_filter(filter())
    });
    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(console)
        .try_init();
    guard
}

fn startup_failed(reason: &str) {
    tracing::error!(reason, "Perseus nao iniciou");
    let logs = log_dir().map_or_else(String::new, |dir| {
        format!("\n\nDetalhes em:\n{}", dir.display())
    });
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Perseus")
        .set_description(format!(
            "O Perseus nao conseguiu iniciar: {reason}.\n\nSe o problema persistir, reinstale o Perseus \
             (o instalador tambem instala o Microsoft WebView2).{logs}"
        ))
        .show();
}

pub fn run() {
    let log_guard = init_logging();
    tracing::info!(version = perseus_core::VERSION, "Iniciando Perseus");

    let client = match SoundCloudClient::new(None) {
        Ok(client) => Arc::new(client),
        Err(err) => {
            startup_failed(&err.to_string());
            return;
        }
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(JobManager::new(client))
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::inspect,
            commands::search,
            commands::list_jobs,
            commands::create_job,
            commands::cancel_job,
            commands::job_events,
            commands::open_folder,
            commands::pick_output_dir,
        ])
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                handle.state::<JobManager>().cancel_all();
                tracing::info!("Perseus encerrado");
            }
        }),
        Err(err) => startup_failed(&format!("falha ao abrir a janela ({err})")),
    }
    drop(log_guard);
}
