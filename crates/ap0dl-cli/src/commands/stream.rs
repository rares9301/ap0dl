use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ap0dl_core::Config;
use ap0dl_core::download::header_map;
use ap0dl_core::models::Subtitle;
use ap0dl_core::player;
use ap0dl_core::proxy::FlixProxy;
use futures::future::join_all;

use super::fallback::fallback_stream;
use super::health::{mark_provider_degraded, mark_provider_healthy};
use super::interactive::{
    DefaultEpisodes, choose_anime, choose_episodes, is_interactive as terminal_interactive,
};
use super::{best_stream, engine, episode_label};

const STREAM_TIMEOUT: Duration = Duration::from_secs(4);

#[allow(clippy::too_many_arguments)] // mirrors the CLI flag surface
pub async fn run(
    config: Config,
    query: Option<&str>,
    provider: Option<&str>,
    index: Option<usize>,
    range: Option<&str>,
    quality: Option<&str>,
    player_override: Option<&str>,
    extra_player_args: Vec<String>,
    include_fillers: bool,
    print_url: bool,
    no_interactive: bool,
) -> Result<()> {
    let default_quality = config.quality.clone();
    let engine = engine(config.clone())?;
    let interactive = terminal_interactive(no_interactive);

    let mut resolved = choose_anime(&engine, query, provider, index, interactive).await?;
    let episodes = choose_episodes(
        &engine,
        &resolved,
        range,
        include_fillers,
        interactive,
        DefaultEpisodes::First,
        "play",
    )
    .await?;

    let configured_player = player_override
        .map(str::to_string)
        .unwrap_or_else(|| config.player.clone());
    // The default "mpv" means "the best mpv for this host"; on WSL that is
    // Windows mpv.exe, which has native hardware acceleration. An explicit
    // --player mpv still selects Linux mpv.
    let player_name = if player_override.is_none() && configured_player == "mpv" {
        player::find_player().unwrap_or(configured_player)
    } else if which_player(&configured_player) {
        configured_player
    } else {
        player::find_player().unwrap_or(configured_player)
    };
    let windows_mpv = player::is_windows_player(&player_name) && player::is_mpv_like(&player_name);

    for episode in episodes {
        let mut playback_episode = episode.clone();
        let streams = match tokio::time::timeout(
            STREAM_TIMEOUT,
            engine.streams(&resolved, &playback_episode),
        )
        .await
        {
            Ok(Ok(streams)) if !streams.is_empty() => streams,
            Ok(Ok(_)) => {
                mark_provider_degraded(resolved.provider.id());
                eprintln!(
                    "{} returned no streams for {}; trying another provider...",
                    resolved.provider.name(),
                    episode_label(playback_episode.number)
                );
                match fallback_stream(
                    &engine,
                    &resolved.anime.title,
                    resolved.provider.id(),
                    playback_episode.number,
                )
                .await
                {
                    Some((fallback_resolved, fallback_episode, streams)) => {
                        eprintln!("Using {} instead.", fallback_resolved.provider.name());
                        resolved = fallback_resolved;
                        playback_episode = fallback_episode;
                        streams
                    }
                    None => {
                        eprintln!(
                            "No playable streams for {} {}.",
                            resolved.anime.title,
                            episode_label(playback_episode.number)
                        );
                        continue;
                    }
                }
            }
            Ok(Err(error)) => {
                mark_provider_degraded(resolved.provider.id());
                eprintln!(
                    "{} stream resolution failed: {error}; trying another provider...",
                    resolved.provider.name()
                );
                match fallback_stream(
                    &engine,
                    &resolved.anime.title,
                    resolved.provider.id(),
                    playback_episode.number,
                )
                .await
                {
                    Some((fallback_resolved, fallback_episode, streams)) => {
                        eprintln!("Using {} instead.", fallback_resolved.provider.name());
                        resolved = fallback_resolved;
                        playback_episode = fallback_episode;
                        streams
                    }
                    None => {
                        eprintln!(
                            "No playable streams for {} {}.",
                            resolved.anime.title,
                            episode_label(playback_episode.number)
                        );
                        continue;
                    }
                }
            }
            Err(_) => {
                mark_provider_degraded(resolved.provider.id());
                eprintln!(
                    "{} stream resolution timed out; trying another provider...",
                    resolved.provider.name()
                );
                match fallback_stream(
                    &engine,
                    &resolved.anime.title,
                    resolved.provider.id(),
                    playback_episode.number,
                )
                .await
                {
                    Some((fallback_resolved, fallback_episode, streams)) => {
                        eprintln!("Using {} instead.", fallback_resolved.provider.name());
                        resolved = fallback_resolved;
                        playback_episode = fallback_episode;
                        streams
                    }
                    None => {
                        eprintln!(
                            "No playable streams for {} {}.",
                            resolved.anime.title,
                            episode_label(playback_episode.number)
                        );
                        continue;
                    }
                }
            }
        };
        mark_provider_healthy(resolved.provider.id());
        let Some(stream) = best_stream(&streams, quality, &default_quality) else {
            eprintln!(
                "No playable streams for {} {:?}.",
                resolved.anime.title, playback_episode.number
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
            episode_label(playback_episode.number)
        );

        // Windows mpv cannot read Linux temp paths. Pass subtitle URLs
        // directly instead of downloading files it cannot open.
        let mut subtitle_args = Vec::new();
        let subtitles = if windows_mpv {
            subtitle_args.extend(
                preferred_subtitles(&stream.subtitles, 4)
                    .into_iter()
                    .map(|subtitle| format!("--sub-file={}", subtitle.url)),
            );
            Vec::new()
        } else {
            download_subtitles(&engine, stream, &title).await?
        };
        eprintln!(
            "Playing {} [{}] {}",
            title,
            episode_label(playback_episode.number),
            stream.display()
        );

        let mut args = config.player_args.clone();
        args.extend(extra_player_args.clone());
        args.extend(subtitle_args);
        // AnimeParadise serves HLS segments as `/ts?url=...`, which recent
        // FFmpeg versions reject because the URL has no extension. These
        // demuxer options are safe for regular HLS too.
        if url.contains("stream.animeparadise.moe") {
            args.push(
                "--demuxer-lavf-o=allowed_segment_extensions=ALL,extension_picky=0".to_string(),
            );
        }
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

    // Some providers expose dozens of language tracks. Downloading all of them
    // before the player starts wastes time and makes mpv's subtitle menu huge.
    // Prefer the default/English tracks and cap the eager downloads.
    let selected = preferred_subtitles(&stream.subtitles, 4);
    let headers = header_map(&stream.headers);
    let http = engine.http.clone();
    let mut tasks = Vec::with_capacity(selected.len());
    for (index, subtitle) in selected.iter().enumerate() {
        let extension = subtitle
            .format
            .as_deref()
            .filter(|format| !format.is_empty())
            .unwrap_or("srt");
        let path = dir.join(format!("{index}.{}", extension.trim_start_matches('.')));
        let url = subtitle.url.clone();
        let lang = subtitle.lang.clone();
        let headers = headers.clone();
        let http = http.clone();
        tasks.push(async move {
            match http.get_bytes(&url, &headers).await {
                Ok(bytes) => match tokio::fs::write(&path, bytes).await {
                    Ok(()) => Some(path),
                    Err(error) => {
                        tracing::warn!(%error, %lang, "subtitle write failed");
                        None
                    }
                },
                Err(error) => {
                    tracing::warn!(%error, %lang, "subtitle download failed");
                    None
                }
            }
        });
    }
    Ok(join_all(tasks).await.into_iter().flatten().collect())
}

fn preferred_subtitles(subtitles: &[Subtitle], limit: usize) -> Vec<&Subtitle> {
    let mut selected: Vec<&Subtitle> = subtitles
        .iter()
        .filter(|subtitle| subtitle.default)
        .collect();
    for subtitle in subtitles {
        if selected.len() >= limit {
            break;
        }
        if is_english(&subtitle.lang) && !selected.iter().any(|item| item.url == subtitle.url) {
            selected.push(subtitle);
        }
    }
    for subtitle in subtitles {
        if selected.len() >= limit {
            break;
        }
        if !selected.iter().any(|item| item.url == subtitle.url) {
            selected.push(subtitle);
        }
    }
    selected.truncate(limit);
    selected
}

fn is_english(language: &str) -> bool {
    let language = language.to_ascii_lowercase();
    language.contains("english")
        || language == "en"
        || language.starts_with("en-")
        || language.starts_with("eng")
}

fn which_player(name: &str) -> bool {
    which::which(name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subtitle(language: &str, default: bool) -> Subtitle {
        Subtitle {
            url: format!("https://example.test/{language}.srt"),
            lang: language.to_string(),
            format: Some("srt".to_string()),
            default,
        }
    }

    #[test]
    fn subtitle_selection_prefers_english_and_caps_downloads() {
        let subtitles = vec![
            subtitle("Spanish", false),
            subtitle("English", false),
            subtitle("French", false),
            subtitle("Japanese", false),
            subtitle("English (signs)", false),
        ];
        let selected = preferred_subtitles(&subtitles, 4);
        assert_eq!(selected.len(), 4);
        assert_eq!(selected[0].lang, "English");
        assert_eq!(selected[1].lang, "English (signs)");
    }

    #[test]
    fn subtitle_selection_keeps_only_four_large_tracks() {
        let subtitles: Vec<Subtitle> = (0..30)
            .map(|index| subtitle(&format!("Language {index}"), index == 0))
            .collect();
        assert_eq!(preferred_subtitles(&subtitles, 4).len(), 4);
    }

    #[test]
    fn english_language_detection() {
        assert!(is_english("English"));
        assert!(is_english("en-US"));
        assert!(is_english("eng"));
        assert!(!is_english("Spanish"));
    }
}
