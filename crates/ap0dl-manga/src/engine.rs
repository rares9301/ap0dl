use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;

use ap0dl_core::Config;
use ap0dl_core::engine::fuzzy_score;
use ap0dl_core::error::{Error, Result};
use ap0dl_core::http::Http;

use crate::config::MangaConfig;
use crate::filters::FilterValues;
use crate::models::{Chapter, Manga, MangasPage, Page};
use crate::selection::sort_chapters;
use crate::source::{MangaSource, SourceInfo};
use crate::sources;

const FALLBACK_TIMEOUT: Duration = Duration::from_secs(10);

/// A resolved series: the source that owns it plus its metadata.
#[derive(Clone)]
pub struct ResolvedManga {
    pub source: Arc<dyn MangaSource>,
    pub manga: Manga,
}

pub type Hit = (Arc<dyn MangaSource>, Manga);

/// Front door for all manga source work. Mirrors the anime
/// [`ap0dl_core::Engine`]: one shared [`Http`], sources looked up by id,
/// name, `source:query` prefix or URL host, and concurrent search with local
/// fuzzy ranking.
pub struct MangaEngine {
    pub config: Config,
    pub manga: MangaConfig,
    pub http: Http,
    sources: Vec<Arc<dyn MangaSource>>,
}

impl MangaEngine {
    pub fn new(config: Config, manga: MangaConfig) -> Result<Self> {
        let http = Http::new(&config)?;
        let sources = sources::all(&manga)?;
        Ok(Self::with_sources(config, manga, http, sources))
    }

    /// Builds an engine over an explicit source list. Used by tests and by
    /// hosts that register extra sources.
    pub fn with_sources(
        config: Config,
        manga: MangaConfig,
        http: Http,
        sources: Vec<Arc<dyn MangaSource>>,
    ) -> Self {
        Self {
            config,
            manga,
            http,
            sources,
        }
    }

    pub fn sources(&self) -> &[Arc<dyn MangaSource>] {
        &self.sources
    }

    pub fn source_infos(&self) -> Vec<SourceInfo> {
        self.sources.iter().map(|source| source.info()).collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn MangaSource>> {
        let needle = normalize_name(id);
        self.sources
            .iter()
            .find(|source| {
                normalize_name(source.id()) == needle || normalize_name(source.name()) == needle
            })
            .cloned()
    }

    pub fn require(&self, id: &str) -> Result<Arc<dyn MangaSource>> {
        self.get(id).ok_or_else(|| {
            let known = self
                .sources
                .iter()
                .map(|source| source.id())
                .collect::<Vec<_>>()
                .join(", ");
            Error::config(format!("unknown manga source {id:?} (available: {known})"))
        })
    }

    pub fn by_url(&self, value: &str) -> Result<Arc<dyn MangaSource>> {
        let url =
            url::Url::parse(value.trim()).map_err(|error| Error::Url(value.to_string(), error))?;
        self.sources
            .iter()
            .find(|source| source.matches(&url))
            .cloned()
            .ok_or_else(|| Error::Unsupported(format!("no manga source handles {value}")))
    }

    /// Splits `mangadex:frieren` into the source and the query.
    pub fn split_prefix(&self, query: &str) -> (Option<Arc<dyn MangaSource>>, String) {
        if let Some((prefix, rest)) = query.split_once(':') {
            if !rest.trim().is_empty() && !rest.starts_with("//") {
                if let Some(source) = self.get(prefix) {
                    return (Some(source), rest.trim().to_string());
                }
            }
        }
        (None, query.trim().to_string())
    }

    /// Searches one source, or every source concurrently with results merged
    /// and ranked locally. Filters require an explicit source because each
    /// source declares its own.
    pub async fn search(
        &self,
        source: Option<&str>,
        query: &str,
        page: u32,
        limit: usize,
        filters: &FilterValues,
    ) -> Result<Vec<Hit>> {
        let (prefixed, query) = self.split_prefix(query);
        let explicit = match source {
            Some(id) => Some(self.require(id)?),
            None => prefixed,
        };

        if let Some(source) = explicit {
            let filters = filters.validate(source.id(), &source.filters())?;
            let mut results = source
                .search(&self.http, &query, page, &filters)
                .await?
                .mangas;
            if results.is_empty() {
                return Err(Error::NoResults(query));
            }
            if !query.is_empty() {
                rank(&query, &mut results);
            }
            return Ok(results
                .into_iter()
                .take(limit.max(1))
                .map(|manga| (source.clone(), manga))
                .collect());
        }

        if !filters.is_empty() {
            return Err(Error::config(
                "--filter needs a single source; pass --source or a source:query prefix",
            ));
        }
        if self.sources.is_empty() {
            return Err(Error::config("no manga sources are registered"));
        }

        let timeout = Duration::from_secs(self.config.timeout_secs.clamp(3, 15));
        let requests = self.sources.iter().cloned().map(|source| {
            let http = self.http.clone();
            let query = query.clone();
            async move {
                let empty = FilterValues::default();
                match tokio::time::timeout(timeout, source.search(&http, &query, page, &empty))
                    .await
                {
                    Ok(Ok(page)) => Some((source, page.mangas)),
                    Ok(Err(error)) => {
                        tracing::warn!(source = source.id(), %error, "manga search failed");
                        None
                    }
                    Err(_) => {
                        tracing::warn!(source = source.id(), "manga search timed out");
                        None
                    }
                }
            }
        });

        let mut hits: Vec<Hit> = Vec::new();
        for (source, mut mangas) in join_all(requests).await.into_iter().flatten() {
            rank(&query, &mut mangas);
            hits.extend(
                mangas
                    .into_iter()
                    .take(limit.max(1))
                    .map(|manga| (source.clone(), manga)),
            );
        }
        if hits.is_empty() {
            return Err(Error::NoResults(query));
        }
        let preferred = self.manga.default_source.clone();
        hits.sort_by(|(left_source, left), (right_source, right)| {
            score(&query, right)
                .partial_cmp(&score(&query, left))
                .unwrap_or(Ordering::Equal)
                // Extended titles ("X Anthology", "X (Doujinshi)") clamp to
                // the same score as "X", so the shorter title wins before the
                // default source does; the same series then lists once per
                // source, default source first.
                .then_with(|| left.title.len().cmp(&right.title.len()))
                .then_with(|| {
                    let left_preferred = left_source.id() == preferred;
                    let right_preferred = right_source.id() == preferred;
                    right_preferred.cmp(&left_preferred)
                })
                .then_with(|| left_source.id().cmp(right_source.id()))
        });
        hits.truncate(limit.max(1));
        Ok(hits)
    }

    pub async fn popular(&self, source: &str, page: u32) -> Result<MangasPage> {
        self.require(source)?.popular(&self.http, page).await
    }

    pub async fn latest(&self, source: &str, page: u32) -> Result<MangasPage> {
        let source = self.require(source)?;
        if !source.supports_latest() {
            return Err(Error::unsupported(format!(
                "{} has no latest-updates feed",
                source.name()
            )));
        }
        source.latest(&self.http, page).await
    }

    /// Resolves a query (`mangadex:frieren`, a URL, or a plain title) into a
    /// source + manga pair. `index` is one-based and wraps.
    pub async fn resolve(&self, query: &str, index: usize) -> Result<ResolvedManga> {
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::NoResults(String::new()));
        }
        if query.starts_with("http://") || query.starts_with("https://") {
            let source = self.by_url(query)?;
            let manga = source.details(&self.http, query).await?;
            return Ok(ResolvedManga { source, manga });
        }
        let hits = self
            .search(None, query, 1, 50, &FilterValues::default())
            .await?;
        let (source, manga) = hits[index.saturating_sub(1) % hits.len()].clone();
        self.resolve_hit(source, manga).await
    }

    /// Fetches full details for a selected hit, keeping the search metadata
    /// if the details page fails.
    pub async fn resolve_hit(
        &self,
        source: Arc<dyn MangaSource>,
        manga: Manga,
    ) -> Result<ResolvedManga> {
        let manga = match source.details(&self.http, &manga.url).await {
            Ok(details) => details,
            Err(error) => {
                tracing::warn!(source = source.id(), %error, "manga details failed; using search metadata");
                manga
            }
        };
        Ok(ResolvedManga { source, manga })
    }

    /// Chapters ascending, oneshots last.
    pub async fn chapters(&self, resolved: &ResolvedManga) -> Result<Vec<Chapter>> {
        let mut chapters = resolved
            .source
            .chapters(&self.http, &resolved.manga)
            .await?;
        sort_chapters(&mut chapters);
        Ok(chapters)
    }

    /// Page list with every `image_url` filled in.
    pub async fn pages(&self, resolved: &ResolvedManga, chapter: &Chapter) -> Result<Vec<Page>> {
        let mut pages = resolved.source.pages(&self.http, chapter).await?;
        let headers = resolved.source.image_headers();
        for page in &mut pages {
            if page.image_url.is_empty() {
                page.image_url = resolved.source.image_url(&self.http, page).await?;
            }
            if page.headers.is_empty() && !headers.is_empty() {
                page.headers = headers.clone();
            }
        }
        if pages.is_empty() {
            return Err(Error::provider(
                resolved.source.id(),
                format!("{} has no pages", chapter.label()),
            ));
        }
        Ok(pages)
    }

    /// When a source lists the right series but its pages fail, look for the
    /// same title and chapter number on another source (Aniyomi calls the
    /// manual version of this "migrate").
    pub async fn fallback_pages(
        &self,
        original: &Manga,
        current: &str,
        number: f32,
    ) -> Option<(ResolvedManga, Chapter, Vec<Page>)> {
        if number < 0.0 {
            return None;
        }
        let hits = tokio::time::timeout(
            FALLBACK_TIMEOUT,
            self.search(None, &original.title, 1, 10, &FilterValues::default()),
        )
        .await
        .ok()?
        .ok()?;
        for (source, manga) in hits {
            if source.id() == current || !same_series(original, &manga) {
                continue;
            }
            let Ok(Ok(resolved)) =
                tokio::time::timeout(FALLBACK_TIMEOUT, self.resolve_hit(source, manga)).await
            else {
                continue;
            };
            let Ok(Ok(chapters)) =
                tokio::time::timeout(FALLBACK_TIMEOUT, self.chapters(&resolved)).await
            else {
                continue;
            };
            let Some(chapter) = chapters
                .into_iter()
                .find(|chapter| (chapter.number - number).abs() < 0.001)
            else {
                continue;
            };
            if let Ok(Ok(pages)) =
                tokio::time::timeout(FALLBACK_TIMEOUT, self.pages(&resolved, &chapter)).await
            {
                return Some((resolved, chapter, pages));
            }
        }
        None
    }
}

/// Alternative-title matches rank just below primary-title matches. MangaDex
/// spin-offs and doujinshi often carry the parent series name as an alt
/// title, and they must not tie with the series itself.
const ALT_TITLE_WEIGHT: f64 = 0.97;

/// Best fuzzy score over the title and alternative titles, so a romaji query
/// still finds an entry listed under its English name.
pub fn score(query: &str, manga: &Manga) -> f64 {
    manga
        .alt_titles
        .iter()
        .map(|title| fuzzy_score(query, title) * ALT_TITLE_WEIGHT)
        .fold(fuzzy_score(query, &manga.title), f64::max)
}

/// Whether `candidate` (from another source) is the same series: its primary
/// title must exactly match the original's title or one of its alt titles.
pub fn same_series(original: &Manga, candidate: &Manga) -> bool {
    std::iter::once(&original.title)
        .chain(original.alt_titles.iter())
        .any(|title| fuzzy_score(title, &candidate.title) >= 0.999)
}

fn rank(query: &str, mangas: &mut [Manga]) {
    if query.trim().is_empty() {
        return;
    }
    mangas.sort_by(|a, b| {
        score(query, b)
            .partial_cmp(&score(query, a))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.title.len().cmp(&b.title.len()))
    });
}

fn normalize_name(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace([' ', ':', '_', '-'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manga(title: &str, alt: &[&str]) -> Manga {
        let mut manga = Manga::new(
            "test",
            title,
            title,
            format!("https://example.test/{title}"),
        );
        manga.alt_titles = alt.iter().map(|value| value.to_string()).collect();
        manga
    }

    #[test]
    fn alt_titles_count_towards_the_score() {
        let frieren = manga("Sousou no Frieren", &["Frieren: Beyond Journey's End"]);
        assert!(score("frieren beyond journeys end", &frieren) > 0.9);
    }

    #[test]
    fn primary_title_beats_alt_title_and_fallback_needs_primary_match() {
        let series = manga("Bocchi the Rock!", &[]);
        let spinoff = manga("Kessoku Band Diaries", &["Bocchi the Rock!"]);
        assert!(score("bocchi the rock", &series) > score("bocchi the rock", &spinoff));
        let mangadex = manga("Sousou no Frieren", &["Frieren: Beyond Journey's End"]);
        assert!(same_series(
            &mangadex,
            &manga("Frieren: Beyond Journey's End", &[])
        ));
        assert!(!same_series(&series, &spinoff));
    }

    #[test]
    fn ranking_puts_exact_series_before_doujinshi() {
        let mut results = vec![
            manga("Sousou no Frieren - Kudaranai Nichijou (Doujinshi)", &[]),
            manga("Sousou no Frieren", &[]),
            manga("Hokuto no Ken & Sousou no Frieren", &[]),
        ];
        rank("sousou no frieren", &mut results);
        assert_eq!(results[0].title, "Sousou no Frieren");
    }

    #[test]
    fn engine_resolves_prefixes_and_urls() {
        let engine = MangaEngine::new(Config::default(), MangaConfig::default()).unwrap();
        let (source, query) = engine.split_prefix("mangadex:bocchi the rock");
        assert_eq!(source.unwrap().id(), "mangadex");
        assert_eq!(query, "bocchi the rock");
        let (source, query) = engine.split_prefix("re:zero");
        assert!(source.is_none());
        assert_eq!(query, "re:zero");
        assert_eq!(
            engine
                .by_url("https://weebcentral.com/series/01J76/Bocchi")
                .unwrap()
                .id(),
            "weebcentral"
        );
        assert_eq!(engine.require("Weeb Central").unwrap().id(), "weebcentral");
        assert!(engine.require("nope").is_err());
    }
}
