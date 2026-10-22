pub mod config;
pub mod download;
pub mod episodes;
pub mod fallback;
pub mod grab;
pub mod health;
pub mod interactive;
pub mod providers;
pub mod search;
pub mod stream;

use anyhow::{Context, Result};
use ap0dl_core::config::Config;
use ap0dl_core::models::{Episode, Stream};
use ap0dl_core::ranges::EpisodeSelection;
use ap0dl_core::{Engine, Resolved};

pub fn engine(config: Config) -> Result<Engine> {
    Engine::new(config).map_err(|error| anyhow::anyhow!(error.to_string()))
}

pub async fn resolve_query(
    engine: &Engine,
    query: &str,
    provider: Option<&str>,
    index: usize,
) -> Result<Resolved> {
    let effective = match provider {
        Some(provider)
            if !query.starts_with("http://")
                && !query.starts_with("https://")
                && !query.contains(':') =>
        {
            format!("{provider}:{query}")
        }
        _ => query.to_string(),
    };
    engine
        .resolve(&effective, index)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .with_context(|| format!("could not resolve {query:?}"))
}

pub fn episode_selection(range: &str) -> Result<EpisodeSelection> {
    range
        .parse()
        .map_err(|error: String| anyhow::anyhow!("invalid episode range: {error}"))
}

pub fn selected_episodes(
    episodes: &[Episode],
    selection: &EpisodeSelection,
    include_fillers: bool,
) -> Vec<Episode> {
    let filtered: Vec<Episode> = episodes
        .iter()
        .filter(|episode| include_fillers || !episode.filler)
        .cloned()
        .collect();
    selection.apply(&filtered)
}

pub fn choose_streams<'a>(
    streams: &'a [Stream],
    quality: Option<&str>,
    default: &str,
) -> Vec<&'a Stream> {
    let spec = quality.unwrap_or(default);
    let selected = ap0dl_core::quality::select(streams, spec);
    if selected.is_empty() {
        streams.iter().collect()
    } else {
        selected
    }
}

pub fn best_stream<'a>(
    streams: &'a [Stream],
    quality: Option<&str>,
    default: &str,
) -> Option<&'a Stream> {
    choose_streams(streams, quality, default).into_iter().next()
}

pub fn episode_label(episode: f32) -> String {
    if episode.fract() == 0.0 {
        format!("Episode {:02}", episode as u32)
    } else {
        format!("Episode {episode}")
    }
}
