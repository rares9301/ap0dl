use std::collections::BTreeMap;

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use url::Url;

use crate::config::KissKhConfig;
use crate::error::{Error, Result};
use crate::http::{Http, headers};
use crate::models::{Anime, Audio, Episode, SearchResult, Stream, StreamKind, Subtitle};
use crate::provider::{Provider, ProviderStatus};
use crate::providers::{infer_quality, parse_hls_variants};
use crate::util;

/// KissKH. Search/details/episodes are public JSON endpoints; episode video
/// URLs require a short-lived key from a public Google Apps Script endpoint,
/// exactly as the Aniyomi extension does.
pub struct KissKH {
    base: String,
    video_key_url: String,
    subtitle_key_url: String,
}

impl KissKH {
    pub fn new(config: &KissKhConfig) -> Result<Self> {
        let base = config.base_url.trim_end_matches('/').to_string();
        Url::parse(&base).map_err(|error| Error::Url(base.clone(), error))?;
        Ok(Self {
            base,
            video_key_url: config.video_key_url.clone(),
            subtitle_key_url: config.subtitle_key_url.clone(),
        })
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "application/json, text/plain, */*"),
            ("Referer", &format!("{}/", self.base)),
        ])
    }

    fn video_headers(&self) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "*/*"),
            ("Referer", &format!("{}/", self.base)),
            ("Origin", &self.base),
        ])
    }

    fn watch_url(&self, drama: &Drama, episode: Option<&KissEpisode>) -> String {
        let slug = slugify(&drama.title);
        match episode {
            Some(episode) => format!(
                "{}/drama/{}?id={}&ep={}",
                self.base,
                slug,
                drama.id,
                episode.id.unwrap_or(0)
            ),
            None => format!("{}/drama/{}?id={}", self.base, slug, drama.id),
        }
    }

    async fn fetch_drama(&self, http: &Http, id: i64) -> Result<Drama> {
        let url = format!("{}/api/DramaList/Drama/{id}?isq=false", self.base);
        http.get_json_cached(&url, &self.headers(), std::time::Duration::from_secs(600))
            .await
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
                format!("could not find a KissKH drama id in {value:?}"),
            )
        })
    }

    async fn fetch_key(&self, http: &Http, prefix: &str, episode_id: &str) -> Result<String> {
        let url = format!("{prefix}{episode_id}&version=2.8.10");
        let response: KeyDto = http
            .get_json_cached(&url, &self.headers(), std::time::Duration::from_secs(1800))
            .await?;
        Ok(response.key)
    }

    async fn fetch_episode(&self, http: &Http, id: &str) -> Result<KissVideo> {
        let key = self.fetch_key(http, &self.video_key_url, id).await?;
        let url = format!(
            "{}/api/DramaList/Episode/{id}.png?err=false&ts=&time=&kkey={}",
            self.base,
            urlencoding::encode(&key)
        );
        http.get_json(&url, &self.headers()).await
    }

    async fn fetch_subtitles(&self, http: &Http, id: &str) -> Vec<Subtitle> {
        let Ok(key) = self.fetch_key(http, &self.subtitle_key_url, id).await else {
            return Vec::new();
        };
        let url = format!(
            "{}/api/Sub/{id}?kkey={}",
            self.base,
            urlencoding::encode(&key)
        );
        let Ok(items) = http
            .get_json::<Vec<SubtitleDto>>(&url, &self.headers())
            .await
        else {
            return Vec::new();
        };
        items
            .into_iter()
            .filter_map(|item| {
                let src = item.src?;
                // Encrypted .txt subtitles need the KissKH AES keys; leave
                // them out rather than handing a player an unreadable file.
                if src.contains(".txt") || src.trim().is_empty() {
                    return None;
                }
                Some(Subtitle {
                    url: src,
                    lang: item.label.unwrap_or_else(|| "Unknown".to_string()),
                    format: None,
                    default: false,
                })
            })
            .collect()
    }
}

#[async_trait]
impl Provider for KissKH {
    fn id(&self) -> &'static str {
        "kisskh"
    }

    fn name(&self) -> &'static str {
        "KissKH"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "Public JSON API; episode keys via Apps Script; HLS/CDN streams."
    }

    fn matches(&self, url: &Url) -> bool {
        url.host_str()
            .is_some_and(|host| host.to_ascii_lowercase().starts_with("kisskh."))
    }

    async fn search(&self, http: &Http, query: &str, _page: u32) -> Result<Vec<SearchResult>> {
        let mut url = Url::parse(&format!("{}/api/DramaList/Search", self.base))
            .map_err(|error| Error::Url(self.base.clone(), error))?;
        url.query_pairs_mut()
            .append_pair("q", query.trim())
            .append_pair("type", "0");
        let items: Vec<SearchItem> = http.get_json(url.as_str(), &self.headers()).await?;
        Ok(items
            .into_iter()
            .map(|item| item.into_result(self))
            .collect())
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Anime> {
        let id = self.id_from_url(url)?;
        let drama = self.fetch_drama(http, id).await?;
        Ok(drama.into_anime(self))
    }

    async fn episodes(&self, http: &Http, anime: &Anime) -> Result<Vec<Episode>> {
        let id = self.id_from_url(&anime.url)?;
        let drama = self.fetch_drama(http, id).await?;
        let mut episodes: Vec<Episode> = drama
            .episodes
            .iter()
            .filter_map(|episode| {
                let number = episode.number?;
                Some(Episode {
                    number,
                    title: None,
                    id: episode.id?.to_string(),
                    url: self.watch_url(&drama, Some(episode)),
                    filler: false,
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
        let video = self.fetch_episode(http, &episode.id).await?;
        let Some(video_url) = video.video.filter(|url| !url.trim().is_empty()) else {
            return Err(Error::provider(
                self.id(),
                "KissKH returned no video URL for this episode",
            ));
        };
        let video_url = normalize_url(&self.base, &video_url);
        let subtitles = self.fetch_subtitles(http, &episode.id).await;
        let audio = match video.data_type.as_deref().unwrap_or_default() {
            "dub" | "Dub" => Audio::Dub,
            "sub" | "Sub" => Audio::Sub,
            _ => Audio::Both,
        };
        let mut stream_headers = BTreeMap::new();
        stream_headers.insert("Referer".to_string(), format!("{}/", self.base));
        stream_headers.insert("Origin".to_string(), self.base.clone());

        if video_url.contains(".m3u8") {
            let text = match http.get_text(&video_url, &self.video_headers()).await {
                Ok(text) => text,
                Err(error) => {
                    tracing::warn!(%error, "kisskh playlist fetch failed");
                    String::new()
                }
            };
            tracing::debug!(len = text.len(), sample = %text.chars().take(120).collect::<String>(), "kisskh playlist");
            let variants = parse_hls_variants(&text, &video_url);
            let variants = if variants.is_empty() {
                vec![(
                    video_url.clone(),
                    infer_quality(&video_url).or_else(|| infer_quality(&text)),
                )]
            } else {
                variants
            };
            let mut streams = Vec::new();
            for (url, quality) in variants {
                let title = match quality {
                    Some(quality) => format!("KissKH {quality}p"),
                    None => "KissKH".to_string(),
                };
                streams.push(Stream {
                    url,
                    quality,
                    title: Some(title),
                    audio,
                    server: Some("KissKH".to_string()),
                    subtitles: subtitles.clone(),
                    chapters: Vec::new(),
                    headers: stream_headers.clone(),
                    kind: StreamKind::Hls,
                    flix: None,
                });
            }
            streams.sort_by_key(|stream| std::cmp::Reverse(stream.quality));
            return Ok(streams);
        }

        Ok(vec![Stream {
            url: video_url.clone(),
            quality: infer_quality(&video_url),
            title: Some("KissKH".to_string()),
            audio,
            server: Some("KissKH".to_string()),
            subtitles,
            chapters: Vec::new(),
            headers: stream_headers,
            kind: StreamKind::Direct,
            flix: None,
        }])
    }

    async fn latest(&self, http: &Http, page: u32) -> Result<Vec<SearchResult>> {
        let url = format!(
            "{}/api/DramaList/List?page={}&type=0&sub=0&country=0&status=0&order=2&pageSize=40",
            self.base,
            page.max(1)
        );
        let response: ListResponse = http.get_json(&url, &self.headers()).await?;
        Ok(response
            .data
            .into_iter()
            .map(|item| item.into_result(self))
            .collect())
    }
}

#[derive(Debug, Deserialize, Default)]
struct ListResponse {
    #[serde(default)]
    data: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default, rename = "episodesCount")]
    episodes_count: Option<u32>,
}

impl SearchItem {
    fn into_result(self, provider: &KissKH) -> SearchResult {
        SearchResult {
            id: self.id.to_string(),
            title: self.title.clone(),
            url: format!(
                "{}/drama/{}?id={}",
                provider.base,
                slugify(&self.title),
                self.id
            ),
            provider: provider.id().to_string(),
            cover: self.thumbnail,
            year: None,
            format: None,
            episodes: self.episodes_count,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct Drama {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default, rename = "episodesCount")]
    episodes_count: Option<u32>,
    #[serde(default, rename = "releaseDate")]
    release_date: Option<String>,
    #[serde(default)]
    episodes: Vec<KissEpisode>,
}

impl Drama {
    fn into_anime(self, provider: &KissKH) -> Anime {
        let url = provider.watch_url(&self, None);
        Anime {
            id: self.id.to_string(),
            title: self.title,
            url,
            provider: provider.id().to_string(),
            cover: self.thumbnail,
            description: self.description.map(|value| util::strip_html(&value)),
            genres: Vec::new(),
            year: parse_year(self.release_date.as_deref()),
            format: self.r#type,
            status: self.status,
            total_episodes: self.episodes_count,
            anilist_id: None,
            mal_id: None,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct KissEpisode {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    number: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct KissVideo {
    #[serde(default, rename = "Video")]
    video: Option<String>,
    #[serde(default, rename = "dType")]
    data_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KeyDto {
    key: String,
}

#[derive(Debug, Deserialize)]
struct SubtitleDto {
    #[serde(default)]
    src: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

fn normalize_url(base: &str, value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else if value.starts_with("//") {
        format!("https:{value}")
    } else if value.starts_with('/') {
        format!("{base}{value}")
    } else {
        value.to_string()
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn trailing_number(value: &str) -> Option<i64> {
    static TRAILING: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let regex = TRAILING.get_or_init(|| Regex::new(r"(\d+)(?:[^0-9]*)$").unwrap());
    regex
        .captures(value)
        .and_then(|captures| captures.get(1)?.as_str().parse().ok())
}

fn parse_year(value: Option<&str>) -> Option<u32> {
    value
        .and_then(|value| value.get(0..4))
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids_and_slugs() {
        let provider = KissKH::new(&KissKhConfig::default()).unwrap();
        assert_eq!(
            provider
                .id_from_url("https://kisskh.co/drama/bocchi-the-rock?id=6253")
                .unwrap(),
            6253
        );
        assert_eq!(slugify("Bocchi the Rock!"), "bocchi-the-rock");
    }

    #[test]
    fn parses_years() {
        assert_eq!(parse_year(Some("2022-01-01T02:00:00")), Some(2022));
        assert_eq!(parse_year(Some("n/a")), None);
    }

    #[test]
    fn parses_1080p_variants() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:RESOLUTION=1920x1080\nv1080.m3u8\n#EXT-X-STREAM-INF:RESOLUTION=1280x720\nv720.m3u8\n";
        let variants = parse_hls_variants(text, "https://cdn.test/master.m3u8");
        assert_eq!(variants[0].1, Some(1080));
    }
}
