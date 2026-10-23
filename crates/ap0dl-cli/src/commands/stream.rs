use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ap0dl_core::Config;
use ap0dl_core::download::{DownloadOptions, download_episode, header_map};
use ap0dl_core::models::{Episode, Stream, Subtitle};
use ap0dl_core::player;
use ap0dl_core::proxy::FlixProxy;
use ap0dl_core::{Engine, Resolved};
use dialoguer::FuzzySelect;
use futures::future::join_all;

use super::fallback::fallback_stream;
use super::health::{mark_provider_degraded, mark_provider_healthy};
use super::interactive::{
    choose_anime, choose_stream_episodes, is_interactive as terminal_interactive, load_episodes,
    pick_episode,
};
use super::{best_stream, engine, episode_label};

const STREAM_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackAction {
    Replay,
    Download,
    Next,
    Previous,
    ChangeQuality,
    PickEpisode,
    Quit,
}

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
    let choice =
        choose_stream_episodes(&engine, &resolved, range, include_fillers, interactive).await?;
    let mut quality_override = quality.map(str::to_string);

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

    // The post-player control menu is only for the normal interactive path:
    // one episode chosen from the picker. Ranges and scripts retain the old
    // sequential/headless behaviour.
    let post_playback = interactive && range.is_none() && !print_url && choice.selected.len() == 1;

    if !post_playback {
        for episode in choice.selected {
            let selected_episode = episode;
            let Some((next_resolved, actual_episode, streams)) =
                resolve_for_playback(&engine, resolved.clone(), selected_episode).await
            else {
                continue;
            };
            resolved = next_resolved;
            let Some(stream) = best_stream(&streams, quality_override.as_deref(), &default_quality)
            else {
                eprintln!(
                    "No playable streams for {} {}.",
                    resolved.anime.title,
                    episode_label(actual_episode.number)
                );
                continue;
            };
            if print_url {
                println!("{}", stream.url);
                continue;
            }
            launch_stream(
                &engine,
                &config,
                &resolved,
                &actual_episode,
                stream,
                &player_name,
                &extra_player_args,
                windows_mpv,
            )
            .await?;
        }
        return Ok(());
    }

    let mut all = choice.all;
    let mut current = 0usize;
    current = episode_position(&all, choice.selected[0].number).unwrap_or(current);

    loop {
        let requested = all[current].clone();
        let resolved_now = resolve_for_playback(&engine, resolved.clone(), requested).await;
        let Some((next_resolved, actual_episode, streams)) = resolved_now else {
            let next = all.get(current + 1);
            let previous = current.checked_sub(1).and_then(|index| all.get(index));
            match choose_action(&all[current], None, next, previous, false, false)? {
                PlaybackAction::Next => {
                    current = (current + 1).min(all.len().saturating_sub(1));
                }
                PlaybackAction::Previous => {
                    current = current.saturating_sub(1);
                }
                PlaybackAction::PickEpisode => {
                    if let Some(choice) = pick_episode(&all, "Select another episode", current)? {
                        current = choice;
                    }
                }
                PlaybackAction::Replay
                | PlaybackAction::Download
                | PlaybackAction::ChangeQuality => {}
                PlaybackAction::Quit => return Ok(()),
            }
            continue;
        };

        let provider_changed = next_resolved.provider.id() != resolved.provider.id();
        resolved = next_resolved;
        if provider_changed {
            match load_episodes(&engine, &resolved, include_fillers).await {
                Ok(reloaded) => {
                    all = reloaded;
                    current = episode_position(&all, actual_episode.number)
                        .unwrap_or_else(|| current.min(all.len().saturating_sub(1)));
                }
                Err(error) => {
                    tracing::warn!(%error, "could not refresh episodes after provider fallback");
                }
            }
        }

        let Some(mut stream) =
            best_stream(&streams, quality_override.as_deref(), &default_quality).cloned()
        else {
            eprintln!(
                "No playable streams for {} {}.",
                resolved.anime.title,
                episode_label(actual_episode.number)
            );
            let next = all.get(current + 1);
            let previous = current.checked_sub(1).and_then(|index| all.get(index));
            match choose_action(&all[current], None, next, previous, false, false)? {
                PlaybackAction::Next => {
                    current = (current + 1).min(all.len().saturating_sub(1));
                }
                PlaybackAction::Previous => {
                    current = current.saturating_sub(1);
                }
                PlaybackAction::PickEpisode => {
                    if let Some(choice) = pick_episode(&all, "Select another episode", current)? {
                        current = choice;
                    }
                }
                PlaybackAction::Replay
                | PlaybackAction::Download
                | PlaybackAction::ChangeQuality => {}
                PlaybackAction::Quit => return Ok(()),
            }
            continue;
        };

        let mut need_launch = true;
        loop {
            if need_launch {
                if let Err(error) = launch_stream(
                    &engine,
                    &config,
                    &resolved,
                    &actual_episode,
                    &stream,
                    &player_name,
                    &extra_player_args,
                    windows_mpv,
                )
                .await
                {
                    eprintln!("Player failed: {error}");
                }
                need_launch = false;
            }

            let next = all.get(current + 1);
            let previous = current.checked_sub(1).and_then(|index| all.get(index));
            let can_change_quality = distinct_qualities(&streams).len() > 1;
            match choose_action(
                &actual_episode,
                stream.quality,
                next,
                previous,
                true,
                can_change_quality,
            )? {
                // Re-resolve on replay so signed/temporary URLs are refreshed.
                PlaybackAction::Replay => break,
                PlaybackAction::Download => {
                    download_current(&engine, &config, &resolved, &actual_episode, &stream).await;
                }
                PlaybackAction::Next => {
                    current = (current + 1).min(all.len().saturating_sub(1));
                    break;
                }
                PlaybackAction::Previous => {
                    current = current.saturating_sub(1);
                    break;
                }
                PlaybackAction::ChangeQuality => {
                    if let Some(changed) = choose_quality(&streams, stream.quality)? {
                        quality_override = Some(changed);
                        if let Some(chosen) =
                            best_stream(&streams, quality_override.as_deref(), &default_quality)
                        {
                            stream = chosen.clone();
                            need_launch = true;
                        }
                    }
                }
                PlaybackAction::PickEpisode => {
                    if let Some(chosen) = pick_episode(&all, "Select another episode", current)? {
                        current = chosen;
                        break;
                    }
                }
                PlaybackAction::Quit => return Ok(()),
            }
        }
    }
}

/// Resolves streams for one episode and applies the same health/fallback policy
/// everywhere in the command.
async fn resolve_for_playback(
    engine: &Engine,
    resolved: Resolved,
    episode: Episode,
) -> Option<(Resolved, Episode, Vec<Stream>)> {
    let provider_name = resolved.provider.name();
    let episode_number = episode.number;
    let attempt = match tokio::time::timeout(STREAM_TIMEOUT, engine.streams(&resolved, &episode))
        .await
    {
        Ok(Ok(streams)) if !streams.is_empty() => Ok(streams),
        Ok(Ok(_)) => Err("returned no streams"),
        Ok(Err(error)) => {
            eprintln!(
                "{provider_name} stream resolution failed: {error}; trying another provider..."
            );
            mark_provider_degraded(resolved.provider.id());
            return fallback_for(engine, resolved, episode_number, provider_name).await;
        }
        Err(_) => {
            eprintln!("{provider_name} stream resolution timed out; trying another provider...");
            mark_provider_degraded(resolved.provider.id());
            return fallback_for(engine, resolved, episode_number, provider_name).await;
        }
    };

    match attempt {
        Ok(streams) => {
            mark_provider_healthy(resolved.provider.id());
            Some((resolved, episode, streams))
        }
        Err(reason) => {
            mark_provider_degraded(resolved.provider.id());
            eprintln!(
                "{provider_name} {reason} for {}; trying another provider...",
                episode_label(episode_number)
            );
            fallback_for(engine, resolved, episode_number, provider_name).await
        }
    }
}

async fn fallback_for(
    engine: &Engine,
    resolved: Resolved,
    episode_number: f32,
    provider_name: &str,
) -> Option<(Resolved, Episode, Vec<Stream>)> {
    match fallback_stream(
        engine,
        &resolved.anime.title,
        resolved.provider.id(),
        episode_number,
    )
    .await
    {
        Some((fallback_resolved, fallback_episode, streams)) => {
            mark_provider_healthy(fallback_resolved.provider.id());
            eprintln!("Using {} instead.", fallback_resolved.provider.name());
            Some((fallback_resolved, fallback_episode, streams))
        }
        None => {
            eprintln!(
                "{provider_name} has no fallback for {} {}.",
                resolved.anime.title,
                episode_label(episode_number)
            );
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn launch_stream(
    engine: &Engine,
    config: &Config,
    resolved: &Resolved,
    episode: &Episode,
    stream: &Stream,
    player_name: &str,
    extra_player_args: &[String],
    windows_mpv: bool,
) -> Result<()> {
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

    // Windows mpv cannot read Linux temp paths. Pass subtitle URLs directly
    // instead of downloading files it cannot open.
    let mut subtitle_args = Vec::new();
    let subtitles = if windows_mpv {
        subtitle_args.extend(
            preferred_subtitles(&stream.subtitles, 4)
                .into_iter()
                .map(|subtitle| format!("--sub-file={}", subtitle.url)),
        );
        Vec::new()
    } else {
        download_subtitles(engine, stream, &title).await?
    };
    eprintln!(
        "Playing {} [{}] {}",
        title,
        episode_label(episode.number),
        stream.display()
    );

    let mut args = config.player_args.clone();
    args.extend(extra_player_args.iter().cloned());
    args.extend(subtitle_args);
    // AnimeParadise serves HLS segments as `/ts?url=...`, which recent FFmpeg
    // versions reject because the URL has no extension. These demuxer options
    // are safe for regular HLS too.
    if url.contains("stream.animeparadise.moe") {
        args.push("--demuxer-lavf-o=allowed_segment_extensions=ALL,extension_picky=0".to_string());
    }
    let headers = stream.headers.clone();
    let player_name = player_name.to_string();
    let title = title.clone();
    let subtitles = subtitles.clone();
    tokio::task::spawn_blocking(move || {
        player::launch(&player_name, &url, &title, &headers, &subtitles, &args)
    })
    .await
    .map_err(|error| anyhow::anyhow!("player task failed: {error}"))?
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(())
}

async fn download_current(
    engine: &Engine,
    config: &Config,
    resolved: &Resolved,
    episode: &Episode,
    stream: &Stream,
) {
    let options = DownloadOptions {
        ffmpeg: config.ffmpeg.clone(),
        merge_video: true,
        merge_subtitles: config.merge_subtitles,
        xor_mask: Vec::new(),
        progress: None,
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
        &config.download_dir,
        &options,
    )
    .await
    {
        Ok(result) => {
            println!("{}", result.path.display());
            for subtitle in result.subtitles {
                println!("  subtitle: {}", subtitle.display());
            }
        }
        Err(error) => eprintln!("Download failed for episode {}: {error}", episode.number),
    }
}

fn choose_action(
    episode: &Episode,
    quality: Option<u32>,
    next: Option<&Episode>,
    previous: Option<&Episode>,
    can_download: bool,
    can_change_quality: bool,
) -> Result<PlaybackAction> {
    let mut items: Vec<(PlaybackAction, String)> =
        vec![(PlaybackAction::Replay, "Replay this episode".to_string())];
    if can_download {
        items.push((
            PlaybackAction::Download,
            "Download this episode instead".to_string(),
        ));
    }
    if let Some(next) = next {
        items.push((
            PlaybackAction::Next,
            format!("Next: {}", next.display_title()),
        ));
    }
    if let Some(previous) = previous {
        items.push((
            PlaybackAction::Previous,
            format!("Previous: {}", previous.display_title()),
        ));
    }
    if can_change_quality {
        items.push((PlaybackAction::ChangeQuality, "Change quality".to_string()));
    }
    items.push((
        PlaybackAction::PickEpisode,
        "Pick another episode".to_string(),
    ));
    items.push((PlaybackAction::Quit, "Quit".to_string()));

    let labels: Vec<String> = items.iter().map(|(_, label)| label.clone()).collect();
    let prompt = match quality {
        Some(quality) => format!(
            "Finished {} — what next? (played {quality}p)",
            episode.display_title()
        ),
        None => format!("Finished {} — what next?", episode.display_title()),
    };
    match FuzzySelect::new()
        .with_prompt(prompt)
        .items(&labels)
        .default(0)
        .interact()
    {
        Ok(choice) => Ok(items[choice].0),
        Err(_) => Ok(PlaybackAction::Quit),
    }
}

fn choose_quality(streams: &[Stream], current: Option<u32>) -> Result<Option<String>> {
    let qualities = distinct_qualities(streams);
    if qualities.len() < 2 {
        eprintln!("No other quality variants are available for this episode.");
        return Ok(None);
    }
    let labels: Vec<String> = qualities
        .iter()
        .map(|quality| format!("{quality}p"))
        .collect();
    let default = current
        .and_then(|quality| qualities.iter().position(|value| *value == quality))
        .unwrap_or(0);
    let prompt = match current {
        Some(quality) => format!("Select quality (current: {quality}p)"),
        None => "Select quality".to_string(),
    };
    match FuzzySelect::new()
        .with_prompt(prompt)
        .items(&labels)
        .default(default)
        .interact()
    {
        Ok(choice) => Ok(Some(qualities[choice].to_string())),
        Err(_) => Ok(None),
    }
}

fn distinct_qualities(streams: &[Stream]) -> Vec<u32> {
    let mut qualities: Vec<u32> = streams.iter().filter_map(|stream| stream.quality).collect();
    qualities.sort_unstable_by(|left, right| right.cmp(left));
    qualities.dedup();
    qualities
}

fn episode_position(episodes: &[Episode], number: f32) -> Option<usize> {
    episodes
        .iter()
        .position(|episode| (episode.number - number).abs() < 0.001)
}

async fn download_subtitles(engine: &Engine, stream: &Stream, title: &str) -> Result<Vec<PathBuf>> {
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
    use ap0dl_core::models::Audio;

    fn subtitle(language: &str, default: bool) -> Subtitle {
        Subtitle {
            url: format!("https://example.test/{language}.srt"),
            lang: language.to_string(),
            format: Some("srt".to_string()),
            default,
        }
    }

    fn stream(quality: Option<u32>) -> Stream {
        Stream {
            url: "https://example.test/video.m3u8".to_string(),
            container: None,
            quality,
            title: None,
            audio: Audio::Sub,
            server: None,
            subtitles: Vec::new(),
            chapters: Vec::new(),
            headers: Default::default(),
            kind: ap0dl_core::models::StreamKind::Hls,
            flix: None,
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

    #[test]
    fn quality_choices_are_unique_and_descending() {
        let streams = vec![
            stream(Some(720)),
            stream(Some(1080)),
            stream(Some(1080)),
            stream(None),
        ];
        assert_eq!(distinct_qualities(&streams), vec![1080, 720]);
    }
}
