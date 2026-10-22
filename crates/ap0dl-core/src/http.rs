use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use reqwest::{Client, Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;

use crate::config::Config;
use crate::error::{Error, Result};

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// Shared HTTP client with retry, rate limiting and a small TTL cache.
#[derive(Clone)]
pub struct Http {
    inner: Arc<Inner>,
}

struct Inner {
    client: Client,
    default_headers: HeaderMap,
    cache: RwLock<HashMap<String, CacheEntry>>,
    last_request: Mutex<HashMap<String, Instant>>,
    delay: Duration,
}

struct CacheEntry {
    inserted: Instant,
    body: String,
}

impl Http {
    pub fn new(config: &Config) -> Result<Self> {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));
        default_headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8",
            ),
        );
        default_headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));

        let client = Client::builder()
            .default_headers(default_headers.clone())
            .timeout(Duration::from_secs(config.timeout_secs.max(5)))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(10))
            .cookie_store(true)
            .gzip(true)
            .brotli(true)
            .build()
            .map_err(|error| Error::config(format!("failed to build HTTP client: {error}")))?;

        Ok(Self {
            inner: Arc::new(Inner {
                client,
                default_headers,
                cache: RwLock::new(HashMap::new()),
                last_request: Mutex::new(HashMap::new()),
                delay: Duration::from_millis(config.request_delay_ms),
            }),
        })
    }

    pub fn client(&self) -> &Client {
        &self.inner.client
    }

    pub fn headers(&self) -> HeaderMap {
        self.inner.default_headers.clone()
    }

    pub async fn get(&self, url: &str, headers: &HeaderMap) -> Result<Response> {
        let response = self
            .request_with_retry::<serde_json::Value>(reqwest::Method::GET, url, headers, None)
            .await?;
        Ok(response)
    }

    pub async fn get_text(&self, url: &str, headers: &HeaderMap) -> Result<String> {
        match self.get(url, headers).await {
            Ok(response) => {
                let response = ensure_success(response, url).await?;
                let text = response.text().await.map_err(|source| Error::Http {
                    url: url.to_string(),
                    source,
                })?;
                if crate::cloudflare::is_challenge_body(&text) {
                    tracing::debug!(%url, "trying native TLS impersonation after a 200 challenge body");
                    match crate::cloudflare::get_text(url, headers).await {
                        Ok(text) => Ok(text),
                        Err(fallback) => Err(Error::provider(
                            "cloudflare",
                            format!(
                                "reqwest returned a challenge body; native TLS fallback: {fallback}"
                            ),
                        )),
                    }
                } else {
                    Ok(text)
                }
            }
            Err(error @ Error::HttpStatus { status, .. })
                if crate::cloudflare::is_challenge_status(status) =>
            {
                tracing::debug!(%url, status, "trying native TLS impersonation after Cloudflare challenge");
                match crate::cloudflare::get_text(url, headers).await {
                    Ok(text) => Ok(text),
                    Err(fallback) => Err(Error::provider(
                        "cloudflare",
                        format!("{error}; native TLS fallback: {fallback}"),
                    )),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub async fn get_text_cached(
        &self,
        url: &str,
        headers: &HeaderMap,
        ttl: Duration,
    ) -> Result<String> {
        if let Some(value) = self.cache_get(url, ttl).await {
            return Ok(value);
        }
        let text = self.get_text(url, headers).await?;
        self.cache_put(url, &text).await;
        Ok(text)
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str, headers: &HeaderMap) -> Result<T> {
        let text = self.get_text(url, headers).await?;
        serde_json::from_str(&text).map_err(Error::Json)
    }

    pub async fn get_json_cached<T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &HeaderMap,
        ttl: Duration,
    ) -> Result<T> {
        let text = self.get_text_cached(url, headers, ttl).await?;
        serde_json::from_str(&text).map_err(Error::Json)
    }

    pub async fn get_bytes(&self, url: &str, headers: &HeaderMap) -> Result<Bytes> {
        match self.get(url, headers).await {
            Ok(response) => {
                let response = ensure_success(response, url).await?;
                response.bytes().await.map_err(|source| Error::Http {
                    url: url.to_string(),
                    source,
                })
            }
            Err(error @ Error::HttpStatus { status, .. })
                if crate::cloudflare::is_challenge_status(status) =>
            {
                tracing::debug!(%url, status, "trying native TLS impersonation after Cloudflare challenge");
                match crate::cloudflare::get_bytes(url, headers).await {
                    Ok(bytes) => Ok(bytes),
                    Err(fallback) => Err(Error::provider(
                        "cloudflare",
                        format!("{error}; native TLS fallback: {fallback}"),
                    )),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &HeaderMap,
        body: &B,
    ) -> Result<T> {
        let response = self
            .request_with_retry(reqwest::Method::POST, url, headers, Some(body))
            .await?;
        let response = ensure_success(response, url).await?;
        let text = response.text().await.map_err(|source| Error::Http {
            url: url.to_string(),
            source,
        })?;
        serde_json::from_str(&text).map_err(Error::Json)
    }

    /// POST with no body. Some SPA file indexes return their JSON only for a
    /// POST to the folder URL (AnimeToki's Cloudflare Worker index does this).
    pub async fn post_empty_json<T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &HeaderMap,
    ) -> Result<T> {
        let text = self.post_empty_text(url, headers).await?;
        serde_json::from_str(&text).map_err(Error::Json)
    }

    pub async fn post_empty_text(&self, url: &str, headers: &HeaderMap) -> Result<String> {
        let response = self
            .request_with_retry::<serde_json::Value>(reqwest::Method::POST, url, headers, None)
            .await?;
        let response = ensure_success(response, url).await?;
        response.text().await.map_err(|source| Error::Http {
            url: url.to_string(),
            source,
        })
    }

    async fn request_with_retry<B: Serialize>(
        &self,
        method: reqwest::Method,
        url: &str,
        headers: &HeaderMap,
        body: Option<&B>,
    ) -> Result<Response> {
        self.throttle(url).await;

        let mut last_error = None;
        for attempt in 0..3 {
            let mut request = self
                .inner
                .client
                .request(method.clone(), url)
                .headers(self.merge_headers(headers));

            if let Some(body) = body {
                request = request.json(body);
            }

            match request.send().await {
                Ok(response)
                    if response.status().is_success()
                        || response.status() == StatusCode::NOT_MODIFIED =>
                {
                    return Ok(response);
                }
                Ok(response)
                    if response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    last_error = Some(Error::HttpStatus {
                        status: response.status().as_u16(),
                        url: url.to_string(),
                    });
                }
                Ok(response) => {
                    let status = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();
                    let preview = body.chars().take(240).collect::<String>();
                    return Err(Error::HttpStatus {
                        status,
                        url: format!("{url} ({preview})"),
                    });
                }
                Err(error) => {
                    last_error = Some(Error::Http {
                        url: url.to_string(),
                        source: error,
                    });
                }
            }

            if attempt < 2 {
                sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
            }
        }

        Err(last_error.unwrap_or_else(|| Error::HttpStatus {
            status: 0,
            url: url.to_string(),
        }))
    }

    fn merge_headers(&self, headers: &HeaderMap) -> HeaderMap {
        let mut merged = self.inner.default_headers.clone();
        for (key, value) in headers {
            merged.insert(key.clone(), value.clone());
        }
        merged
    }

    async fn throttle(&self, url: &str) {
        let host = url::Url::parse(url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_default();
        if host.is_empty() || self.inner.delay.is_zero() {
            return;
        }

        let mut last = self.inner.last_request.lock().await;
        if let Some(previous) = last.get(&host) {
            let elapsed = previous.elapsed();
            if elapsed < self.inner.delay {
                sleep(self.inner.delay - elapsed).await;
            }
        }
        last.insert(host, Instant::now());
    }

    async fn cache_get(&self, url: &str, ttl: Duration) -> Option<String> {
        let cache = self.inner.cache.read().await;
        cache.get(url).and_then(|entry| {
            if entry.inserted.elapsed() <= ttl {
                Some(entry.body.clone())
            } else {
                None
            }
        })
    }

    async fn cache_put(&self, url: &str, body: &str) {
        let mut cache = self.inner.cache.write().await;
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(
            url.to_string(),
            CacheEntry {
                inserted: Instant::now(),
                body: body.to_string(),
            },
        );
    }
}

async fn ensure_success(response: Response, url: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        let status = status.as_u16();
        let body = response.text().await.unwrap_or_default();
        let preview = body.chars().take(240).collect::<String>();
        Err(Error::HttpStatus {
            status,
            url: format!("{url} ({preview})"),
        })
    }
}

/// Builds a header map from string pairs, which keeps provider code readable.
pub fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            map.insert(name, value);
        }
    }
    map
}
