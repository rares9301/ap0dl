use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use futures::future::join_all;
use regex::Regex;
use serde::Deserialize;
use url::Url;

use crate::config::AniDBConfig;
use crate::error::{Error, Result};
use crate::http::{Http, headers};
use crate::models::{Anime, Audio, Episode, SearchResult, Stream, StreamKind};
use crate::provider::{Provider, ProviderStatus, host_matches};
use crate::providers::{infer_quality, parse_hls_variants};

/// AniDB / Anila. The public site is a thin front-end over a JSON API; the
/// API returns posts, episode ids and iframe links, and those links are plain
/// HLS masters with 1080p/720p/360p variants.
pub struct AniDB {
    base: String,
    play: String,
    public: String,
}

impl AniDB {
    pub fn new(config: &AniDBConfig) -> Result<Self> {
        for value in [&config.base_url, &config.play_url] {
            Url::parse(value.trim_end_matches('/'))
                .map_err(|error| Error::Url(value.clone(), error))?;
        }
        Ok(Self {
            base: config.base_url.trim_end_matches('/').to_string(),
            play: config.play_url.trim_end_matches('/').to_string(),
            public: config.public_url.trim_end_matches('/').to_string(),
        })
    }

    fn api_headers(&self) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "application/json, text/plain, */*"),
            ("Referer", "https://anilab.so/"),
        ])
    }

    fn play_headers(&self) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "application/json, text/plain, */*"),
            ("Referer", "https://play.app/"),
            ("X-Requested-With", "PLAY"),
        ])
    }

    async fn fetch_post(&self, http: &Http, id: i64) -> Result<Post> {
        let url = format!("{}/api/post?id={id}", self.base);
        http.get_json_cached(
            &url,
            &self.api_headers(),
            std::time::Duration::from_secs(600),
        )
        .await
    }

    async fn posts_to_results(&self, http: &Http, posts: Vec<PostItem>) -> Vec<SearchResult> {
        let futures = posts.into_iter().take(24).map(|item| async move {
            let post = self.fetch_post(http, item.id).await.ok()?;
            Some(post.into_search_result(self, item.poster))
        });
        join_all(futures).await.into_iter().flatten().collect()
    }

    fn id_from_url(&self, value: &str) -> Result<i64> {
        if let Ok(url) = Url::parse(value) {
            if let Some((_, id)) = url.query_pairs().find(|(key, _)| key == "id") {
                if let Ok(id) = id.parse() {
                    return Ok(id);
                }
            }
            if let Some(segment) = url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
            {
                if let Some(id) = trailing_number(segment) {
                    return Ok(id);
                }
            }
        }
        trailing_number(value).ok_or_else(|| {
            Error::provider(
                self.id(),
                format!("could not find an AniDB post id in {value:?}"),
            )
        })
    }

    async fn master_variants(&self, http: &Http, link: &str) -> Result<Vec<(String, Option<u32>)>> {
        let text = http
            .get_text(
                link,
                &headers(&[
                    ("Accept", "*/*"),
                    ("Referer", "https://play.anidb.app/"),
                    ("Origin", "https://play.anidb.app"),
                ]),
            )
            .await?;
        let variants = parse_hls_variants(&text, link);
        if variants.is_empty() {
            Ok(vec![(link.to_string(), infer_quality(link))])
        } else {
            Ok(variants)
        }
    }
}

#[async_trait]
impl Provider for AniDB {
    fn id(&self) -> &'static str {
        "anidb"
    }

    fn name(&self) -> &'static str {
        "AniDB / Anila"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "JSON API + direct HLS masters; exposes 1080p, 720p and 360p variants."
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "anilab.so")
            || host_matches(url, "anidb.app")
            || host_matches(url, "amdapi.click")
    }

    async fn search(&self, http: &Http, query: &str, page: u32) -> Result<Vec<SearchResult>> {
        let mut url = Url::parse(&format!("{}/api/search", self.base))
            .map_err(|error| Error::Url(self.base.clone(), error))?;
        url.query_pairs_mut()
            .append_pair("query", query.trim())
            .append_pair("page", &page.max(1).to_string());
        let response: PostList = http.get_json(url.as_str(), &self.api_headers()).await?;
        Ok(self.posts_to_results(http, response.posts).await)
    }

    async fn latest(&self, http: &Http, page: u32) -> Result<Vec<SearchResult>> {
        let url = format!("{}/api/latest?page={}", self.base, page.max(1));
        let response: PostList = http.get_json(&url, &self.api_headers()).await?;
        Ok(self.posts_to_results(http, response.posts).await)
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Anime> {
        let id = self.id_from_url(url)?;
        let post = self.fetch_post(http, id).await?;
        Ok(post.into_anime(self))
    }

    async fn episodes(&self, http: &Http, anime: &Anime) -> Result<Vec<Episode>> {
        let id = self.id_from_url(&anime.url)?;
        let url = format!("{}/api/anime/{id}/episodes", self.play);
        let response: EpisodeList = http
            .get_json(&url, &self.play_headers())
            .await
            .map_err(|error| Error::provider(self.id(), error))?;
        let mut episodes: Vec<Episode> = response
            .list
            .into_iter()
            .filter_map(|episode| {
                let number = episode.number.parse::<f32>().ok()?;
                Some(Episode {
                    number,
                    title: episode.name.filter(|name| !name.trim().is_empty()),
                    id: episode.id.clone(),
                    url: format!("{}/watch/{}", self.public, episode.id),
                    filler: episode.filler,
                    recap: false,
                    aired: None,
                    audio: Audio::Both,
                })
            })
            .collect();
        episodes.sort_by(|a, b| a.number.total_cmp(&b.number));
        Ok(episodes)
    }

    async fn streams(&self, http: &Http, _anime: &Anime, episode: &Episode) -> Result<Vec<Stream>> {
        let url = format!("{}/api/episode/{}/servers", self.play, episode.id);
        let response: ServerList = http
            .get_json(&url, &self.play_headers())
            .await
            .map_err(|error| Error::provider(self.id(), error))?;

        let mut streams = Vec::new();
        let mut seen = HashSet::new();
        let mut last_error = None;

        for server in response.list {
            if !seen.insert(server.id.clone()) {
                continue;
            }
            let audio = match server.lang.as_deref().unwrap_or_default() {
                "dub" => Audio::Dub,
                "sub" => Audio::Sub,
                _ => Audio::Both,
            };
            let server_name = server.name.unwrap_or_else(|| "Server".to_string());
            let iframe_url = format!("{}/api/episode/{}/iframe", self.play, server.id);
            let iframe: Iframe = match http.get_json(&iframe_url, &self.play_headers()).await {
                Ok(iframe) => iframe,
                Err(error) => {
                    last_error = Some(error);
                    continue;
                }
            };
            if iframe.link.trim().is_empty() {
                continue;
            }

            let variants = match self.master_variants(http, &iframe.link).await {
                Ok(variants) => variants,
                Err(error) => {
                    last_error = Some(error);
                    continue;
                }
            };

            for (variant_url, quality) in variants {
                if !seen.insert(variant_url.clone()) {
                    continue;
                }
                let mut stream_headers = BTreeMap::new();
                stream_headers.insert("Referer".to_string(), "https://play.anidb.app/".to_string());
                stream_headers.insert("Origin".to_string(), "https://play.anidb.app".to_string());
                let title = match quality {
                    Some(quality) => format!("[{}] {server_name} {quality}p", audio.label()),
                    None => format!("[{}] {server_name}", audio.label()),
                };
                streams.push(Stream {
                    url: variant_url.clone(),
                    quality,
                    title: Some(title),
                    audio,
                    server: Some(server_name.clone()),
                    subtitles: Vec::new(),
                    chapters: Vec::new(),
                    headers: stream_headers,
                    kind: if variant_url.contains(".m3u8") {
                        StreamKind::Hls
                    } else {
                        StreamKind::Direct
                    },
                    flix: None,
                });
            }
        }

        if streams.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                Error::provider(self.id(), "AniDB returned no playable servers")
            }));
        }
        streams.sort_by_key(|stream| std::cmp::Reverse(stream.quality));
        Ok(streams)
    }
}

#[derive(Debug, Deserialize, Default)]
struct PostList {
    #[serde(default)]
    posts: Vec<PostItem>,
}

#[derive(Debug, Deserialize)]
struct PostItem {
    id: i64,
    #[serde(default)]
    poster: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Post {
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    poster: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    premiered: Option<String>,
    #[serde(default)]
    genres: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
}

impl Post {
    fn into_search_result(self, provider: &AniDB, fallback_poster: Option<String>) -> SearchResult {
        SearchResult {
            url: format!("{}/anime/{}", provider.public, self.id),
            title: self.title,
            id: self.id.to_string(),
            provider: provider.id().to_string(),
            cover: self.poster.or(fallback_poster),
            year: parse_year(self.premiered.as_deref()),
            format: self.r#type,
            episodes: None,
        }
    }

    fn into_anime(self, provider: &AniDB) -> Anime {
        Anime {
            id: self.id.to_string(),
            title: self.title,
            url: format!("{}/anime/{}", provider.public, self.id),
            provider: provider.id().to_string(),
            cover: self.poster,
            description: self.overview,
            genres: self
                .genres
                .map(|genres| {
                    genres
                        .split(',')
                        .map(str::trim)
                        .filter(|genre| !genre.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            year: parse_year(self.premiered.as_deref()),
            format: self.r#type,
            status: self.status,
            total_episodes: None,
            anilist_id: None,
            mal_id: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EpisodeList {
    #[serde(default)]
    list: Vec<EpisodeItem>,
}

#[derive(Debug, Deserialize)]
struct EpisodeItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    number: String,
    #[serde(default)]
    filler: bool,
}

#[derive(Debug, Deserialize)]
struct ServerList {
    #[serde(default)]
    list: Vec<ServerItem>,
}

#[derive(Debug, Deserialize)]
struct ServerItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Iframe {
    #[serde(default)]
    link: String,
}

fn trailing_number(value: &str) -> Option<i64> {
    static TRAILING: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let regex = TRAILING.get_or_init(|| Regex::new(r"(\d+)(?:[^0-9]*)$").unwrap());
    regex
        .captures(value)
        .and_then(|captures| captures.get(1)?.as_str().parse().ok())
}

fn parse_year(value: Option<&str>) -> Option<u32> {
    static YEAR: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let regex = YEAR.get_or_init(|| Regex::new(r"(\d{4})").unwrap());
    value
        .and_then(|value| regex.captures(value))
        .and_then(|captures| captures.get(1)?.as_str().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_ids_from_urls() {
        let provider = AniDB::new(&AniDBConfig::default()).unwrap();
        assert_eq!(
            provider
                .id_from_url("https://anilab.so/anime/1000000729")
                .unwrap(),
            1000000729
        );
        assert_eq!(
            provider
                .id_from_url("https://anilab.so/anime/bocchi-the-rock-1000000729")
                .unwrap(),
            1000000729
        );
    }

    #[test]
    fn parses_1080p_variants() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1,RESOLUTION=1920x1080\n1080.m3u8\n#EXT-X-STREAM-INF:RESOLUTION=1280x720\n720.m3u8\n";
        let variants = parse_hls_variants(text, "https://example.test/master.m3u8");
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].1, Some(1080));
        assert!(variants[0].0.ends_with("/1080.m3u8"));
    }

    #[test]
    fn parses_year() {
        assert_eq!(parse_year(Some("Fall 2022")), Some(2022));
        assert_eq!(parse_year(Some("unknown")), None);
    }
}
