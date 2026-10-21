use anyhow::Result;
use ap0dl_core::Config;

use super::{
    best_stream, choose_streams, engine, episode_selection, resolve_query, selected_episodes,
};

pub async fn run(
    config: Config,
    query: &str,
    provider: Option<&str>,
    index: usize,
    range: &str,
    quality: Option<&str>,
    include_fillers: bool,
) -> Result<()> {
    let default_quality = config.quality.clone();
    let engine = engine(config)?;
    let resolved = resolve_query(&engine, query, provider, index).await?;
    let episodes = engine
        .episodes(&resolved)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let selection = episode_selection(range)?;
    let episodes = selected_episodes(&episodes, &selection, include_fillers);

    if episodes.is_empty() {
        anyhow::bail!("no episodes matched the selected range");
    }

    for episode in episodes {
        let streams = engine
            .streams(&resolved, &episode)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let chosen = if quality.is_some() {
            choose_streams(&streams, quality, &default_quality)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        } else {
            streams.clone()
        };
        let payload = serde_json::json!({
            "anime": resolved.anime.title,
            "provider": resolved.provider.id(),
            "episode": episode.number,
            "title": episode.title,
            "streams": chosen,
        });
        println!("{}", serde_json::to_string(&payload)?);
        if let Some(_stream) = best_stream(&streams, quality, &default_quality) {
            // `best_stream` validates that at least one stream can be selected;
            // the JSON above is the machine-readable result.
        }
    }
    Ok(())
}
