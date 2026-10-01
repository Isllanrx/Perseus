use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::shared::config::{CLIENT_ID_CACHE_TTL, MAX_PAGE_BYTES, MAX_SCRIPTS_TO_SCAN, cache_dir};
use crate::shared::error::{Error, Result};
use crate::shared::filesystem::atomic_write;
use crate::shared::format::mask_secret;
use crate::shared::http::{Http, ensure_success, read_limited};
use crate::shared::soundcloud::Endpoints;
use crate::shared::soundcloud::urls::is_valid_client_id;

static SCRIPT_SRC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<script[^>]+src=["'](https://[^"']+\.js)["']"#).expect("regex valida")
});
static CANDIDATE_RES: LazyLock<[Regex; 2]> = LazyLock::new(|| {
    [
        Regex::new(r#"client_id\s*[:=]\s*["']([A-Za-z0-9]{32})["']"#).expect("regex valida"),
        Regex::new(r"client_id=([A-Za-z0-9]{32})").expect("regex valida"),
    ]
});

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    client_id: String,
    timestamp: f64,
}

pub struct ClientIdProvider {
    http: Http,
    endpoints: Endpoints,
    cache_path: Option<PathBuf>,
    ttl: Duration,
    current: Mutex<Option<String>>,
}

impl ClientIdProvider {
    pub fn new(http: Http, endpoints: Endpoints, explicit_id: Option<String>) -> Result<Self> {
        if let Some(id) = &explicit_id
            && !is_valid_client_id(id)
        {
            return Err(Error::InvalidInput(
                "client_id deve ter 32 caracteres alfanumericos.".into(),
            ));
        }
        Ok(Self {
            http,
            endpoints,
            cache_path: cache_dir().map(|dir| dir.join("client_id.json")),
            ttl: CLIENT_ID_CACHE_TTL,
            current: Mutex::new(explicit_id),
        })
    }

    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn with_cache_path(mut self, path: Option<PathBuf>) -> Self {
        self.cache_path = path;
        self
    }

    pub async fn client_id(&self) -> Result<String> {
        let mut current = self.current.lock().await;
        if let Some(id) = current.as_ref() {
            return Ok(id.clone());
        }
        let id = self.load_or_discover().await?;
        *current = Some(id.clone());
        Ok(id)
    }

    pub async fn refresh(&self, stale: &str) -> Result<String> {
        let mut current = self.current.lock().await;
        if let Some(id) = current.as_ref()
            && (id != stale || self.verify(id).await?)
        {
            return Ok(id.clone());
        }
        tracing::warn!("client_id rejeitado pela API; obtendo um novo");
        let id = self.discover().await?;
        *current = Some(id.clone());
        Ok(id)
    }

    async fn load_or_discover(&self) -> Result<String> {
        if let Some(cached) = self.read_cache()
            && self.verify(&cached).await?
        {
            return Ok(cached);
        }
        self.discover().await
    }

    async fn discover(&self) -> Result<String> {
        tracing::info!("Obtendo client_id publico a partir de soundcloud.com");
        let html = self.fetch_text(&self.endpoints.web_base).await?;
        let scripts: Vec<String> = SCRIPT_SRC_RE
            .captures_iter(&html)
            .map(|caps| caps[1].to_owned())
            .filter(|url| self.http.is_trusted(url))
            .collect();

        let mut seen = Vec::new();
        for script in scripts.iter().rev().take(MAX_SCRIPTS_TO_SCAN) {
            let text = match self.fetch_text(script).await {
                Ok(text) => text,
                Err(err) => {
                    tracing::debug!(error = %err, "script ignorado");
                    continue;
                }
            };
            if let Some(id) = self.first_valid(&text, &mut seen).await? {
                return Ok(id);
            }
        }
        if let Some(id) = self.first_valid(&html, &mut seen).await? {
            return Ok(id);
        }
        Err(Error::ClientIdUnavailable(
            "Nao foi possivel obter um client_id valido do SoundCloud. Verifique a conexao ou informe um \
             manualmente com --client-id."
                .into(),
        ))
    }

    async fn first_valid(&self, text: &str, seen: &mut Vec<String>) -> Result<Option<String>> {
        for pattern in CANDIDATE_RES.iter() {
            for caps in pattern.captures_iter(text) {
                let candidate = caps[1].to_owned();
                if seen.contains(&candidate) {
                    continue;
                }
                seen.push(candidate.clone());
                if self.verify(&candidate).await? {
                    tracing::info!(client_id = %mask_secret(&candidate), "client_id validado");
                    self.write_cache(&candidate);
                    return Ok(Some(candidate));
                }
            }
        }
        Ok(None)
    }

    async fn verify(&self, candidate: &str) -> Result<bool> {
        let url = format!("{}/search/tracks", self.endpoints.api_base);
        let response = self
            .http
            .get(
                &url,
                &[("q", "a"), ("limit", "1"), ("client_id", candidate)],
            )
            .await
            .map_err(|err| {
                Error::api(format!("Falha de rede ao validar client_id: {err}"), None)
            })?;
        match response.status().as_u16() {
            200 => Ok(true),
            400 | 401 | 403 => Ok(false),
            status => Err(Error::api(
                format!("API indisponivel ao validar client_id (HTTP {status})"),
                Some(status),
            )),
        }
    }

    async fn fetch_text(&self, url: &str) -> Result<String> {
        let response = ensure_success(self.http.get(url, &[]).await?, "soundcloud.com")?;
        let body = read_limited(response, MAX_PAGE_BYTES, "pagina").await?;
        Ok(String::from_utf8_lossy(&body).into_owned())
    }

    fn read_cache(&self) -> Option<String> {
        let raw = std::fs::read(self.cache_path.as_ref()?).ok()?;
        let entry: CacheEntry = serde_json::from_slice(&raw).ok()?;
        let age = now_secs() - entry.timestamp;
        (is_valid_client_id(&entry.client_id) && (0.0..self.ttl.as_secs_f64()).contains(&age))
            .then_some(entry.client_id)
    }

    fn write_cache(&self, client_id: &str) {
        let Some(path) = &self.cache_path else { return };
        let entry = CacheEntry {
            client_id: client_id.to_owned(),
            timestamp: now_secs(),
        };
        let result = serde_json::to_vec(&entry)
            .map_err(|err| Error::InvalidInput(err.to_string()))
            .and_then(|json| atomic_write(path, &json));
        if let Err(err) = result {
            tracing::debug!(error = %err, "nao foi possivel gravar cache de client_id");
        }
    }
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}
