use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;

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

        if self.providers.is_empty() {
            return Err(Error::config("no providers are registered"));
        }

        // The old default-provider-first flow often returned a loosely related
        // title (for example PLUTO for "cyberpunk 2") because that provider's
        // search is fuzzy server-side. Query every provider concurrently and
        // rank the merged results locally; this is both more accurate and
        // faster than a serial fallback chain.
        let per_provider_limit = limit.max(1);
        let search_timeout = Duration::from_secs(self.config.timeout_secs.clamp(3, 10));
        let requests = self.providers.iter().cloned().map(|provider| {
            let http = self.http.clone();
            let query = query.clone();
            async move {
                match tokio::time::timeout(search_timeout, provider.search(&http, &query, page))
                    .await
                {
                    Ok(Ok(mut results)) => {
                        rank_results(&query, &mut results);
                        Some((provider, results))
                    }
                    Ok(Err(error)) => {
                        tracing::warn!(provider = provider.id(), %error, "provider search failed");
                        None
                    }
                    Err(_) => {
                        tracing::warn!(
                            provider = provider.id(),
                            seconds = search_timeout.as_secs(),
                            "provider search timed out"
                        );
                        None
                    }
                }
            }
        });

        let mut collected: Vec<(Arc<dyn Provider>, SearchResult)> = Vec::new();
        for (provider, results) in join_all(requests).await.into_iter().flatten() {
            collected.extend(limit_results(provider, results, per_provider_limit));
        }

        if collected.is_empty() {
            return Err(Error::NoResults(query));
        }

        let preferred_provider = self.config.default_provider.clone();
        collected.sort_by(|(left_provider, left), (right_provider, right)| {
            fuzzy_score(&query, &right.title)
                .partial_cmp(&fuzzy_score(&query, &left.title))
                .unwrap_or(Ordering::Equal)
                .then_with(|| {
                    let left_preferred = left_provider.id() == preferred_provider.as_str();
                    let right_preferred = right_provider.id() == preferred_provider.as_str();
                    right_preferred.cmp(&left_preferred)
                })
                .then_with(|| left.title.len().cmp(&right.title.len()))
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left_provider.id().cmp(right_provider.id()))
        });
        collected.truncate(limit.max(1));
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
        self.resolve_result(provider, result).await
    }

    /// Resolves an already-selected search hit. The interactive picker uses
    /// this to avoid searching the same providers twice.
    pub async fn resolve_result(
        &self,
        provider: Arc<dyn Provider>,
        result: SearchResult,
    ) -> Result<Resolved> {
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

/// Makes provider result ordering predictable. Sites frequently return
/// loosely related series before the requested one, so use token coverage plus
/// whole-title similarity instead of a simple substring check. This lets
/// `cyberpunk 2` rank `Cyberpunk: Edgerunners 2` above `PLUTO`.
fn rank_results(query: &str, results: &mut [SearchResult]) {
    if query.trim().is_empty() {
        return;
    }
    results.sort_by(|a, b| {
        fuzzy_score(query, &b.title)
            .partial_cmp(&fuzzy_score(query, &a.title))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.title.len().cmp(&b.title.len()))
            .then_with(|| a.title.cmp(&b.title))
    });
}

/// Returns a 0..1 match score for a user query against a result title.
pub fn fuzzy_score(query: &str, title: &str) -> f64 {
    let query = normalize_title(query);
    let title = normalize_title(title);
    if query.is_empty() || title.is_empty() {
        return 0.0;
    }
    if query == title {
        return 1.0;
    }

    let query_tokens: Vec<&str> = query.split_whitespace().collect();
    let title_tokens: Vec<&str> = title.split_whitespace().collect();
    if query_tokens.is_empty() || title_tokens.is_empty() {
        return 0.0;
    }

    let coverage = query_tokens
        .iter()
        .map(|query_token| {
            title_tokens
                .iter()
                .map(|title_token| token_score(query_token, title_token))
                .fold(0.0, f64::max)
        })
        .sum::<f64>()
        / query_tokens.len() as f64;

    let whole = strsim::jaro_winkler(&query, &title);
    let extra_tokens = title_tokens.len().saturating_sub(query_tokens.len()) as f64;
    let length_penalty = (extra_tokens * 0.01).min(0.08);
    let all_tokens_exact = query_tokens.iter().all(|query_token| {
        title_tokens
            .iter()
            .any(|title_token| query_token == title_token)
    });

    let mut score = coverage * 0.72 + whole * 0.28 - length_penalty;
    if all_tokens_exact {
        score += 0.04;
    }
    if title.starts_with(&query) {
        score += 0.02;
    }
    score.clamp(0.0, 1.0)
}

fn token_score(query: &str, title: &str) -> f64 {
    if query == title {
        return 1.0;
    }
    // Numeric tokens are season/episode numbers, not fuzzy words. This keeps
    // "2" from matching "2099" or "2025".
    if query.chars().all(|character| character.is_ascii_digit())
        || title.chars().all(|character| character.is_ascii_digit())
    {
        return 0.0;
    }
    if title.starts_with(query) || query.starts_with(title) {
        return 0.92;
    }
    if title.contains(query) || query.contains(title) {
        return 0.84;
    }
    let similarity = strsim::normalized_levenshtein(query, title);
    if similarity >= 0.8 {
        0.65 + similarity * 0.25
    } else {
        0.0
    }
}

fn normalize_title(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_alphanumeric() {
            if pending_space && !output.is_empty() {
                output.push(' ');
            }
            output.extend(character.to_lowercase());
            pending_space = false;
        } else if character == '&' {
            if !output.is_empty() {
                output.push(' ');
            }
            output.push_str("and");
            pending_space = true;
        } else {
            pending_space = true;
        }
    }
    output.trim().to_string()
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

    fn search_result(title: &str) -> SearchResult {
        SearchResult {
            id: title.to_string(),
            title: title.to_string(),
            url: format!("https://example.test/{title}"),
            provider: "test".to_string(),
            cover: None,
            year: None,
            format: None,
            episodes: None,
        }
    }

    #[test]
    fn fuzzy_cyberpunk_season_two_beats_pluto() {
        let mut results = vec![
            search_result("PLUTO"),
            search_result("PSYCHO-PASS 2"),
            search_result("Cyberpunk: Edgerunners"),
            search_result("Cyberpunk: Edgerunners 2"),
        ];
        rank_results("cyberpunk 2", &mut results);
        assert_eq!(results[0].title, "Cyberpunk: Edgerunners 2");
        assert_eq!(results[1].title, "Cyberpunk: Edgerunners");
        let pluto = results
            .iter()
            .position(|result| result.title == "PLUTO")
            .unwrap();
        assert!(pluto > 1);
    }

    #[test]
    fn fuzzy_numeric_tokens_do_not_match_partial_years() {
        assert!(fuzzy_score("cyberpunk 2", "Cyberpunk: Edgerunners 2") > 0.9);
        assert!(fuzzy_score("2", "Demon Lord 2099") < 0.3);
    }

    #[test]
    fn fuzzy_exact_and_prefix_titles_rank_first() {
        assert_eq!(fuzzy_score("BOCCHI THE ROCK!", "Bocchi the Rock!"), 1.0);
        assert!(fuzzy_score("bocchi", "BOCCHI THE ROCK!") > 0.9);
        assert!(fuzzy_score("bocchi", "Bocchi the Rock! Recap") > 0.8);
    }
}
