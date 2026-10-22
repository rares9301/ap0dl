use std::path::PathBuf;

use anyhow::Result;
use ap0dl_core::Config;
use ap0dl_core::download::{DownloadOptions, download_episode};

use super::fallback::fallback_stream;
use super::health::{mark_provider_degraded, mark_provider_healthy};
use super::interactive::{
    choose_anime, choose_download_episodes, is_interactive as terminal_interactive,
};
use super::{best_stream, engine, episode_label};

#[allow(clippy::too_many_arguments)] // mirrors the CLI flag surface
pub async fn run(
    config: Config,
    query: Option<&str>,
    provider: Option<&str>,
    index: Option<usize>,
    range: Option<&str>,
    quality: Option<&str>,
    dir: Option<PathBuf>,
    include_fillers: bool,
    no_merge: bool,
    json: bool,
    print_url: bool,
    no_interactive: bool,
) -> Result<()> {
    let default_quality = config.quality.clone();
    let download_root = dir
        .or_else(|| Some(config.download_dir.clone()))
        .unwrap_or_else(|| PathBuf::from("."));
    let ffmpeg = config.ffmpeg.clone();
    let engine = engine(config)?;
    let interactive = terminal_interactive(no_interactive);
    let mut resolved = choose_anime(&engine, query, provider, index, interactive).await?;
    let episodes =
        choose_download_episodes(&engine, &resolved, range, include_fillers, interactive)
            .await?
            .selected;

    let options = DownloadOptions {
        ffmpeg,
        merge_video: !no_merge,
        merge_subtitles: !no_merge,
        xor_mask: Vec::new(),
    };
    let mut downloaded = 0usize;

    for mut episode in episodes {
        eprintln!(
            "Resolving {} {}...",
            resolved.anime.title,
            episode_label(episode.number)
        );
        let streams = match engine.streams(&resolved, &episode).await {
            Ok(streams) if !streams.is_empty() => streams,
            other => {
                if let Err(error) = &other {
                    eprintln!(
                        "{} stream resolution failed: {error}; trying another provider...",
                        resolved.provider.name()
                    );
                } else {
                    eprintln!(
                        "{} returned no streams; trying another provider...",
                        resolved.provider.name()
                    );
                }
                mark_provider_degraded(resolved.provider.id());
                match fallback_stream(
                    &engine,
                    &resolved.anime.title,
                    resolved.provider.id(),
                    episode.number,
                )
                .await
                {
                    Some((fallback_resolved, fallback_episode, streams)) => {
                        eprintln!("Using {} instead.", fallback_resolved.provider.name());
                        resolved = fallback_resolved;
                        episode = fallback_episode;
                        streams
                    }
                    None => {
                        eprintln!(
                            "No playable streams for {} {}.",
                            resolved.anime.title,
                            episode_label(episode.number)
                        );
                        continue;
                    }
                }
            }
        };
        mark_provider_healthy(resolved.provider.id());

        let Some(stream) = best_stream(&streams, quality, &default_quality) else {
            eprintln!(
                "No playable streams for {} {}.",
                resolved.anime.title,
                episode_label(episode.number)
            );
            continue;
        };
        if print_url {
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "episode": episode.number,
                        "url": stream.url,
                        "quality": stream.quality,
                    }))?
                );
            } else {
                println!("{}", stream.url);
            }
            downloaded += 1;
            continue;
        }
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
