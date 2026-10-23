use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use base64::Engine;
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::config::AnimeTokiConfig;
use crate::error::{Error, Result};
use crate::http::{Http, headers};
use crate::models::{Anime, Audio, Episode, SearchResult, Stream, StreamKind};
use crate::provider::{Provider, ProviderStatus, host_matches};
use crate::providers::infer_quality;
use crate::util;

/// AnimeToki publishes episode files through a small Cloudflare Worker file
/// browser. The catalogue is regular WordPress HTML, while the folder index
/// is a JSON POST and the final stream is a direct MKV (usually 1080p).
pub struct AnimeToki {
    base: String,
    cloud: String,
    drive: String,
}

#[derive(Debug, Deserialize)]
struct Folder {
    #[serde(default)]
    files: Vec<FolderFile>,
}

#[derive(Debug, Deserialize)]
struct FolderFile {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "mimeType")]
    mime_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudEpisode {
    folder: String,
    file_id: String,
    name: String,
}

impl AnimeToki {
    pub fn new(config: &AnimeTokiConfig) -> Result<Self> {
        for value in [&config.base_url, &config.cloud_url, &config.drive_url] {
            Url::parse(value.trim_end_matches('/'))
                .map_err(|error| Error::Url(value.clone(), error))?;
        }
        Ok(Self {
            base: config.base_url.trim_end_matches('/').to_string(),
            cloud: config.cloud_url.trim_end_matches('/').to_string(),
            drive: config.drive_url.trim_end_matches('/').to_string(),
        })
    }

    fn doc_headers(&self, referer: &str) -> reqwest::header::HeaderMap {
        headers(&[
            (
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
            ("Referer", referer),
        ])
    }

    fn folder_headers(&self, referer: &str) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "application/json, text/plain, */*"),
            ("Referer", referer),
            ("Origin", &self.base),
            ("X-Requested-With", "XMLHttpRequest"),
        ])
    }

    async fn warm_session(&self, http: &Http) {
        let referer = format!("{}/", self.base);
        let request_headers = self.doc_headers(&referer);
        let _ = http
            .get_text(&format!("{}/", self.base), &request_headers)
            .await;
        let _ = http
            .get_text(&format!("{}/", self.cloud), &request_headers)
            .await;
        let _ = http
            .get_text(&format!("{}/", self.drive), &request_headers)
            .await;
    }

    async fn cloud_files(
        &self,
        http: &Http,
        folder_url: &str,
        referer: &str,
    ) -> Result<Vec<FolderFile>> {
        let folder_url = normalize_folder_url(folder_url);
        let folder_url = folder_url.as_str();
        let request_headers = self.folder_headers(referer);
        let first = http.post_empty_text(folder_url, &request_headers).await;
        let (text, first_error) = match first {
            Ok(text) => match serde_json::from_str::<Folder>(&text) {
                Ok(folder) => return Ok(folder.files),
                Err(error) => (text, error.to_string()),
            },
            Err(error) => (String::new(), error.to_string()),
        };
        tracing::debug!(
            folder = %folder_url,
            preview = %text.chars().take(160).collect::<String>(),
            "AnimeToki folder index needs a session warm-up"
        );

        // The worker answers `Session Expired` until the catalogue and worker
        // origins have both set their short-lived cookies.
        self.warm_session(http).await;
        let text = http
            .post_empty_text(folder_url, &request_headers)
            .await
            .map_err(|error| {
                Error::provider(
                    self.id(),
                    format!("AnimeToki folder index failed ({first_error}); retry: {error}"),
                )
            })?;
        let folder: Folder = serde_json::from_str(&text).map_err(|error| {
            Error::provider(
                self.id(),
                format!(
                    "AnimeToki folder index returned non-JSON ({first_error}): {error}; body: {}",
                    text.chars().take(160).collect::<String>()
                ),
            )
        })?;
        Ok(folder.files)
    }

    fn parse_search(&self, html: &str) -> Vec<SearchResult> {
        let document = Html::parse_document(html);
        let Ok(item_selector) = Selector::parse("li.post-item") else {
            return Vec::new();
        };
        let Ok(title_selector) = Selector::parse("h2.post-title a") else {
            return Vec::new();
        };
        let Ok(image_selector) = Selector::parse("img") else {
            return Vec::new();
        };
        document
            .select(&item_selector)
            .filter_map(|item| {
                let link = item.select(&title_selector).next()?;
                let href = link.value().attr("href")?;
                let title = link.text().collect::<String>().trim().to_string();
                if title.is_empty() {
                    return None;
                }
                let url = util::absolutize(&format!("{}/", self.base), href);
                let id = url
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let cover = item
                    .select(&image_selector)
                    .next()
                    .and_then(|image| {
                        image
                            .value()
                            .attr("data-src")
                            .or_else(|| image.value().attr("src"))
                    })
                    .map(|value| util::absolutize(&format!("{}/", self.base), value));
                let year = util::last_number(&title).and_then(|number| {
                    (1900.0..=2100.0).contains(&number).then_some(number as u32)
                });
                Some(SearchResult {
                    id,
                    title,
                    url,
                    provider: self.id().to_string(),
                    cover,
                    year,
                    format: None,
                    episodes: None,
                })
            })
            .collect()
    }

    fn parse_details(&self, url: &str, html: &str) -> Anime {
        let document = Html::parse_document(html);
        let title = document
            .select(&Selector::parse("h1.post-title, h1.entry-title").unwrap())
            .next()
            .map(|element| element.text().collect::<String>().trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| {
                url.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("AnimeToki")
                    .replace('-', " ")
            });
        let cover = document
            .select(
                &Selector::parse(
                    "meta[property='og:image'], figure img, .single-featured-image img",
                )
                .unwrap(),
            )
            .next()
            .and_then(|element| {
                element
                    .value()
                    .attr("content")
                    .or_else(|| element.value().attr("data-src"))
                    .or_else(|| element.value().attr("src"))
            })
            .map(|value| util::absolutize(&format!("{}/", self.base), value));
        let description = document
            .select(&Selector::parse(".review-short-summary, .entry-content").unwrap())
            .next()
            .map(|element| element.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty());
        let genres = document
            .select(&Selector::parse("a.post-cat").unwrap())
            .map(|element| element.text().collect::<String>().trim().to_string())
            .filter(|genre| !genre.is_empty())
            .collect();
        Anime {
            id: url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string(),
            title,
            url: url.to_string(),
            provider: self.id().to_string(),
            cover,
            description,
            genres,
            year: None,
            format: None,
            status: None,
            total_episodes: None,
            anilist_id: None,
            mal_id: None,
        }
    }

    async fn episodes_from_folder(
        &self,
        http: &Http,
        folder_url: &str,
        referer: &str,
    ) -> Result<Vec<Episode>> {
        let files = self.cloud_files(http, folder_url, referer).await?;
        let mut episodes = Vec::new();
        let mut seen = HashSet::new();
        for file in files {
            if !is_video(&file) || !seen.insert(file.id.clone()) {
                continue;
            }
            let number = episode_number(&file.name).unwrap_or((episodes.len() + 1) as f32);
            let quality = infer_quality(&file.name);
            let episode = CloudEpisode {
                folder: folder_url.to_string(),
                file_id: file.id.clone(),
                name: file.name.clone(),
            };
            let encoded = serde_json::to_string(&episode).map_err(|error| {
                Error::provider(
                    self.id(),
                    format!("could not encode AnimeToki episode: {error}"),
                )
            })?;
            episodes.push(Episode {
                number,
                title: Some(clean_name(&file.name)),
                id: format!("cloud:{encoded}"),
                url: download_url(&file.id, &file.name),
                filler: false,
                recap: false,
                aired: None,
                audio: Audio::Both,
            });
            if let Some(quality) = quality {
                tracing::debug!(file = %file.name, quality, "AnimeToki episode");
            }
        }
        episodes.sort_by(|left, right| left.number.total_cmp(&right.number));
        Ok(episodes)
    }
}

#[async_trait]
impl Provider for AnimeToki {
    fn id(&self) -> &'static str {
        "animetoki"
    }

    fn name(&self) -> &'static str {
        "AnimeToki"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> ProviderStatus {
        // Search, details and episode indexing are live. Individual AT-Cloud
        // files can still return a quota page under high traffic, so playback
        // is not guaranteed for every episode.
        ProviderStatus::Experimental
    }

    fn notes(&self) -> &'static str {
        "WordPress catalogue + Cloudflare Worker file index; 1080p MKVs. AT-Cloud per-file quotas can block playback."
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "animetoki.com")
    }

    async fn search(&self, http: &Http, query: &str, page: u32) -> Result<Vec<SearchResult>> {
        let mut url = Url::parse(&format!("{}/", self.base))
            .map_err(|error| Error::Url(self.base.clone(), error))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("s", query.trim());
            if page > 1 {
                pairs.append_pair("paged", &page.to_string());
            }
        }
        let html = http
            .get_text(url.as_ref(), &self.doc_headers(&format!("{}/", self.base)))
            .await
            .map_err(|error| Error::provider(self.id(), error))?;
        Ok(self.parse_search(&html))
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Anime> {
        let html = http
            .get_text(url, &self.doc_headers(&format!("{}/", self.base)))
            .await
            .map_err(|error| Error::provider(self.id(), error))?;
        Ok(self.parse_details(url, &html))
    }

    async fn episodes(&self, http: &Http, anime: &Anime) -> Result<Vec<Episode>> {
        let html = http
            .get_text(&anime.url, &self.doc_headers(&format!("{}/", self.base)))
            .await
            .map_err(|error| Error::provider(self.id(), error))?;
        let (folders, direct) = {
            let document = Html::parse_document(&html);
            let selector = Selector::parse("a[href]").unwrap();
            let mut folders = Vec::new();
            let mut direct = Vec::new();
            let mut seen = HashSet::new();
            for element in document.select(&selector) {
                let Some(href) = element.value().attr("href") else {
                    continue;
                };
                let url = util::absolutize(&format!("{}/", self.base), href);
                if !seen.insert(url.clone()) {
                    continue;
                }
                if url.contains(&self.cloud) || url.contains(&self.drive) {
                    folders.push(url);
                } else if is_direct_video_url(&url) {
                    direct.push(url);
                }
            }
            (folders, direct)
        };

        let mut episodes = Vec::new();
        for folder in folders {
            match self.episodes_from_folder(http, &folder, &anime.url).await {
                Ok(found) => episodes.extend(found),
                Err(error) => tracing::warn!(%error, folder = %folder, "AnimeToki folder failed"),
            }
        }
        for (index, url) in direct.into_iter().enumerate() {
            let name = url
                .split(['?', '#'])
                .next()
                .and_then(|value| value.rsplit('/').next())
                .unwrap_or("Episode");
            episodes.push(Episode {
                number: (index + 1) as f32,
                title: Some(clean_name(name)),
                id: format!("direct:{url}"),
                url,
                filler: false,
                recap: false,
                aired: None,
                audio: Audio::Both,
            });
        }

        if episodes.is_empty() {
            return Err(Error::provider(
                self.id(),
                "AnimeToki page had no cloud folders or direct video files",
            ));
        }
        episodes.sort_by(|left, right| left.number.total_cmp(&right.number));
        Ok(episodes)
    }

    async fn streams(
        &self,
        _http: &Http,
        _anime: &Anime,
        episode: &Episode,
    ) -> Result<Vec<Stream>> {
        let (url, container, quality, title) =
            if let Some(encoded) = episode.id.strip_prefix("cloud:") {
                let data: CloudEpisode = serde_json::from_str(encoded).map_err(|error| {
                    Error::provider(self.id(), format!("invalid AnimeToki id: {error}"))
                })?;
                (
                    download_url(&data.file_id, &data.name),
                    container_for(&data.name),
                    infer_quality(&data.name),
                    clean_name(&data.name),
                )
            } else if let Some(url) = episode.id.strip_prefix("direct:") {
                (
                    url.to_string(),
                    container_for(url),
                    infer_quality(url),
                    "AnimeToki".to_string(),
                )
            } else {
                return Err(Error::provider(self.id(), "unknown AnimeToki episode id"));
            };

        let mut stream_headers = BTreeMap::new();
        stream_headers.insert("Referer".to_string(), format!("{}/", self.base));
        stream_headers.insert("Origin".to_string(), self.base.clone());
        Ok(vec![Stream {
            url,
            container,
            quality,
            title: Some(match quality {
                Some(quality) => format!("AnimeToki {quality}p - {title}"),
                None => format!("AnimeToki - {title}"),
            }),
            audio: Audio::Both,
            server: Some("AnimeToki Cloud".to_string()),
            subtitles: Vec::new(),
            chapters: Vec::new(),
            headers: stream_headers,
            kind: StreamKind::Direct,
            flix: None,
        }])
    }
}

/// The worker redirects percent-encoded folder names to their base64 form.
/// Reqwest turns a POST across that redirect into a GET, so normalize the
/// path before requesting it.
fn normalize_folder_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return value.to_string();
    };
    let Some(segment) = url.path_segments().and_then(|mut segments| segments.next()) else {
        return value.to_string();
    };
    let decoded = urlencoding::decode(segment)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| segment.to_string());
    let encoded = base64::engine::general_purpose::STANDARD.encode(decoded.as_bytes());
    url.set_path(&format!("/{encoded}/"));
    url.to_string()
}

fn is_video(file: &FolderFile) -> bool {
    file.mime_type.starts_with("video/")
        || [".mp4", ".mkv", ".webm", ".avi"]
            .iter()
            .any(|extension| file.name.to_ascii_lowercase().ends_with(extension))
}

fn is_direct_video_url(url: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    [".mp4", ".mkv", ".webm", ".avi"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn episode_number(name: &str) -> Option<f32> {
    static EPISODE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let regex =
        EPISODE.get_or_init(|| Regex::new(r"(?i)(?:episode|ep|e)[\s._-]*(\d+(?:\.\d+)?)").unwrap());
    regex
        .captures(name)
        .and_then(|captures| captures.get(1)?.as_str().parse().ok())
}

fn container_for(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    ["mkv", "mp4", "webm", "avi"]
        .into_iter()
        .find(|extension| lower.contains(&format!(".{extension}")))
        .map(str::to_string)
}

fn clean_name(name: &str) -> String {
    name.trim()
        .trim_end_matches(".mkv")
        .trim_end_matches(".mp4")
        .trim_end_matches(".webm")
        .trim()
        .to_string()
}

fn download_url(file_id: &str, name: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(name.as_bytes());
    format!(
        "https://cloud.animetoki.com/?a=download&id={}&name={}&mode=stream",
        urlencoding::encode(file_id),
        urlencoding::encode(&encoded)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_episode_numbers() {
        assert_eq!(
            episode_number("[AnimeToki] Cyberpunk - Edgerunners - Episode 10 [1080p] .mkv"),
            Some(10.0)
        );
        assert_eq!(episode_number("Show E2 [720p].mp4"), Some(2.0));
        assert_eq!(episode_number("No number here"), None);
    }

    #[test]
    fn parses_cloud_file_index() {
        let payload = r#"{"files":[
            {"mimeType":"video/x-matroska","size":"506827883","id":"abc","name":"[AnimeToki] Show - Episode 1 [1080p] .mkv"},
            {"mimeType":"image/png","id":"cover","name":"cover.png"}
        ]}"#;
        let folder: Folder = serde_json::from_str(payload).unwrap();
        assert_eq!(folder.files.len(), 2);
        assert!(is_video(&folder.files[0]));
        assert!(!is_video(&folder.files[1]));
        assert_eq!(infer_quality(&folder.files[0].name), Some(1080));
    }

    #[test]
    fn builds_download_url() {
        let url = download_url("abc", "[AnimeToki] Show - Episode 1 [1080p] .mkv");
        assert!(url.starts_with("https://cloud.animetoki.com/?a=download&id=abc&name="));
        assert!(url.ends_with("&mode=stream"));
    }
}
