use std::path::PathBuf;
use std::time::Duration;

use directories::{ProjectDirs, UserDirs};

pub const APP_NAME: &str = "Perseus";

pub const API_BASE: &str = "https://api-v2.soundcloud.com";
pub const WEB_BASE: &str = "https://soundcloud.com";

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
pub const ACCEPT: &str = "application/json, text/javascript, */*; q=0.01";
pub const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9,pt-BR;q=0.8";

pub const WEB_HOSTS: [&str; 3] = ["soundcloud.com", "www.soundcloud.com", "m.soundcloud.com"];
pub const SHORTLINK_HOSTS: [&str; 1] = ["on.soundcloud.com"];
pub const TRUSTED_MEDIA_HOST_SUFFIXES: [&str; 3] =
    ["sndcdn.com", "soundcloud.com", "soundcloud.cloud"];

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);
pub const HTTP_RETRY_STATUS: [u16; 9] = [408, 425, 429, 500, 502, 503, 504, 522, 524];
pub const HTTP_MAX_RETRIES: u32 = 4;
pub const HTTP_BACKOFF_BASE: Duration = Duration::from_millis(800);
pub const HTTP_BACKOFF_CAP: Duration = Duration::from_secs(30);
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

pub const TRACK_BATCH_SIZE: usize = 50;
pub const TRACK_BATCH_CONCURRENCY: usize = 4;
pub const MAX_CONCURRENT_API_REQUESTS: usize = 8;
pub const PAGE_SIZE: usize = 200;
pub const MAX_PAGES: usize = 200;
pub const MAX_COLLECTION_PLAYLISTS: usize = 500;
pub const MAX_RELATED_TRACKS: usize = 200;
pub const MAX_SEARCH_QUERY: usize = 200;
pub const MAX_SEARCH_RESULTS: usize = 50;
pub const MAX_URL_LENGTH: usize = 2048;
pub const MAX_SHORTLINK_HOPS: usize = 5;
pub const MAX_SCRIPTS_TO_SCAN: usize = 12;

pub const MAX_AUDIO_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ARTWORK_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_PLAYLIST_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_API_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_PAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_HLS_SEGMENTS: usize = 10_000;
pub const HLS_SEGMENT_CONCURRENCY: usize = 6;
pub const MIN_AUDIO_BYTES: u64 = 16 * 1024;
pub const MIN_AUDIO_SECONDS: f64 = 3.0;

pub const CLIENT_ID_CACHE_TTL: Duration = Duration::from_secs(12 * 3600);

pub const DEFAULT_DOWNLOAD_WORKERS: usize = 4;
pub const MAX_DOWNLOAD_WORKERS: usize = 16;
pub const DOWNLOAD_ATTEMPTS: u32 = 3;
pub const DOWNLOAD_BACKOFF_BASE: Duration = Duration::from_secs(1);
pub const DOWNLOAD_BACKOFF_CAP: Duration = Duration::from_secs(30);
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub const MAX_LIMIT: usize = 10_000;
pub const MIN_KBPS_LIMIT: u32 = 64;
pub const ARCHIVE_FILE: &str = ".perseus-archive.json";
pub const REMOVED_FOLDER: &str = "Removed";

pub const WATCH_MIN_INTERVAL: u64 = 10;
pub const WATCH_MAX_INTERVAL: u64 = 3600;
pub const WATCH_MAX_BACKOFF: Duration = Duration::from_secs(600);
pub const WATCH_MAX_TRACK_ATTEMPTS: u32 = 3;

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", APP_NAME)
}

pub fn default_output_dir() -> PathBuf {
    UserDirs::new()
        .and_then(|dirs| {
            dirs.audio_dir()
                .map(PathBuf::from)
                .or_else(|| Some(dirs.home_dir().join("Music")))
        })
        .map_or_else(|| PathBuf::from("downloads"), |music| music.join(APP_NAME))
}

pub fn library_path() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.data_local_dir().join("library.json"))
}

pub fn cache_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.cache_dir().to_path_buf())
}

pub fn log_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.data_local_dir().join("logs"))
}
