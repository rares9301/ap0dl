use std::path::PathBuf;

use anyhow::Result;
use ap0dl_core::Config;
use ap0dl_core::download::header_map;
use ap0dl_core::player;
use ap0dl_core::proxy::FlixProxy;

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
    player_override: Option<&str>,
    extra_player_args: Vec<String>,
    include_fillers: bool,
    print_url: bool,
) -> Result<()> {
    let default_quality = config.quality.clone();
    let engine = engine(config.clone())?;
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

    let player_name = player_override
        .map(str::to_string)
        .unwrap_or_else(|| config.player.clone());
    let player_name = if which_player(&player_name) {
        player_name
    } else {
        player::find_player().unwrap_or(player_name)
    };

    for episode in episodes {
        let streams = engine
            .streams(&resolved, &episode)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let Some(stream) = best_stream(&streams, quality, &default_quality) else {
            eprintln!(
                "No playable streams for {} {:?}.",
                resolved.anime.title, episode.number
            );
            continue;
        };

        if print_url {
            println!("{}", stream.url);
            continue;
        }

        let proxy = match &stream.flix {
            Some(info) => match FlixProxy::start(info.clone()).await {
                Ok(proxy) => Some(proxy),
                Err(error) => {
                    tracing::warn!(%error, "could not start flixcloud proxy; using remote URL");
                    None
                }
            },
            None => None,
        };
        let url = proxy
            .as_ref()
            .map(|proxy| proxy.url())
            .unwrap_or_else(|| stream.url.clone());
        let title = format!(
            "{} - {}",
            resolved.anime.title,
            episode_label(episode.number)
        );

        let subtitles = download_subtitles(&engine, stream, &title).await?;
        eprintln!(
            "Playing {} [{}] {}",
            title,
            episode_label(episode.number),
            stream.display()
        );

        let mut args = config.player_args.clone();
        args.extend(extra_player_args.clone());
        let headers = stream.headers.clone();
        let player_name = player_name.clone();
        let title_clone = title.clone();
        let subtitles_clone = subtitles.clone();
        tokio::task::spawn_blocking(move || {
            player::launch(
                &player_name,
                &url,
                &title_clone,
                &headers,
                &subtitles_clone,
                &args,
            )
        })
        .await
        .map_err(|error| anyhow::anyhow!("player task failed: {error}"))?
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    }
    Ok(())
}

async fn download_subtitles(
    engine: &ap0dl_core::Engine,
    stream: &ap0dl_core::Stream,
    title: &str,
) -> Result<Vec<PathBuf>> {
    if stream.subtitles.is_empty() {
        return Ok(Vec::new());
    }
    let dir = std::env::temp_dir()
        .join("ap0dl-subs")
        .join(ap0dl_core::util::sanitize_filename(title));
    tokio::fs::create_dir_all(&dir).await?;
    let headers = header_map(&stream.headers);
    let mut paths = Vec::new();
    for (index, subtitle) in stream.subtitles.iter().enumerate() {
        let extension = subtitle
            .format
            .as_deref()
            .filter(|format| !format.is_empty())
            .unwrap_or("srt");
        let path = dir.join(format!("{index}.{}", extension.trim_start_matches('.')));
        match engine.http.get_bytes(&subtitle.url, &headers).await {
            Ok(bytes) => {
                tokio::fs::write(&path, bytes).await?;
                paths.push(path);
            }
            Err(error) => {
                tracing::warn!(%error, lang = %subtitle.lang, "subtitle download failed");
            }
        }
    }
    Ok(paths)
}

fn which_player(name: &str) -> bool {
    which::which(name).is_ok()
}
