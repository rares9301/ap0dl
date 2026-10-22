use std::io::IsTerminal;

use anyhow::{Context, Result};
use ap0dl_core::models::Episode;
use ap0dl_core::{Engine, Resolved};
use dialoguer::{FuzzySelect, Input, MultiSelect};

use super::health::sort_hits_by_health;
use super::{episode_selection, selected_episodes};

/// The episodes chosen for an action plus the full visible list. `stream`
/// keeps the list so it can implement next/previous and re-picking after the
/// player exits.
#[derive(Debug, Clone)]
pub struct EpisodeChoice {
    pub selected: Vec<Episode>,
    pub all: Vec<Episode>,
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

/// Fetches the episode list once and applies the filler filter. Both pickers
/// and the post-playback next/previous controls use this same list.
pub async fn load_episodes(
    engine: &Engine,
    resolved: &Resolved,
    include_fillers: bool,
) -> Result<Vec<Episode>> {
    let episodes = engine
        .episodes(resolved)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let visible: Vec<Episode> = episodes
        .into_iter()
        .filter(|episode| include_fillers || !episode.filler)
        .collect();
    if visible.is_empty() {
        anyhow::bail!("no episodes found for {}", resolved.anime.title);
    }
    Ok(visible)
}

fn from_range(all: &[Episode], range: &str, include_fillers: bool) -> Result<EpisodeChoice> {
    let selection = episode_selection(range)?;
    let selected = selected_episodes(all, &selection, include_fillers);
    if selected.is_empty() {
        anyhow::bail!("no episodes matched the selected range");
    }
    Ok(EpisodeChoice {
        selected,
        all: all.to_vec(),
    })
}

/// Single-episode fuzzy picker used by `stream`, returning the full list for
/// next/previous after the player closes.
pub async fn choose_stream_episodes(
    engine: &Engine,
    resolved: &Resolved,
    range: Option<&str>,
    include_fillers: bool,
    interactive: bool,
) -> Result<EpisodeChoice> {
    let all = load_episodes(engine, resolved, include_fillers).await?;
    if let Some(range) = range {
        return from_range(&all, range, include_fillers);
    }
    if !interactive {
        return Ok(EpisodeChoice {
            selected: vec![all[0].clone()],
            all,
        });
    }

    let choice = pick_episode(&all, "Select episode to play", 0)?
        .expect("non-empty episode list has a selection");
    Ok(EpisodeChoice {
        selected: vec![all[choice].clone()],
        all,
    })
}

/// Multi-select checklist used by `download`. Space toggles an episode; the
/// first row is a convenient "download everything" option. Existing
/// `--range`/non-TTY behaviour is unchanged, so scripts still select exactly
/// what they asked for.
pub async fn choose_download_episodes(
    engine: &Engine,
    resolved: &Resolved,
    range: Option<&str>,
    include_fillers: bool,
    interactive: bool,
) -> Result<EpisodeChoice> {
    let all = load_episodes(engine, resolved, include_fillers).await?;
    if let Some(range) = range {
        return from_range(&all, range, include_fillers);
    }
    if !interactive {
        return Ok(EpisodeChoice {
            selected: all.clone(),
            all,
        });
    }

    let mut labels = Vec::with_capacity(all.len() + 1);
    labels.push(format!("DOWNLOAD ALL EPISODES ({})", all.len()));
    labels.extend(all.iter().map(Episode::display_title));
    let picks = MultiSelect::new()
        .with_prompt("Select episodes (space toggles, Enter starts download)")
        .items(&labels)
        .interact()
        .context("episode selection cancelled")?;
    if picks.is_empty() {
        anyhow::bail!("no episodes selected");
    }
    let selected = if picks.contains(&0) {
        all.clone()
    } else {
        picks
            .into_iter()
            .filter(|index| *index > 0)
            .map(|index| all[index - 1].clone())
            .collect()
    };
    Ok(EpisodeChoice { selected, all })
}

/// Reusable fuzzy episode selector. Returns `None` when the user cancels, so
/// the post-playback menu can return to its action list instead of aborting.
pub fn pick_episode(episodes: &[Episode], prompt: &str, default: usize) -> Result<Option<usize>> {
    if episodes.is_empty() {
        return Ok(None);
    }
    let labels: Vec<String> = episodes.iter().map(Episode::display_title).collect();
    match FuzzySelect::new()
        .with_prompt(prompt)
        .items(&labels)
        .default(default.min(labels.len() - 1))
        .interact()
    {
        Ok(choice) => Ok(Some(choice)),
        Err(_) => Ok(None),
    }
}

fn is_url(query: &str) -> bool {
    query.starts_with("http://") || query.starts_with("https://") || query.starts_with("www.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(number: f32) -> Episode {
        Episode {
            number,
            title: None,
            id: number.to_string(),
            url: String::new(),
            filler: false,
            recap: false,
            aired: None,
            audio: ap0dl_core::models::Audio::Sub,
        }
    }

    #[test]
    fn range_selection_keeps_the_full_list_for_navigation() {
        let all = vec![episode(1.0), episode(2.0), episode(3.0)];
        let choice = from_range(&all, "2-3", true).unwrap();
        assert_eq!(choice.all.len(), 3);
        assert_eq!(choice.selected.len(), 2);
        assert_eq!(choice.selected[0].number, 2.0);
    }
}
