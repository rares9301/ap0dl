use std::path::PathBuf;

use anyhow::Result;
use ap0dl_core::Config;
use ap0dl_core::download::{DownloadOptions, download_episode};

use super::{
    best_stream, engine, episode_label, episode_selection, resolve_query, selected_episodes,
};

#[allow(clippy::too_many_arguments)] // mirrors the CLI flag surface
pub async fn run(
    config: Config,
    query: &str,
    provider: Option<&str>,
    index: usize,
    range: &str,
    quality: Option<&str>,
    dir: Option<PathBuf>,
    include_fillers: bool,
    no_merge: bool,
    json: bool,
) -> Result<()> {
    let default_quality = config.quality.clone();
    let download_root = dir
        .or_else(|| Some(config.download_dir.clone()))
        .unwrap_or_else(|| PathBuf::from("."));
    let ffmpeg = config.ffmpeg.clone();
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

    let options = DownloadOptions {
        ffmpeg,
        merge_video: !no_merge,
        merge_subtitles: !no_merge,
        xor_mask: Vec::new(),
    };
    let mut downloaded = 0usize;

    for episode in episodes {
        let streams = engine
            .streams(&resolved, &episode)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let Some(stream) = best_stream(&streams, quality, &default_quality) else {
            eprintln!(
                "No playable streams for {} {}.",
                resolved.anime.title,
                episode_label(episode.number)
            );
            continue;
        };
        eprintln!(
            "Downloading {} {} -> {}",
            resolved.anime.title,
            episode_label(episode.number),
            stream.display()
        );

        match download_episode(
            &engine.http,
            stream,
            &resolved.anime.title,
            episode.number,
            &download_root,
            &options,
        )
        .await
        {
            Ok(result) => {
                downloaded += 1;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string(&serde_json::json!({
                            "episode": episode.number,
                            "path": result.path,
                            "subtitles": result.subtitles,
                        }))?
                    );
                } else {
                    println!("{}", result.path.display());
                    for subtitle in result.subtitles {
                        println!("  subtitle: {}", subtitle.display());
                    }
                }
            }
            Err(error) => eprintln!("Download failed for episode {}: {error}", episode.number),
        }
    }

    if downloaded == 0 {
        anyhow::bail!("nothing was downloaded");
    }
    Ok(())
}
