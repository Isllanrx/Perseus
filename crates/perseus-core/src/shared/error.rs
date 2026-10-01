use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidUrl(String),
    #[error("{0}")]
    UntrustedUrl(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    ClientIdUnavailable(String),
    #[error("{message}")]
    Api {
        message: String,
        status: Option<u16>,
    },
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    UnsupportedResource(String),
    #[error("{0}")]
    StreamUnavailable(String),
    #[error("{0}")]
    Transfer(String),
    #[error("{0}")]
    Download(String),
    #[error("{0}")]
    Integrity(String),
    #[error("falha de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "espaco insuficiente em disco: necessario ~{needed_mb} MB, disponivel {available_mb} MB"
    )]
    InsufficientSpace { needed_mb: u64, available_mb: u64 },
    #[error("operacao cancelada")]
    Cancelled,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    InvalidInput,
    NotFound,
    Upstream,
    Unavailable,
    Download,
    Cancelled,
}

impl Error {
    pub fn api(message: impl Into<String>, status: Option<u16>) -> Self {
        Self::Api {
            message: message.into(),
            status,
        }
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Api { status, .. } => status.is_none_or(|code| code == 429 || code >= 500),
            Self::Transfer(_) | Self::Integrity(_) => true,
            _ => false,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::InvalidUrl(_)
            | Self::UntrustedUrl(_)
            | Self::InvalidInput(_)
            | Self::UnsupportedResource(_) => ErrorKind::InvalidInput,
            Self::NotFound(_) => ErrorKind::NotFound,
            Self::ClientIdUnavailable(_) | Self::Api { .. } => ErrorKind::Upstream,
            Self::StreamUnavailable(_) => ErrorKind::Unavailable,
            Self::Transfer(_)
            | Self::Download(_)
            | Self::Integrity(_)
            | Self::Io(_)
            | Self::InsufficientSpace { .. } => ErrorKind::Download,
            Self::Cancelled => ErrorKind::Cancelled,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidUrl(_) => "invalid_url",
            Self::UntrustedUrl(_) => "untrusted_url",
            Self::InvalidInput(_) => "invalid_input",
            Self::ClientIdUnavailable(_) => "client_id_unavailable",
            Self::Api { .. } => "api",
            Self::NotFound(_) => "not_found",
            Self::UnsupportedResource(_) => "unsupported_resource",
            Self::StreamUnavailable(_) => "stream_unavailable",
            Self::Transfer(_) => "transfer",
            Self::Download(_) => "download",
            Self::Integrity(_) => "integrity",
            Self::Io(_) => "io",
            Self::InsufficientSpace { .. } => "insufficient_space",
            Self::Cancelled => "cancelled",
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        let reason = if err.is_timeout() {
            "tempo esgotado"
        } else if err.is_connect() {
            "falha de conexao"
        } else if err.is_body() || err.is_decode() {
            "resposta interrompida"
        } else if err.is_redirect() {
            "redirecionamentos demais"
        } else {
            "erro de rede"
        };
        Self::Transfer(reason.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_matches_status_policy() {
        assert!(Error::api("x", None).is_retryable());
        assert!(Error::api("x", Some(429)).is_retryable());
        assert!(Error::api("x", Some(503)).is_retryable());
        assert!(!Error::api("x", Some(403)).is_retryable());
        assert!(!Error::NotFound("x".into()).is_retryable());
        assert!(Error::Integrity("x".into()).is_retryable());
        assert!(!Error::Download("x".into()).is_retryable());
    }
}
