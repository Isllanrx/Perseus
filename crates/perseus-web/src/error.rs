use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use perseus_core::shared::error::ErrorKind;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiError {
    #[serde(skip)]
    status: StatusCode,
    kind: &'static str,
    code: &'static str,
    message: String,
    #[serde(skip)]
    retry_after_s: Option<u64>,
}

impl ApiError {
    pub fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            kind: "invalid_input",
            code,
            message: message.into(),
            retry_after_s: None,
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            kind: "not_found",
            code: "route_not_found",
            message: "Rota inexistente.".into(),
            retry_after_s: None,
        }
    }

    pub fn rate_limited(retry_after_s: u64) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            kind: "rate_limited",
            code: "rate_limited",
            message: "Muitas requisicoes; aguarde alguns segundos.".into(),
            retry_after_s: Some(retry_after_s),
        }
    }
}

impl From<perseus_core::Error> for ApiError {
    fn from(err: perseus_core::Error) -> Self {
        let (status, kind) = match err.kind() {
            ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_input"),
            ErrorKind::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ErrorKind::Upstream => (StatusCode::BAD_GATEWAY, "upstream"),
            ErrorKind::Unavailable => (StatusCode::UNPROCESSABLE_ENTITY, "unavailable"),
            ErrorKind::Download => (StatusCode::BAD_GATEWAY, "download"),
            ErrorKind::Cancelled => (StatusCode::SERVICE_UNAVAILABLE, "cancelled"),
        };
        if status.is_server_error() {
            tracing::warn!(code = err.code(), error = %err, "falha ao atender requisicao");
        }
        Self {
            status,
            kind,
            code: err.code(),
            message: err.to_string(),
            retry_after_s: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(&self)).into_response();
        let headers = response.headers_mut();
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if let Some(seconds) = self.retry_after_s {
            headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}
