use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Value, json};

use crate::features::download::naming::{FileNaming, validate_template};
use crate::shared::config::{
    DEFAULT_DOWNLOAD_WORKERS, MAX_DOWNLOAD_WORKERS, MAX_LIMIT, MIN_KBPS_LIMIT, default_output_dir,
    library_path,
};
use crate::shared::error::{Error, Result};
use crate::shared::soundcloud::transcoding::{AudioFormat, Quality};

#[allow(
    clippy::struct_excessive_bools,
    reason = "cada bool e uma opcao independente exposta ao usuario"
)]
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub quality: Quality,
    pub name_template: Option<String>,
    pub min_duration_s: Option<u32>,
    pub max_duration_s: Option<u32>,
    pub write_playlist_file: bool,
    pub original_artwork: bool,
    pub sync_removed: bool,
    pub use_library: bool,
    pub max_kbps: Option<u32>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            quality: Quality::Compatible,
            name_template: None,
            min_duration_s: None,
            max_duration_s: None,
            write_playlist_file: true,
            original_artwork: false,
            sync_removed: false,
            use_library: true,
            max_kbps: None,
        }
    }
}

impl DownloadOptions {
    pub fn validate(&self) -> Result<()> {
        if let Some(template) = &self.name_template {
            validate_template(template)?;
        }
        if let (Some(min), Some(max)) = (self.min_duration_s, self.max_duration_s)
            && min > max
        {
            return Err(Error::InvalidInput(
                "a duracao minima nao pode ser maior que a maxima".into(),
            ));
        }
        if self.max_kbps.is_some_and(|kbps| kbps < MIN_KBPS_LIMIT) {
            return Err(Error::InvalidInput(format!(
                "o limite de banda deve ser de pelo menos {MIN_KBPS_LIMIT} kbit/s"
            )));
        }
        Ok(())
    }

    pub fn accepts_duration(&self, duration_ms: Option<i64>) -> bool {
        let Some(seconds) = duration_ms.map(|ms| ms / 1000) else {
            return true;
        };
        self.min_duration_s
            .is_none_or(|min| seconds >= i64::from(min))
            && self
                .max_duration_s
                .is_none_or(|max| seconds <= i64::from(max))
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub output_dir: PathBuf,
    pub limit: Option<usize>,
    pub only_track_ids: Option<HashSet<i64>>,
    pub workers: usize,
    pub options: DownloadOptions,
    pub library_path: Option<PathBuf>,
}

impl DownloadRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            output_dir: default_output_dir(),
            limit: None,
            only_track_ids: None,
            workers: DEFAULT_DOWNLOAD_WORKERS,
            options: DownloadOptions::default(),
            library_path: library_path(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self
            .limit
            .is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit))
        {
            return Err(Error::InvalidInput(format!(
                "limit deve estar entre 1 e {MAX_LIMIT}"
            )));
        }
        if !(1..=MAX_DOWNLOAD_WORKERS).contains(&self.workers) {
            return Err(Error::InvalidInput(format!(
                "workers deve estar entre 1 e {MAX_DOWNLOAD_WORKERS}"
            )));
        }
        self.options.validate()
    }

    pub fn syncs_removals(&self) -> bool {
        self.options.sync_removed && self.limit.is_none() && self.only_track_ids.is_none()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrackTags {
    pub artist: Option<String>,
    pub genre: Option<String>,
    pub isrc: Option<String>,
    pub label: Option<String>,
    pub composer: Option<String>,
    pub copyright: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: usize,
    pub total_tracks: usize,
    pub duration_ms: Option<i64>,
    pub permalink_url: Option<String>,
    pub artwork_url: Option<String>,
    pub artwork_fallback_url: Option<String>,
    pub transcoding_url: String,
    pub protocol: String,
    pub format: AudioFormat,
    pub estimated_kbps: u32,
    pub track_authorization: Option<String>,
    pub output_dir: PathBuf,
    pub naming: FileNaming,
    pub tags: TrackTags,
}

#[cfg(test)]
impl DownloadJob {
    pub fn for_tests(dir: &std::path::Path, format: AudioFormat) -> Self {
        Self {
            track_id: 1,
            title: "Song".into(),
            artist: "Band".into(),
            album: "Album".into(),
            track_number: 1,
            total_tracks: 10,
            duration_ms: Some(180_000),
            permalink_url: Some("https://soundcloud.com/band/song".into()),
            artwork_url: None,
            artwork_fallback_url: None,
            transcoding_url: "https://api-v2.soundcloud.com/t".into(),
            protocol: "progressive".into(),
            format,
            estimated_kbps: 128,
            track_authorization: None,
            output_dir: dir.to_path_buf(),
            naming: FileNaming::Numbered {
                number: 1,
                total: 10,
                display: "Band - Song".into(),
            },
            tags: TrackTags::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletedDownload {
    pub track_id: i64,
    pub title: String,
    pub path: String,
    pub size_bytes: u64,
    pub reused: bool,
    #[serde(default)]
    pub from_library: bool,
    pub attempts: u32,
    pub elapsed_seconds: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DownloadStats {
    pub selected: usize,
    pub scheduled: usize,
    pub download_attempts: u32,
    pub retries: u32,
    pub transport_retries: u32,
    pub library_hits: usize,
    pub moved_removed: usize,
    pub bytes_per_second: f64,
}

impl DownloadStats {
    pub fn new(
        selected: usize,
        scheduled: usize,
        completed: &[CompletedDownload],
        elapsed_seconds: f64,
    ) -> Self {
        let transferred = completed.iter().filter(|c| !c.reused);
        let download_attempts: u32 = transferred.clone().map(|c| c.attempts).sum();
        let downloaded = u32::try_from(transferred.clone().count()).unwrap_or(u32::MAX);
        let bytes: u64 = transferred.map(|c| c.size_bytes).sum();
        Self {
            selected,
            scheduled,
            download_attempts,
            retries: download_attempts.saturating_sub(downloaded),
            transport_retries: 0,
            library_hits: completed.iter().filter(|c| c.from_library).count(),
            moved_removed: 0,
            bytes_per_second: if elapsed_seconds > 0.0 {
                (bytes as f64 / elapsed_seconds).round()
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DownloadReport {
    pub completed: Vec<CompletedDownload>,
    pub failed: BTreeMap<i64, String>,
    pub unavailable: BTreeMap<i64, String>,
    pub target_dir: Option<String>,
    pub fatal_error: Option<String>,
    pub fatal_error_code: Option<&'static str>,
    pub cancelled: bool,
    pub elapsed_seconds: f64,
    pub run_id: String,
    pub stats: DownloadStats,
}

impl DownloadReport {
    pub fn downloaded(&self) -> impl Iterator<Item = &CompletedDownload> {
        self.completed.iter().filter(|item| !item.reused)
    }

    pub fn reused_count(&self) -> usize {
        self.completed.iter().filter(|item| item.reused).count()
    }

    pub fn transferred_bytes(&self) -> u64 {
        self.downloaded().map(|item| item.size_bytes).sum()
    }

    pub fn settled_ids(&self) -> HashSet<i64> {
        self.completed
            .iter()
            .map(|item| item.track_id)
            .chain(self.unavailable.keys().copied())
            .collect()
    }

    pub fn ok(&self) -> bool {
        self.fatal_error.is_none() && self.failed.is_empty() && !self.cancelled
    }

    pub fn to_json(&self) -> Value {
        let mut data = serde_json::to_value(self).unwrap_or_else(|_| json!({}));
        data["summary"] = json!({
            "downloaded": self.downloaded().count(),
            "reused": self.reused_count(),
            "failed": self.failed.len(),
            "unavailable": self.unavailable.len(),
            "transferred_bytes": self.transferred_bytes(),
            "ok": self.ok(),
        });
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed(track_id: i64, reused: bool) -> CompletedDownload {
        CompletedDownload {
            track_id,
            title: "t".into(),
            path: "p".into(),
            size_bytes: 10,
            reused,
            from_library: false,
            attempts: 1,
            elapsed_seconds: 0.1,
        }
    }

    #[test]
    fn report_rules() {
        let mut report = DownloadReport {
            completed: vec![completed(1, false), completed(2, true)],
            ..Default::default()
        };
        report.unavailable.insert(3, "DRM".into());
        assert!(report.ok());
        assert_eq!(report.transferred_bytes(), 10);
        assert_eq!(report.settled_ids(), HashSet::from([1, 2, 3]));
        report.failed.insert(4, "timeout".into());
        assert!(!report.ok());
        let summary = &report.to_json()["summary"];
        assert_eq!(summary["downloaded"], 1);
        assert_eq!(summary["ok"], false);
    }

    #[test]
    fn validates_limits() {
        let mut request = DownloadRequest::new("https://soundcloud.com/a/b");
        assert!(request.validate().is_ok());
        request.workers = 0;
        assert!(request.validate().is_err());
        request.workers = 4;
        request.limit = Some(0);
        assert!(request.validate().is_err());
        request.limit = None;
        request.options.min_duration_s = Some(600);
        request.options.max_duration_s = Some(60);
        assert!(request.validate().is_err());
        request.options.max_duration_s = None;
        request.options.max_kbps = Some(8);
        assert!(request.validate().is_err());
        request.options.max_kbps = None;
        request.options.name_template = Some("{title} {nao_existe}".into());
        assert!(request.validate().is_err());
    }

    #[test]
    fn duration_filter() {
        let options = DownloadOptions {
            min_duration_s: Some(60),
            max_duration_s: Some(600),
            ..DownloadOptions::default()
        };
        assert!(options.accepts_duration(Some(120_000)));
        assert!(!options.accepts_duration(Some(30_000)));
        assert!(!options.accepts_duration(Some(3_600_000)));
        assert!(
            options.accepts_duration(None),
            "sem duracao conhecida nao filtra"
        );
    }
}
