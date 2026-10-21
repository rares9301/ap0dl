use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, Result};
use ap0dl_core::Config;
use ap0dl_core::download::header_map;
use ap0dl_core::models::{Episode, Subtitle};
use ap0dl_core::player;
use ap0dl_core::proxy::FlixProxy;
use dialoguer::{FuzzySelect, Input};
use futures::future::join_all;

use super::{
    best_stream, engine, episode_label, episode_selection, resolve_query, selected_episodes,
};

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
    let interactive =
        !no_interactive && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();

    let query = match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(query) => query.to_string(),
        None if interactive => Input::<String>::new()
            .with_prompt("Search anime")
            .interact_text()
            .context("search prompt cancelled")?,
        None => anyhow::bail!(
            "an anime query is required when stdin is not a terminal; pass a title, for example: ap0dl stream \"cyberpunk 2\""
        ),
    };
    if query.trim().is_empty() {
        anyhow::bail!("empty anime query");
    }

    let resolved = if let Some(index) = index {
        resolve_query(&engine, &query, provider, index).await?
    } else if !interactive {
        resolve_query(&engine, &query, provider, 1).await?
    } else {
        let hits = engine
            .search(provider, &query, 1, 40)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .with_context(|| format!("could not search for {query:?}"))?;
        if hits.is_empty() {
            anyhow::bail!("no results for {query:?}");
        }
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
        let (provider, result) = hits[choice].clone();
        engine.resolve_result(provider, result).await?
    };
    let episodes = engine
        .episodes(&resolved)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if episodes.is_empty() {
        anyhow::bail!("no episodes found for {}", resolved.anime.title);
    }

    let episodes = match range {
        Some(range) => {
            let selection = episode_selection(range)?;
            selected_episodes(&episodes, &selection, include_fillers)
        }
        None if interactive => {
            let visible: Vec<Episode> = episodes
                .into_iter()
                .filter(|episode| include_fillers || !episode.filler)
                .collect();
            if visible.is_empty() {
                anyhow::bail!("no episodes matched the selection");
            }
            let labels: Vec<String> = visible
                .iter()
                .map(|episode| episode.display_title())
                .collect();
            let choice = FuzzySelect::new()
                .with_prompt("Select episode (type to fuzzy-filter, Enter to play)")
                .items(&labels)
                .default(0)
                .interact()
                .context("episode selection cancelled")?;
            vec![visible[choice].clone()]
        }
        None => episodes
            .into_iter()
            .filter(|episode| include_fillers || !episode.filler)
            .take(1)
            .collect(),
    };

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
