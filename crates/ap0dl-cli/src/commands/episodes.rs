use anyhow::Result;
use ap0dl_core::Config;

use super::{engine, episode_selection, resolve_query, selected_episodes};

pub async fn run(
    config: Config,
    query: &str,
    provider: Option<&str>,
    index: usize,
    json: bool,
    include_fillers: bool,
) -> Result<()> {
    let engine = engine(config)?;
    let resolved = resolve_query(&engine, query, provider, index).await?;
    let episodes = engine
        .episodes(&resolved)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let selection = episode_selection(":")?;
    let episodes = selected_episodes(&episodes, &selection, include_fillers);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "anime": resolved.anime,
                "episodes": episodes,
            }))?
        );
        return Ok(());
    }

    println!("{} [{}]", resolved.anime.title, resolved.provider.name());
    println!("{}", resolved.anime.url);
    if let Some(description) = &resolved.anime.description {
        println!("{}", description.trim());
    }
    for episode in episodes {
        let mut flags = String::new();
        if episode.filler {
            flags.push_str(" [filler]");
        }
        if episode.recap {
            flags.push_str(" [recap]");
        }
        println!(
            "  {:>7}  {}{}",
            episode.number,
            episode.title.as_deref().unwrap_or(""),
            flags
        );
    }
    Ok(())
}
