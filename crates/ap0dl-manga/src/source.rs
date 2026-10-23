use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Serialize;
use url::Url;

use ap0dl_core::error::Result;
use ap0dl_core::http::Http;
pub use ap0dl_core::provider::{ProviderStatus as SourceStatus, host_matches};

use crate::filters::{Filter, FilterValues};
use crate::models::{Chapter, Manga, MangasPage, Page};

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub lang: String,
    pub base_url: String,
    pub status: SourceStatus,
    pub supports_latest: bool,
    pub notes: String,
}

/// Async adapter for one manga site.
///
/// The method set follows Aniyomi's `CatalogueSource`/`HttpSource` contract:
/// browse (`popular`, `latest`), `search` with a declared filter list,
/// `details`, `chapters`, `pages`, and an optional per-page `image_url`
/// step. Like anime providers, sources never own the HTTP client; the
/// engine passes the shared [`Http`] so retries, pacing, cookies and the
/// Cloudflare fallback stay uniform.
#[async_trait]
pub trait MangaSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// ISO 639-1 language of the chapters this source returns.
    fn lang(&self) -> &str {
        "en"
    }
    fn base_url(&self) -> &str;
    fn status(&self) -> SourceStatus;
    fn notes(&self) -> &'static str {
        ""
    }

    /// Whether a URL belongs to this source.
    fn matches(&self, url: &Url) -> bool;

    /// Headers the image CDN requires (Aniyomi's `imageRequest` headers).
    /// Copied onto every [`Page`] the source returns.
    fn image_headers(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    /// Search filters this source understands (Aniyomi's `getFilterList`).
    fn filters(&self) -> Vec<Filter> {
        Vec::new()
    }

    fn supports_latest(&self) -> bool {
        false
    }

    async fn popular(&self, http: &Http, page: u32) -> Result<MangasPage>;

    /// Latest-updates feed. Sources without one return an empty page.
    async fn latest(&self, _http: &Http, _page: u32) -> Result<MangasPage> {
        Ok(MangasPage::default())
    }

    /// `filters` were validated against [`MangaSource::filters`].
    async fn search(
        &self,
        http: &Http,
        query: &str,
        page: u32,
        filters: &FilterValues,
    ) -> Result<MangasPage>;

    /// Fills in metadata for a manga returned by browse/search or a URL.
    async fn details(&self, http: &Http, url: &str) -> Result<Manga>;

    /// Chapters in any order; the engine sorts them ascending.
    async fn chapters(&self, http: &Http, manga: &Manga) -> Result<Vec<Chapter>>;

    async fn pages(&self, http: &Http, chapter: &Chapter) -> Result<Vec<Page>>;

    /// Resolves a page whose `image_url` was left empty by [`MangaSource::pages`].
    async fn image_url(&self, _http: &Http, page: &Page) -> Result<String> {
        Ok(page.image_url.clone())
    }

    fn info(&self) -> SourceInfo {
        SourceInfo {
            id: self.id().to_string(),
            name: self.name().to_string(),
            lang: self.lang().to_string(),
            base_url: self.base_url().to_string(),
            status: self.status(),
            supports_latest: self.supports_latest(),
            notes: self.notes().to_string(),
        }
    }
}
