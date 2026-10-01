use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Response, redirect};
use url::Url;

use crate::shared::config::{
    ACCEPT, ACCEPT_LANGUAGE, CONNECT_TIMEOUT, HTTP_BACKOFF_BASE, HTTP_BACKOFF_CAP,
    HTTP_MAX_RETRIES, HTTP_RETRY_STATUS, MAX_RETRY_AFTER, READ_TIMEOUT,
    TRUSTED_MEDIA_HOST_SUFFIXES, USER_AGENT, WEB_BASE,
};
use crate::shared::error::{Error, Result};
use crate::shared::retry::backoff_delay;

const MAX_REDIRECTS: usize = 5;

tokio::task_local! {
    static TRANSPORT_RETRIES: Arc<AtomicU32>;
}

pub fn count_transport_retries<F: Future>(
    counter: Arc<AtomicU32>,
    future: F,
) -> impl Future<Output = F::Output> {
    TRANSPORT_RETRIES.scope(counter, future)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPolicy {
    SoundCloud,
    #[cfg(any(test, feature = "test-util"))]
    Loopback,
}

impl HostPolicy {
    pub fn allows(self, url: &Url) -> bool {
        match self {
            Self::SoundCloud => is_trusted_url(url),
            #[cfg(any(test, feature = "test-util"))]
            Self::Loopback => is_trusted_url(url) || url.host_str() == Some("127.0.0.1"),
        }
    }
}

fn is_trusted_url(url: &Url) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(url::Host::Domain(host)) = url.host() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    TRUSTED_MEDIA_HOST_SUFFIXES.iter().any(|suffix| {
        host == *suffix
            || host
                .strip_suffix(suffix)
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

pub fn is_trusted_media_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|parsed| is_trusted_url(&parsed))
}

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    no_redirect: reqwest::Client,
    policy: HostPolicy,
}

impl Http {
    pub fn new() -> Result<Self> {
        Self::with_policy(HostPolicy::SoundCloud)
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn for_tests() -> Self {
        Self::with_policy(HostPolicy::Loopback).expect("cliente HTTP de teste")
    }

    fn with_policy(policy: HostPolicy) -> Result<Self> {
        let redirects = redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("redirecionamentos demais")
            } else if policy.allows(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        });
        Ok(Self {
            client: Self::builder(policy)
                .redirect(redirects)
                .build()
                .map_err(|err| Self::build_error(&err))?,
            no_redirect: Self::builder(policy)
                .redirect(redirect::Policy::none())
                .build()
                .map_err(|err| Self::build_error(&err))?,
            policy,
        })
    }

    fn builder(policy: HostPolicy) -> reqwest::ClientBuilder {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static(ACCEPT));
        headers.insert(
            header::ACCEPT_LANGUAGE,
            HeaderValue::from_static(ACCEPT_LANGUAGE),
        );
        headers.insert(header::ORIGIN, HeaderValue::from_static(WEB_BASE));
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://soundcloud.com/"),
        );
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .pool_max_idle_per_host(16)
            .https_only(policy == HostPolicy::SoundCloud)
    }

    fn build_error(err: &reqwest::Error) -> Error {
        Error::Transfer(format!("nao foi possivel iniciar o cliente HTTP: {err}"))
    }

    pub fn is_trusted(&self, url: &str) -> bool {
        Url::parse(url).is_ok_and(|parsed| self.policy.allows(&parsed))
    }

    pub fn ensure_trusted(&self, url: &str) -> Result<()> {
        if self.is_trusted(url) {
            return Ok(());
        }
        let host = Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned));
        Err(Error::UntrustedUrl(format!(
            "Host de midia nao confiavel recusado: {host:?}"
        )))
    }

    pub async fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Response> {
        self.get_with(&self.client, url, query, None).await
    }

    pub async fn get_from(&self, url: &str, from: u64) -> Result<Response> {
        self.get_with(&self.client, url, &[], Some(from)).await
    }

    pub async fn get_no_redirect(&self, url: &str) -> Result<Response> {
        self.get_with(&self.no_redirect, url, &[], None).await
    }

    async fn get_with(
        &self,
        client: &reqwest::Client,
        url: &str,
        query: &[(&str, &str)],
        range_from: Option<u64>,
    ) -> Result<Response> {
        let mut retries = 0;
        loop {
            let mut request = client.get(url).query(query);
            if let Some(from) = range_from.filter(|from| *from > 0) {
                request = request.header(header::RANGE, format!("bytes={from}-"));
            }
            let delay = match request.send().await {
                Ok(response)
                    if retries < HTTP_MAX_RETRIES
                        && HTTP_RETRY_STATUS.contains(&response.status().as_u16()) =>
                {
                    retry_after(&response).unwrap_or_else(|| {
                        backoff_delay(retries + 1, HTTP_BACKOFF_BASE, HTTP_BACKOFF_CAP)
                    })
                }
                Ok(response) => return Ok(response),
                Err(err)
                    if retries < HTTP_MAX_RETRIES && (err.is_connect() || err.is_timeout()) =>
                {
                    backoff_delay(retries + 1, HTTP_BACKOFF_BASE, HTTP_BACKOFF_CAP)
                }
                Err(err) => return Err(err.into()),
            };
            tracing::debug!(
                retry = retries + 1,
                delay_ms = delay.as_millis() as u64,
                "retry de transporte"
            );
            let _ = TRANSPORT_RETRIES.try_with(|count| count.fetch_add(1, Ordering::Relaxed));
            tokio::time::sleep(delay).await;
            retries += 1;
        }
    }
}

fn retry_after(response: &Response) -> Option<Duration> {
    let seconds: u64 = response
        .headers()
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

pub fn ensure_success(response: Response, what: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        Err(Error::Transfer(format!(
            "{what} respondeu HTTP {}",
            status.as_u16()
        )))
    }
}

pub async fn read_limited(mut response: Response, limit: u64, what: &str) -> Result<Vec<u8>> {
    let too_big = || Error::Download(format!("{what} excede o limite de {limit} bytes"));
    if response.content_length().is_some_and(|len| len > limit) {
        return Err(too_big());
    }
    let mut body = Vec::with_capacity(response.content_length().unwrap_or(0).min(limit) as usize);
    while let Some(chunk) = response.chunk().await? {
        if (body.len() + chunk.len()) as u64 > limit {
            return Err(too_big());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_media_hosts() {
        for url in [
            "https://cf-media.sndcdn.com/abc.mp3?Policy=x",
            "https://playback.media-streaming.soundcloud.cloud/x/aac_160k/y.m4s",
            "https://api-v2.soundcloud.com/media/x/stream/hls",
            "https://i1.sndcdn.com/artworks-000-t500x500.jpg",
            "https://SNDCDN.com/a.mp3",
        ] {
            assert!(is_trusted_media_url(url), "{url}");
        }
    }

    #[test]
    fn untrusted_media_hosts() {
        let http = Http::new().expect("cliente");
        for url in [
            "http://cf-media.sndcdn.com/abc.mp3",
            "https://sndcdn.com.evil.io/abc.mp3",
            "https://evilsndcdn.com/abc.mp3",
            "https://user@cf-media.sndcdn.com/abc.mp3",
            "https://127.0.0.1/abc.mp3",
            "file:///etc/passwd",
            "",
        ] {
            assert!(!is_trusted_media_url(url), "{url}");
            assert!(
                matches!(http.ensure_trusted(url), Err(Error::UntrustedUrl(_))),
                "{url}"
            );
        }
    }
}
