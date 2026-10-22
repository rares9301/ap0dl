use std::io::IsTerminal;

use anyhow::{Context, Result};
use ap0dl_core::models::Episode;
use ap0dl_core::{Engine, Resolved};
use dialoguer::{FuzzySelect, Input};

use super::health::sort_hits_by_health;
use super::{episode_selection, selected_episodes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultEpisodes {
    First,
    All,
}

pub fn is_interactive(no_interactive: bool) -> bool {
    !no_interactive && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Shared anime picker used by both `stream` and `download`.
pub async fn choose_anime(
    engine: &Engine,
    query: Option<&str>,
    provider: Option<&str>,
    index: Option<usize>,
    interactive: bool,
) -> Result<Resolved> {
    let query = match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(query) => query.to_string(),
        None if interactive => Input::<String>::new()
            .with_prompt("Search anime")
            .interact_text()
            .context("search prompt cancelled")?,
        None => anyhow::bail!(
            "an anime query is required when stdin is not a terminal; pass a title, for example: ap0dl \"cyberpunk 2\""
        ),
    };
    if query.trim().is_empty() {
        anyhow::bail!("empty anime query");
    }

    if is_url(&query) {
        return engine
            .resolve(&query, index.unwrap_or(1))
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .with_context(|| format!("could not resolve {query:?}"));
    }

    let mut hits = engine
        .search(provider, &query, 1, 40)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .with_context(|| format!("could not search for {query:?}"))?;
    if hits.is_empty() {
        anyhow::bail!("no results for {query:?}");
    }
    sort_hits_by_health(&mut hits);

    let selected = if let Some(index) = index {
        hits[index.saturating_sub(1) % hits.len()].clone()
    } else if interactive {
        let labels: Vec<String> = hits
            .iter()
            .map(|(provider, result)| {
                let year = result
                    .year
                    .map(|year| format!(" ({year})"))
                    .unwrap_or_default();
                format!("[{}] {}{}", provider.name(), result.title, year)
            })
            .collect();
        let choice = FuzzySelect::new()
            .with_prompt("Select anime (type to fuzzy-filter, Enter to pick)")
            .items(&labels)
            .default(0)
            .interact()
            .context("anime selection cancelled")?;
        hits[choice].clone()
    } else {
        // Engine ranking already ordered by title score. The health sort kept
        // that order within each group while moving recently failing providers
        // below healthy ones, so the first hit is the best usable match.
        hits.into_iter()
            .next()
            .expect("hits was checked as non-empty")
    };

    engine
        .resolve_result(selected.0, selected.1)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .with_context(|| format!("could not resolve {query:?}"))
}

/// Shared episode picker. `range` is explicit scriptable input; when omitted,
/// an interactive terminal gets the fuzzy picker and scripts get either the
/// first episode (stream) or every episode (download).
pub async fn choose_episodes(
    engine: &Engine,
    resolved: &Resolved,
    range: Option<&str>,
    include_fillers: bool,
    interactive: bool,
    default: DefaultEpisodes,
    action: &str,
) -> Result<Vec<Episode>> {
    let episodes = engine
        .episodes(resolved)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if episodes.is_empty() {
        anyhow::bail!("no episodes found for {}", resolved.anime.title);
    }

    if let Some(range) = range {
        let selection = episode_selection(range)?;
        let selected = selected_episodes(&episodes, &selection, include_fillers);
        if selected.is_empty() {
            anyhow::bail!("no episodes matched the selected range");
        }
        return Ok(selected);
    }

    let visible: Vec<Episode> = episodes
        .into_iter()
        .filter(|episode| include_fillers || !episode.filler)
        .collect();
    if visible.is_empty() {
        anyhow::bail!("no episodes matched the selection");
    }
    if !interactive {
        return Ok(match default {
            DefaultEpisodes::First => visible.into_iter().take(1).collect(),
            DefaultEpisodes::All => visible,
        });
    }

    let labels: Vec<String> = visible
        .iter()
        .map(|episode| episode.display_title())
        .collect();
    let choice = FuzzySelect::new()
        .with_prompt(format!(
            "Select episode to {action} (type to fuzzy-filter, Enter to confirm)"
        ))
        .items(&labels)
        .default(0)
        .interact()
        .context("episode selection cancelled")?;
    Ok(vec![visible[choice].clone()])
}

fn is_url(query: &str) -> bool {
    query.starts_with("http://") || query.starts_with("https://") || query.starts_with("www.")
}
