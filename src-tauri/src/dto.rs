use perseus_core::DownloadOptions;
use perseus_core::features::download::naming::validate_template;
use perseus_core::shared::config::{
    MAX_DOWNLOAD_WORKERS, MAX_LIMIT, MAX_URL_LENGTH, MIN_KBPS_LIMIT, WATCH_MAX_INTERVAL,
    WATCH_MIN_INTERVAL,
};
use perseus_core::shared::error::ErrorKind;
use perseus_core::shared::soundcloud::transcoding::Quality;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_OUTPUT_DIR_LENGTH: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Playlist,
    Track,
    Watch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Completed,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Debug, Serialize)]
pub struct ConfigOut {
    pub version: &'static str,
    pub default_output_dir: String,
    pub max_workers: usize,
    pub default_workers: usize,
    pub interval_min: u64,
    pub interval_max: u64,
}

#[derive(Debug, Serialize)]
pub struct TrackOut {
    pub id: i64,
    pub position: usize,
    pub title: String,
    pub artist: String,
    pub duration_ms: Option<i64>,
    pub artwork_url: Option<String>,
    pub permalink_url: Option<String>,
    pub available: bool,
    pub reason: Option<String>,
    pub reason_code: Option<&'static str>,
    pub group: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InspectOut {
    pub kind: &'static str,
    pub url: String,
    pub playlist_context: Option<String>,
    pub title: String,
    pub artist: String,
    pub artwork_url: Option<String>,
    pub permalink_url: Option<String>,
    pub total_tracks: usize,
    pub tracks: Vec<TrackOut>,
}

fn default_workers() -> usize {
    perseus_core::shared::config::DEFAULT_DOWNLOAD_WORKERS
}

fn default_interval() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "cada bool e uma opcao independente do usuario"
)]
pub struct JobIn {
    pub url: String,
    #[serde(default = "default_mode")]
    pub mode: Mode,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default = "default_workers")]
    pub workers: usize,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default)]
    pub quality: Quality,
    #[serde(default)]
    pub name_template: Option<String>,
    #[serde(default)]
    pub min_duration_s: Option<u32>,
    #[serde(default)]
    pub max_duration_s: Option<u32>,
    #[serde(default = "enabled")]
    pub write_playlist_file: bool,
    #[serde(default)]
    pub original_artwork: bool,
    #[serde(default)]
    pub sync_removed: bool,
    #[serde(default = "enabled")]
    pub use_library: bool,
    #[serde(default)]
    pub max_kbps: Option<u32>,
}

fn enabled() -> bool {
    true
}

fn default_mode() -> Mode {
    Mode::Playlist
}

impl JobIn {
    pub fn options(&self) -> DownloadOptions {
        DownloadOptions {
            quality: self.quality,
            name_template: self.name_template.clone(),
            min_duration_s: self.min_duration_s,
            max_duration_s: self.max_duration_s,
            write_playlist_file: self.write_playlist_file,
            original_artwork: self.original_artwork,
            sync_removed: self.sync_removed,
            use_library: self.use_library,
            max_kbps: self.max_kbps,
        }
    }

    pub fn validated(mut self) -> Result<Self, CommandError> {
        self.url = self.url.trim().to_owned();
        self.output_dir = self
            .output_dir
            .map(|dir| dir.trim().to_owned())
            .filter(|dir| !dir.is_empty());
        let invalid = |code: &'static str, message: String| {
            CommandError::of(CommandErrorKind::InvalidInput, code, message)
        };
        if self.url.is_empty() || self.url.len() > MAX_URL_LENGTH {
            return Err(invalid(
                "url_length",
                format!("A URL deve ter entre 1 e {MAX_URL_LENGTH} caracteres."),
            ));
        }
        if self
            .output_dir
            .as_ref()
            .is_some_and(|dir| dir.len() > MAX_OUTPUT_DIR_LENGTH)
        {
            return Err(invalid(
                "output_dir_too_long",
                format!("A pasta de destino excede {MAX_OUTPUT_DIR_LENGTH} caracteres."),
            ));
        }
        if self
            .output_dir
            .as_ref()
            .is_some_and(|dir| !std::path::Path::new(dir).is_absolute())
        {
            return Err(invalid(
                "output_dir_relative",
                "Informe o caminho completo da pasta de destino (ex.: C:\\Users\\voce\\Music)."
                    .into(),
            ));
        }
        if !(1..=MAX_DOWNLOAD_WORKERS).contains(&self.workers) {
            return Err(invalid(
                "workers_range",
                format!("Downloads simultaneos devem estar entre 1 e {MAX_DOWNLOAD_WORKERS}."),
            ));
        }
        if self
            .limit
            .is_some_and(|limit| !(1..=MAX_LIMIT).contains(&limit))
        {
            return Err(invalid(
                "limit_range",
                format!("O limite de faixas deve estar entre 1 e {MAX_LIMIT}."),
            ));
        }
        self.name_template = self
            .name_template
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        if let Some(template) = &self.name_template
            && let Err(err) = validate_template(template)
        {
            return Err(invalid("name_template_invalid", err.to_string()));
        }
        if let (Some(min), Some(max)) = (self.min_duration_s, self.max_duration_s)
            && min > max
        {
            return Err(invalid(
                "duration_range",
                "A duracao minima nao pode ser maior que a maxima.".into(),
            ));
        }
        if self.max_kbps.is_some_and(|kbps| kbps < MIN_KBPS_LIMIT) {
            return Err(invalid(
                "kbps_range",
                format!("O limite de banda deve ser de pelo menos {MIN_KBPS_LIMIT} kbit/s."),
            ));
        }
        if !(WATCH_MIN_INTERVAL..=WATCH_MAX_INTERVAL).contains(&self.interval) {
            return Err(invalid(
                "interval_range",
                format!(
                    "O intervalo deve estar entre {WATCH_MIN_INTERVAL} e {WATCH_MAX_INTERVAL} segundos."
                ),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct JobCounts {
    pub downloaded: usize,
    pub reused: usize,
    pub failed: usize,
    pub unavailable: usize,
    pub selected: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobOut {
    pub id: String,
    pub url: String,
    pub mode: Mode,
    pub state: JobState,
    pub title: Option<String>,
    pub target_dir: Option<String>,
    pub created_at: f64,
    pub finished_at: Option<f64>,
    pub error: Option<String>,
    pub error_code: Option<&'static str>,
    pub counts: JobCounts,
    pub report: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobEvent {
    pub job_id: String,
    pub id: u64,
    pub event: &'static str,
    pub data: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorKind {
    InvalidInput,
    NotFound,
    Upstream,
    Unavailable,
    Download,
    Cancelled,
    TooManyJobs,
    JobNotFound,
    Conflict,
    Internal,
}

impl From<ErrorKind> for CommandErrorKind {
    fn from(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::InvalidInput => Self::InvalidInput,
            ErrorKind::NotFound => Self::NotFound,
            ErrorKind::Upstream => Self::Upstream,
            ErrorKind::Unavailable => Self::Unavailable,
            ErrorKind::Download => Self::Download,
            ErrorKind::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub kind: CommandErrorKind,
    pub code: &'static str,
    pub message: String,
}

impl CommandError {
    pub fn of(kind: CommandErrorKind, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
        }
    }

    pub fn not_found() -> Self {
        Self::of(
            CommandErrorKind::JobNotFound,
            "job_not_found",
            "Tarefa nao encontrada.",
        )
    }
}

impl From<perseus_core::Error> for CommandError {
    fn from(err: perseus_core::Error) -> Self {
        Self::of(err.kind().into(), err.code(), err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn job(value: Value) -> Result<JobIn, CommandError> {
        serde_json::from_value::<JobIn>(value)
            .map_err(|err| {
                CommandError::of(CommandErrorKind::Internal, "internal", err.to_string())
            })?
            .validated()
    }

    #[test]
    fn applies_defaults_and_trims() {
        let spec = job(json!({"url": "  https://soundcloud.com/a/b  ", "output_dir": "   "}))
            .expect("valido");
        assert_eq!(spec.url, "https://soundcloud.com/a/b");
        assert_eq!(spec.mode, Mode::Playlist);
        assert_eq!(spec.output_dir, None);
        assert_eq!(spec.workers, 4);
    }

    #[test]
    fn rejects_out_of_range_and_unknown_fields() {
        assert!(job(json!({"url": ""})).is_err());
        assert!(job(json!({"url": "x", "workers": 0})).is_err());
        assert!(job(json!({"url": "x", "limit": 0})).is_err());
        assert!(job(json!({"url": "x", "interval": 5})).is_err());
        assert!(job(json!({"url": "x", "output_dir": "musicas"})).is_err());
        let absolute = std::env::temp_dir().display().to_string();
        assert!(job(json!({"url": "x", "output_dir": absolute})).is_ok());
        assert!(job(json!({"url": "x", "engine": "scrapy"})).is_err());
        let code = |value: Value| job(value).err().map(|e| e.code);
        assert_eq!(
            code(json!({"url": "x", "name_template": "{artist}"})),
            Some("name_template_invalid")
        );
        assert_eq!(
            code(json!({"url": "x", "min_duration_s": 600, "max_duration_s": 60})),
            Some("duration_range")
        );
        assert_eq!(code(json!({"url": "x", "max_kbps": 8})), Some("kbps_range"));
        let full = job(json!({"url": "x", "quality": "best", "name_template": " {title} [{id}] ", "use_library": false}))
            .expect("valido");
        let options = full.options();
        assert_eq!(options.quality, Quality::Best);
        assert_eq!(options.name_template.as_deref(), Some("{title} [{id}]"));
        assert!(!options.use_library);
        assert!(options.write_playlist_file, "padrao ligado");
    }

    #[test]
    fn serializes_error_kinds() {
        let err = serde_json::to_value(CommandError::from(perseus_core::Error::NotFound(
            "x".into(),
        )))
        .expect("json");
        assert_eq!(
            err,
            json!({"kind": "not_found", "code": "not_found", "message": "x"})
        );
        assert_eq!(
            serde_json::to_value(CommandError::not_found()).expect("json")["kind"],
            "job_not_found"
        );
    }
}
