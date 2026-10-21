use async_trait::async_trait;
use serde::Serialize;
use url::Url;

use crate::error::Result;
use crate::http::Http;
use crate::models::{Anime, Episode, SearchResult, Stream};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStatus {
    /// Live smoke test passed and the provider is expected to work.
    Stable,
    /// Implemented but depends on an external service or may break often.
    Experimental,
    /// Site is dead, geo-blocked or protected by an unsolvable challenge.
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub status: ProviderStatus,
    pub notes: String,
}

/// Async adapter for one anime site.
///
/// Providers never own the HTTP client; the engine passes the shared
/// [`Http`] value so retries, rate limiting and cookies apply uniformly.
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn base_url(&self) -> &str;
    fn status(&self) -> ProviderStatus;
    fn notes(&self) -> &'static str {
        ""
    }

    /// Whether a URL belongs to this provider.
    fn matches(&self, url: &Url) -> bool;

    async fn search(&self, http: &Http, query: &str, page: u32) -> Result<Vec<SearchResult>>;

    async fn details(&self, http: &Http, url: &str) -> Result<Anime>;

    async fn episodes(&self, http: &Http, anime: &Anime) -> Result<Vec<Episode>>;

    async fn streams(&self, http: &Http, anime: &Anime, episode: &Episode) -> Result<Vec<Stream>>;

    /// Optional latest-updates feed. Providers that do not support it return an
    /// empty list rather than an error.
    async fn latest(&self, _http: &Http, _page: u32) -> Result<Vec<SearchResult>> {
        Ok(Vec::new())
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id().to_string(),
            name: self.name().to_string(),
            base_url: self.base_url().to_string(),
            status: self.status(),
            notes: self.notes().to_string(),
        }
    }
}

pub fn host_matches(url: &Url, domain: &str) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let domain = domain.trim_start_matches("www.").to_ascii_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
}
