use std::sync::Arc;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::http::Http;
use crate::models::{Anime, Episode, SearchResult, Stream};
use crate::provider::{Provider, ProviderInfo};
use crate::providers;

/// A resolved show: the provider that owns it plus its metadata.
#[derive(Clone)]
pub struct Resolved {
    pub provider: Arc<dyn Provider>,
    pub anime: Anime,
}

/// Front door for all provider work.
pub struct Engine {
    pub config: Config,
    pub http: Http,
    providers: Vec<Arc<dyn Provider>>,
}

impl Engine {
    pub fn new(config: Config) -> Result<Self> {
        let http = Http::new(&config)?;
        let providers = providers::all(&config)?;
        Ok(Self {
            config,
            http,
            providers,
        })
    }

    pub fn providers(&self) -> &[Arc<dyn Provider>] {
        &self.providers
    }

    pub fn provider_infos(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|provider| provider.info())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Provider>> {
        let needle = normalize_provider_name(id);
        self.providers
            .iter()
            .find(|provider| {
                normalize_provider_name(provider.id()) == needle
                    || normalize_provider_name(provider.name()) == needle
            })
            .cloned()
    }

    pub fn by_url(&self, value: &str) -> Result<Arc<dyn Provider>> {
        let url = parse_url(value)?;
        self.providers
            .iter()
            .find(|provider| provider.matches(&url))
            .cloned()
            .ok_or_else(|| Error::Unsupported(format!("no provider handles {value}")))
    }

    /// Searches one provider, or the default provider with a fallback across
    /// all providers when no explicit provider was requested.
    pub async fn search(
        &self,
        provider: Option<&str>,
        query: &str,
        page: u32,
        limit: usize,
    ) -> Result<Vec<(Arc<dyn Provider>, SearchResult)>> {
        let explicit =
            provider.or_else(|| parse_provider_prefix(query, &self.providers).map(|p| p.id()));
        let query = strip_provider_prefix(query, &self.providers);

        if let Some(id) = explicit {
            let provider = self
                .get(id)
                .ok_or_else(|| Error::config(format!("unknown provider {id:?}")))?;
            let mut results = provider.search(&self.http, &query, page).await?;
            if results.is_empty() {
                return Err(Error::NoResults(query));
            }
            rank_results(&query, &mut results);
            return Ok(limit_results(provider, results, limit));
        }

        let default = self
            .get(&self.config.default_provider)
            .or_else(|| self.providers.first().cloned())
            .ok_or_else(|| Error::config("no providers are registered"))?;

        let mut collected = Vec::new();
        match default.search(&self.http, &query, page).await {
            Ok(mut results) => {
                rank_results(&query, &mut results);
                collected.extend(limit_results(default.clone(), results, limit));
            }
            Err(error) => {
                tracing::warn!(provider = default.id(), %error, "default provider search failed");
            }
        }

        if collected.is_empty() {
            for provider in &self.providers {
                if provider.id() == default.id() {
                    continue;
                }
                match provider.search(&self.http, &query, page).await {
                    Ok(mut results) if !results.is_empty() => {
                        rank_results(&query, &mut results);
                        collected.extend(limit_results(provider.clone(), results, limit));
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::debug!(provider = provider.id(), %error, "fallback search failed");
                    }
                }
            }
        }

        if collected.is_empty() {
            return Err(Error::NoResults(query));
        }
        Ok(collected)
    }

    /// Resolves a user query (`reanime:One Piece`, a URL, or a plain title)
    /// into a provider + anime pair. `index` is one-based.
    pub async fn resolve(&self, query: &str, index: usize) -> Result<Resolved> {
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::NoResults(String::new()));
        }

        if looks_like_url(query) || self.by_url(query).is_ok() {
            let provider = self.by_url(query)?;
            let url = query.to_string();
            let anime = provider.details(&self.http, &url).await?;
            return Ok(Resolved { provider, anime });
        }

        let (provider, search_query) = match parse_provider_prefix_raw(query, &self.providers) {
            Some((provider, rest)) => (Some(provider), rest),
            None => (None, query.to_string()),
        };
        let results = self
            .search(provider.map(|provider| provider.id()), &search_query, 1, 50)
            .await
            .map_err(|error| match error {
                Error::NoResults(_) => Error::NoResults(search_query.clone()),
                other => other,
            })?;

        let Some((provider, result)) = nth_result(&results, index).cloned() else {
            return Err(Error::NoResults(search_query));
        };
        let anime = match provider.details(&self.http, &result.url).await {
            Ok(anime) => anime,
            Err(error) => {
                tracing::warn!(provider = provider.id(), %error, "details failed; using search metadata");
                result.into_anime()
            }
        };
        Ok(Resolved { provider, anime })
    }

    pub async fn episodes(&self, resolved: &Resolved) -> Result<Vec<Episode>> {
        let mut episodes = resolved
            .provider
            .episodes(&self.http, &resolved.anime)
            .await?;
        episodes.sort_by(|a, b| a.number.total_cmp(&b.number));
        Ok(episodes)
    }

    pub async fn streams(&self, resolved: &Resolved, episode: &Episode) -> Result<Vec<Stream>> {
        resolved
            .provider
            .streams(&self.http, &resolved.anime, episode)
            .await
    }
}

/// Selects a one-based result without dividing by zero on an empty list.
fn nth_result<T>(results: &[T], index: usize) -> Option<&T> {
    if results.is_empty() {
        None
    } else {
        Some(&results[index.saturating_sub(1) % results.len()])
    }
}

/// Makes provider result ordering predictable: exact title first, then
/// prefix matches, then substring matches, then everything else. This is
/// especially important for sites whose search endpoint returns specials
/// before the main series.
fn rank_results(query: &str, results: &mut [SearchResult]) {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return;
    }
    results.sort_by(|a, b| {
        let key = |result: &SearchResult| {
            let title = result.title.to_ascii_lowercase();
            let rank = if title == needle {
                0u8
            } else if title.starts_with(&needle) {
                1
            } else if title.contains(&needle) {
                2
            } else {
                3
            };
            (rank, title.len(), title)
        };
        key(a).cmp(&key(b))
    });
}

fn limit_results(
    provider: Arc<dyn Provider>,
    results: Vec<SearchResult>,
    limit: usize,
) -> Vec<(Arc<dyn Provider>, SearchResult)> {
    results
        .into_iter()
        .take(limit.max(1))
        .map(|result| (provider.clone(), result))
        .collect()
}

fn normalize_provider_name(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace([' ', ':', '_', '-'], "")
}

fn parse_provider_prefix<'a>(
    query: &'a str,
    providers: &'a [Arc<dyn Provider>],
) -> Option<&'a Arc<dyn Provider>> {
    parse_provider_prefix_raw(query, providers).map(|(provider, _)| provider)
}

fn parse_provider_prefix_raw<'a>(
    query: &str,
    providers: &'a [Arc<dyn Provider>],
) -> Option<(&'a Arc<dyn Provider>, String)> {
    let (prefix, rest) = query.split_once(':')?;
    if rest.trim().is_empty() {
        return None;
    }
    let needle = normalize_provider_name(prefix);
    providers
        .iter()
        .find(|provider| {
            normalize_provider_name(provider.id()) == needle
                || normalize_provider_name(provider.name()) == needle
        })
        .map(|provider| (provider, rest.trim().to_string()))
}

fn strip_provider_prefix(query: &str, providers: &[Arc<dyn Provider>]) -> String {
    parse_provider_prefix_raw(query, providers)
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| query.to_string())
}

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://") || value.starts_with("www.")
}

fn parse_url(value: &str) -> Result<url::Url> {
    let value = value.trim();
    let candidate = if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else {
        format!("https://{}", value.trim_start_matches('/'))
    };
    url::Url::parse(&candidate).map_err(|error| Error::Url(value.to_string(), error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_result_selection_is_none() {
        let results: Vec<u32> = Vec::new();
        assert!(nth_result(&results, 1).is_none());
    }

    #[test]
    fn result_indexes_are_one_based_and_wrap() {
        let results = [10u32, 20, 30];
        assert_eq!(nth_result(&results, 1), Some(&10));
        assert_eq!(nth_result(&results, 3), Some(&30));
        assert_eq!(nth_result(&results, 4), Some(&10));
        assert_eq!(nth_result(&results, 0), Some(&10));
    }
}
