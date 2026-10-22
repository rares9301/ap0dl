use std::time::Duration;

use ap0dl_core::engine::fuzzy_score;
use ap0dl_core::models::{Episode, Stream};
use ap0dl_core::{Engine, Resolved};

use super::health::provider_is_degraded;

const FALLBACK_TIMEOUT: Duration = Duration::from_secs(8);

/// If a provider returns the right title but its stream endpoint is down,
/// try the same title on another provider before giving up. This keeps a dead
/// upstream request from blocking playback or download entirely.
pub async fn fallback_stream(
    engine: &Engine,
    title: &str,
    current_provider: &str,
    episode_number: f32,
) -> Option<(Resolved, Episode, Vec<Stream>)> {
    let hits = match tokio::time::timeout(FALLBACK_TIMEOUT, engine.search(None, title, 1, 10)).await
    {
        Ok(Ok(hits)) => hits,
        _ => return None,
    };
    for (provider, result) in hits {
        if provider.id() == current_provider
            || provider_is_degraded(provider.id())
            || fuzzy_score(title, &result.title) < 0.999
        {
            continue;
        }
        let resolved =
            match tokio::time::timeout(FALLBACK_TIMEOUT, engine.resolve_result(provider, result))
                .await
            {
                Ok(Ok(resolved)) => resolved,
                _ => continue,
            };
        let episodes =
            match tokio::time::timeout(FALLBACK_TIMEOUT, engine.episodes(&resolved)).await {
                Ok(Ok(episodes)) => episodes,
                _ => continue,
            };
        let Some(episode) = episodes
            .into_iter()
            .find(|episode| (episode.number - episode_number).abs() < 0.001)
        else {
            continue;
        };
        match tokio::time::timeout(FALLBACK_TIMEOUT, engine.streams(&resolved, &episode)).await {
            Ok(Ok(streams)) if !streams.is_empty() => {
                return Some((resolved, episode, streams));
            }
            _ => {}
        }
    }
    None
}
