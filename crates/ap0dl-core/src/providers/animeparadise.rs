use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Deserialize;
use url::Url;

use crate::config::AnimeParadiseConfig;
use crate::error::{Error, Result};
use crate::http::{Http, headers};
use crate::models::{Anime, Audio, Episode, SearchResult, Stream, StreamKind, Subtitle};
use crate::provider::{Provider, ProviderStatus, host_matches};
use crate::providers::parse_hls_variants;

/// AnimeParadise is a small, API-backed catalogue. Its public JSON API
/// exposes search, details and episodes; playback is an HMAC-like opaque
/// stream token proxied through stream.animeparadise.moe as HLS.
pub struct AnimeParadise {
    base: String,
    api: String,
    stream: String,
}

impl AnimeParadise {
    pub fn new(config: &AnimeParadiseConfig) -> Result<Self> {
        for value in [&config.base_url, &config.api_url, &config.stream_url] {
            Url::parse(value.trim_end_matches('/'))
                .map_err(|error| Error::Url(value.clone(), error))?;
        }
        Ok(Self {
            base: config.base_url.trim_end_matches('/').to_string(),
            api: config.api_url.trim_end_matches('/').to_string(),
            stream: config.stream_url.trim_end_matches('/').to_string(),
        })
    }

    fn api_headers(&self) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "application/json, text/plain, */*"),
            ("Origin", &self.base),
            ("Referer", &format!("{}/", self.base)),
        ])
    }

    fn anime_url(&self, slug: &str) -> String {
        format!("{}/anime/{slug}", self.base)
    }

    fn slug_from_query(&self, query: &str) -> String {
        query
            .trim()
            .to_ascii_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }

    async fn search_item(&self, http: &Http, slug: &str) -> Option<SearchResult> {
        let url = format!("{}/anime/{slug}", self.api);
        let response: Envelope<DetailsItem> =
            http.get_json(&url, &self.api_headers()).await.ok()?;
        response.data?.into_search_result(self)
    }

    async fn playlist(&self, http: &Http, episode: &Episode) -> Result<Vec<Stream>> {
        let (uid, origin) = episode
            .id
            .split_once('|')
            .ok_or_else(|| Error::provider(self.id(), "invalid AnimeParadise episode id"))?;
        let mut url = Url::parse(&format!("{}/ep/{uid}", self.api))
            .map_err(|error| Error::Url(self.api.clone(), error))?;
        url.query_pairs_mut().append_pair("origin", origin);
        let response: Envelope<EpisodeData> = http
            .get_json(url.as_str(), &self.api_headers())
            .await
            .map_err(|error| Error::provider(self.id(), error))?;
        let data = response
            .data
            .ok_or_else(|| Error::provider(self.id(), "AnimeParadise episode had no data"))?;
        let episode_data = data.episode;
        let stream_link = episode_data
            .stream_link
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| Error::provider(self.id(), "AnimeParadise returned no stream token"))?;

        let mut playlist_url = Url::parse(&format!("{}/m3u8", self.stream))
            .map_err(|error| Error::Url(self.stream.clone(), error))?;
        playlist_url
            .query_pairs_mut()
            .append_pair("url", &stream_link);
        let playlist_url = playlist_url.to_string();
        let stream_headers = headers(&[
            ("Accept", "*/*"),
            ("Origin", &self.base),
            ("Referer", &format!("{}/", self.base)),
        ]);
        let master = http.get_text(&playlist_url, &stream_headers).await?;
        let variants = parse_hls_variants(&master, &playlist_url);
        let subtitles: Vec<Subtitle> = episode_data
            .sub_data
            .unwrap_or_default()
            .into_iter()
            .map(|subtitle| Subtitle {
                url: if subtitle.src.starts_with("http://") || subtitle.src.starts_with("https://")
                {
                    subtitle.src
                } else {
                    format!("{}/stream/file/{}", self.api, subtitle.src)
                },
                lang: subtitle.label,
                format: Some(subtitle.kind),
                default: false,
            })
            .collect();

        let mut streams = Vec::new();
        if variants.is_empty() {
            streams.push(Stream {
                url: playlist_url.clone(),
                container: None,
                quality: None,
                title: Some("AnimeParadise".to_string()),
                audio: Audio::Sub,
                server: Some("AnimeParadise".to_string()),
                subtitles: subtitles.clone(),
                chapters: Vec::new(),
                headers: header_map(&stream_headers),
                kind: StreamKind::Hls,
                flix: None,
            });
        } else {
            for (variant_url, quality) in variants {
                streams.push(Stream {
                    url: variant_url,
                    container: None,
                    quality,
                    title: Some(match quality {
                        Some(quality) => format!("AnimeParadise {quality}p"),
                        None => "AnimeParadise".to_string(),
                    }),
                    audio: Audio::Sub,
                    server: Some("AnimeParadise".to_string()),
                    subtitles: subtitles.clone(),
                    chapters: Vec::new(),
                    headers: header_map(&stream_headers),
                    kind: StreamKind::Hls,
                    flix: None,
                });
            }
        }
        streams.sort_by_key(|stream| std::cmp::Reverse(stream.quality));
        Ok(streams)
    }
}

#[async_trait]
impl Provider for AnimeParadise {
    fn id(&self) -> &'static str {
        "animeparadise"
    }

    fn name(&self) -> &'static str {
        "AnimeParadise"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "Public JSON catalogue with opaque-token HLS playback and 1080p variants."
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "animeparadise.moe")
    }

    async fn search(&self, http: &Http, query: &str, page: u32) -> Result<Vec<SearchResult>> {
        let mut url = Url::parse(&format!("{}/search", self.api))
            .map_err(|error| Error::Url(self.api.clone(), error))?;
        url.query_pairs_mut()
            .append_pair("sort", "POPULARITY")
            .append_pair("page", &page.max(1).to_string())
            .append_pair("q", query.trim())
            .append_pair("v", "1");
        let response: Envelope<Vec<SearchItem>> = http
            .get_json(url.as_str(), &self.api_headers())
            .await
            .map_err(|error| Error::provider(self.id(), error))?;
        let mut results: Vec<SearchResult> = response
            .data
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| item.into_search_result(self))
            .collect();
        if results.is_empty() && !query.trim().is_empty() {
            // A handful of catalogue entries (notably older Netflix shows)
            // are absent from the search index but are addressable by slug.
            let slug = self.slug_from_query(query);
            if !slug.is_empty() {
                if let Some(result) = self.search_item(http, &slug).await {
                    results.push(result);
                }
            }
        }
        Ok(results)
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Anime> {
        let slug = Url::parse(url)
            .ok()
            .and_then(|url| {
                url.path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .map(str::to_string)
            })
            .filter(|slug| !slug.is_empty())
            .unwrap_or_else(|| self.slug_from_query(url));
        let api_url = format!("{}/anime/{slug}", self.api);
        let response: Envelope<DetailsItem> = http
            .get_json(&api_url, &self.api_headers())
            .await
            .map_err(|error| Error::provider(self.id(), error))?;
        let details = response.data.ok_or_else(|| {
            Error::provider(self.id(), format!("AnimeParadise has no anime {slug:?}"))
        })?;
        Ok(details.into_anime(self, &slug))
    }

    async fn episodes(&self, http: &Http, anime: &Anime) -> Result<Vec<Episode>> {
        let url = format!("{}/anime/{}/episode", self.api, anime.id);
        let response: Envelope<Vec<EpisodeItem>> =
            http.get_json(&url, &self.api_headers())
                .await
                .map_err(|error| Error::provider(self.id(), error))?;
        let mut episodes: Vec<Episode> = response
            .data
            .unwrap_or_default()
            .into_iter()
            .filter_map(|episode| {
                let number = episode.number?.parse::<f32>().ok()?;
                Some(Episode {
                    number,
                    title: episode.title.filter(|title| !title.trim().is_empty()),
                    id: format!("{}|{}", episode.uid, episode.origin),
                    url: format!(
                        "{}/watch/{}?origin={}",
                        self.base, episode.uid, episode.origin
                    ),
                    filler: false,
                    recap: false,
                    aired: None,
                    audio: Audio::Sub,
                })
            })
            .collect();
        episodes.sort_by(|left, right| left.number.total_cmp(&right.number));
        Ok(episodes)
    }

    async fn streams(&self, http: &Http, _anime: &Anime, episode: &Episode) -> Result<Vec<Stream>> {
        self.playlist(http, episode).await
    }
}

fn header_map(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    #[serde(rename = "_id")]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default, rename = "posterImage")]
    poster_image: Option<Poster>,
    #[serde(default, rename = "alternativeTitle")]
    alternative_title: Option<Titles>,
    #[serde(default, rename = "animeSeason")]
    anime_season: Option<Season>,
    #[serde(default, rename = "startDate")]
    start_date: Option<String>,
    #[serde(default, rename = "episodeCount")]
    episode_count: Option<u32>,
}

impl SearchItem {
    fn into_search_result(self, provider: &AnimeParadise) -> Option<SearchResult> {
        if self.link.is_empty() {
            return None;
        }
        let title = self
            .alternative_title
            .as_ref()
            .and_then(|titles| titles.english.clone())
            .or_else(|| {
                self.alternative_title
                    .as_ref()
                    .and_then(|titles| titles.romaji.clone())
            })
            .unwrap_or(self.title);
        Some(SearchResult {
            id: self.id,
            url: provider.anime_url(&self.link),
            title,
            provider: provider.id().to_string(),
            cover: self.poster_image.and_then(Poster::url),
            year: self
                .anime_season
                .and_then(|season| season.year)
                .or_else(|| parse_year(self.start_date.as_deref())),
            format: None,
            episodes: self.episode_count,
        })
    }
}

#[derive(Debug, Deserialize)]
struct DetailsItem {
    #[serde(rename = "_id")]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default, rename = "posterImage")]
    poster_image: Option<Poster>,
    #[serde(default, rename = "alternativeTitle")]
    alternative_title: Option<Titles>,
    #[serde(default)]
    synopsys: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "episodeCount")]
    episode_count: Option<u32>,
    #[serde(default, rename = "animeSeason")]
    anime_season: Option<Season>,
    #[serde(default, rename = "startDate")]
    start_date: Option<String>,
}

impl DetailsItem {
    fn into_search_result(self, provider: &AnimeParadise) -> Option<SearchResult> {
        if self.link.is_empty() {
            return None;
        }
        let title = self
            .alternative_title
            .as_ref()
            .and_then(|titles| titles.english.clone())
            .unwrap_or_else(|| self.title.clone());
        Some(SearchResult {
            id: self.id,
            url: provider.anime_url(&self.link),
            title,
            provider: provider.id().to_string(),
            cover: self.poster_image.and_then(Poster::url),
            year: self.anime_season.and_then(|season| season.year),
            format: None,
            episodes: self.episode_count,
        })
    }

    fn into_anime(self, provider: &AnimeParadise, slug: &str) -> Anime {
        let title = self
            .alternative_title
            .as_ref()
            .and_then(|titles| titles.english.clone())
            .unwrap_or(self.title.clone());
        Anime {
            id: self.id,
            title,
            url: provider.anime_url(slug),
            provider: provider.id().to_string(),
            cover: self.poster_image.and_then(Poster::url),
            description: self.synopsys,
            genres: self.genres,
            year: self
                .anime_season
                .and_then(|season| season.year)
                .or_else(|| parse_year(self.start_date.as_deref())),
            format: None,
            status: self.status,
            total_episodes: self.episode_count,
            anilist_id: None,
            mal_id: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EpisodeItem {
    #[serde(default)]
    uid: String,
    #[serde(default)]
    origin: String,
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EpisodeData {
    #[serde(default)]
    episode: StreamEpisode,
}

#[derive(Debug, Deserialize, Default)]
struct StreamEpisode {
    #[serde(default, rename = "streamLink")]
    stream_link: Option<String>,
    #[serde(default, rename = "subData")]
    sub_data: Option<Vec<SubtitleItem>>,
}

#[derive(Debug, Deserialize)]
struct SubtitleItem {
    #[serde(default)]
    src: String,
    #[serde(default)]
    label: String,
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct Poster {
    #[serde(default)]
    large: Option<String>,
    #[serde(default)]
    medium: Option<String>,
    #[serde(default)]
    small: Option<String>,
}

impl Poster {
    fn url(self) -> Option<String> {
        self.large.or(self.medium).or(self.small)
    }
}

#[derive(Debug, Deserialize)]
struct Titles {
    #[serde(default)]
    english: Option<String>,
    #[serde(default)]
    romaji: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Season {
    #[serde(default)]
    year: Option<u32>,
}

fn parse_year(value: Option<&str>) -> Option<u32> {
    let value = value?;
    let year = value.get(0..4)?;
    year.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugifies_search_fallbacks() {
        let provider = AnimeParadise::new(&AnimeParadiseConfig::default()).unwrap();
        assert_eq!(
            provider.slug_from_query("Cyberpunk: Edgerunners 2"),
            "cyberpunk-edgerunners-2"
        );
    }

    #[test]
    fn parses_search_payload() {
        let payload = r#"{
            "success": true,
            "data": [{
                "_id": "abc",
                "title": "Bocchi the Rock!",
                "link": "bocchi-the-rock",
                "episodeCount": 12,
                "startDate": "2022-10-09",
                "posterImage": {"large": "https://example.test/poster.jpg"},
                "animeSeason": {"year": 2022},
                "alternativeTitle": {"english": "BOCCHI THE ROCK!"}
            }]
        }"#;
        let envelope: Envelope<Vec<SearchItem>> = serde_json::from_str(payload).unwrap();
        let provider = AnimeParadise::new(&AnimeParadiseConfig::default()).unwrap();
        let results: Vec<_> = envelope
            .data
            .unwrap()
            .into_iter()
            .filter_map(|item| item.into_search_result(&provider))
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "BOCCHI THE ROCK!");
        assert_eq!(results[0].year, Some(2022));
        assert_eq!(results[0].episodes, Some(12));
    }

    #[test]
    fn parses_stream_episode_payload() {
        let payload = r#"{
            "data": {"episode": {
                "streamLink": "token",
                "subData": [{"src":"https://stream.example/captions", "label":"English", "type":"vtt"}]
            }}
        }"#;
        let envelope: Envelope<EpisodeData> = serde_json::from_str(payload).unwrap();
        let episode = envelope.data.unwrap().episode;
        assert_eq!(episode.stream_link.as_deref(), Some("token"));
        assert_eq!(episode.sub_data.unwrap()[0].label, "English");
    }
}
