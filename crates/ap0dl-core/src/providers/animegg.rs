use std::collections::BTreeMap;

use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};
use serde::Deserialize;
use url::Url;

use crate::config::AnimeGGConfig;
use crate::error::{Error, Result};
use crate::http::{Http, headers};
use crate::models::{Anime, Audio, Episode, SearchResult, Stream, StreamKind};
use crate::provider::{Provider, ProviderStatus, host_matches};
use crate::util;

pub struct AnimeGG {
    base: String,
}

impl AnimeGG {
    pub fn new(config: &AnimeGGConfig) -> Result<Self> {
        let base = config.base_url.trim_end_matches('/').to_string();
        Url::parse(&base).map_err(|error| Error::Url(base.clone(), error))?;
        Ok(Self { base })
    }

    fn headers(&self, referer: &str) -> reqwest::header::HeaderMap {
        headers(&[
            (
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
            ("Referer", referer),
        ])
    }

    fn slug_from_url(&self, value: &str) -> String {
        if let Ok(url) = Url::parse(value) {
            if let Some(segment) = url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
            {
                if !segment.is_empty() {
                    return segment.to_string();
                }
            }
        }
        value
            .trim()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(value)
            .to_string()
    }

    fn parse_search(&self, html: &str) -> Vec<SearchResult> {
        let document = Html::parse_document(html);
        let selector = Selector::parse(".mse").unwrap();
        let title_selector = Selector::parse(".first h2").unwrap();
        let image_selector = Selector::parse("img").unwrap();

        document
            .select(&selector)
            .filter_map(|element| {
                let title = element
                    .select(&title_selector)
                    .next()
                    .map(|node| node.text().collect::<String>().trim().to_string())
                    .filter(|title| !title.is_empty())?;
                let href = element.value().attr("href")?;
                let url = util::absolutize(&self.base, href);
                let cover = element
                    .select(&image_selector)
                    .next()
                    .and_then(|image| image.value().attr("src"))
                    .map(|src| util::absolutize(&self.base, src));
                Some(SearchResult {
                    id: self.slug_from_url(&url),
                    title,
                    url,
                    provider: self.id().to_string(),
                    cover,
                    year: None,
                    format: None,
                    episodes: None,
                })
            })
            .collect()
    }

    fn parse_popular(&self, html: &str) -> Vec<SearchResult> {
        let document = Html::parse_document(html);
        let selector = Selector::parse(".fea").unwrap();
        let link_selector = Selector::parse(".rightpop a").unwrap();
        let image_selector = Selector::parse("img").unwrap();

        document
            .select(&selector)
            .filter_map(|element| {
                let link = element.select(&link_selector).next()?;
                let title = link.text().collect::<String>().trim().to_string();
                if title.is_empty() {
                    return None;
                }
                let href = link.value().attr("href")?;
                let url = util::absolutize(&self.base, href);
                let cover = element
                    .select(&image_selector)
                    .next()
                    .and_then(|image| image.value().attr("src"))
                    .map(|src| util::absolutize(&self.base, src));
                Some(SearchResult {
                    id: self.slug_from_url(&url),
                    title,
                    url,
                    provider: self.id().to_string(),
                    cover,
                    year: None,
                    format: None,
                    episodes: None,
                })
            })
            .collect()
    }

    async fn episode_page(&self, http: &Http, episode: &Episode) -> Result<String> {
        http.get_text(&episode.url, &self.headers(&format!("{}/", self.base)))
            .await
    }

    fn iframe_sources(&self, html: &str) -> Vec<(String, Audio)> {
        let document = Html::parse_document(html);
        let mut out = Vec::new();
        for (selector, audio) in [
            ("#subbed-Animegg iframe", Audio::Sub),
            ("#dubbed-Animegg iframe", Audio::Dub),
            ("#raw-Animegg iframe", Audio::Raw),
        ] {
            let selector = Selector::parse(selector).unwrap();
            for element in document.select(&selector) {
                if let Some(src) = element.value().attr("src") {
                    out.push((util::absolutize(&self.base, src), audio));
                }
            }
        }

        if out.is_empty() {
            let selector = Selector::parse("iframe").unwrap();
            for (index, element) in document.select(&selector).enumerate() {
                if let Some(src) = element.value().attr("src") {
                    let audio = if index == 1 { Audio::Dub } else { Audio::Sub };
                    out.push((util::absolutize(&self.base, src), audio));
                }
            }
        }
        out
    }

    async fn extract_embed(&self, http: &Http, embed_url: &str) -> Result<Vec<GgVideo>> {
        let html = http
            .get_text(embed_url, &self.headers(&format!("{}/", self.base)))
            .await?;
        let start = html
            .find("var videoSources")
            .ok_or_else(|| Error::extraction("animegg embed has no videoSources"))?;
        let rest = &html[start..];
        let rest = rest
            .split_once('=')
            .map(|(_, value)| value)
            .unwrap_or_default();
        let raw = rest.split(';').next().unwrap_or_default().trim();
        if raw.is_empty() {
            return Err(Error::extraction("animegg videoSources is empty"));
        }

        let key_regex = Regex::new(r"([{,])\s*([A-Za-z_][A-Za-z0-9_]*)\s*:").unwrap();
        let quoted = key_regex.replace_all(raw, r#"$1"$2":"#);
        serde_json::from_str::<Vec<GgVideo>>(&quoted).map_err(|error| {
            Error::extraction(format!("could not parse animegg videoSources: {error}"))
        })
    }
}

impl AnimeGG {
    fn parse_episode_list(&self, html: &str) -> Vec<Episode> {
        let document = Html::parse_document(html);
        let selector = Selector::parse(".newmanga li div").unwrap();
        let link_selector = Selector::parse("a.anm_det_pop").unwrap();
        let title_selector = Selector::parse(".anititle").unwrap();

        document
            .select(&selector)
            .enumerate()
            .filter_map(|(index, element)| {
                let link = element.select(&link_selector).next()?;
                let href = link.value().attr("href")?;
                let url = util::absolutize(&self.base, href);
                let label = link.text().collect::<String>();
                let number = util::last_number(&label)
                    .or_else(|| util::last_number(&url))
                    .unwrap_or((index + 1) as f32);
                let title = element
                    .select(&title_selector)
                    .next()
                    .map(|node| node.text().collect::<String>().trim().to_string())
                    .filter(|value| !value.is_empty());
                Some(Episode {
                    number,
                    title,
                    id: self.slug_from_url(&url),
                    url,
                    filler: false,
                    recap: false,
                    aired: None,
                    audio: Audio::Both,
                })
            })
            .collect()
    }

    fn series_url(&self, html: &str) -> Option<String> {
        let document = Html::parse_document(html);
        let selector = Selector::parse("a[href^='/series/']").unwrap();
        document
            .select(&selector)
            .filter_map(|element| element.value().attr("href"))
            .find(|href| href.starts_with("/series/"))
            .map(|href| util::absolutize(&self.base, href))
    }
}

#[async_trait]
impl Provider for AnimeGG {
    fn id(&self) -> &'static str {
        "animegg"
    }

    fn name(&self) -> &'static str {
        "AnimeGG"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "Second working source. HTML search + direct MP4 qualities."
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "animegg.org")
    }

    async fn search(&self, http: &Http, query: &str, _page: u32) -> Result<Vec<SearchResult>> {
        let mut url = Url::parse(&format!("{}/search/", self.base))
            .map_err(|error| Error::Url(self.base.clone(), error))?;
        url.query_pairs_mut().append_pair("q", query.trim());
        let html = http
            .get_text(url.as_str(), &self.headers(&format!("{}/", self.base)))
            .await?;
        Ok(self.parse_search(&html))
    }

    async fn latest(&self, http: &Http, _page: u32) -> Result<Vec<SearchResult>> {
        let url = format!(
            "{}/popular-series?sortBy=createdAt&sortDirection=DESC&ongoing&limit=50&start=0",
            self.base
        );
        let html = http
            .get_text(&url, &self.headers(&format!("{}/", self.base)))
            .await?;
        Ok(self.parse_popular(&html))
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Anime> {
        let html = http
            .get_text(url, &self.headers(&format!("{}/", self.base)))
            .await?;
        let document = Html::parse_document(&html);
        let title = document
            .select(&Selector::parse(".media-body h1").unwrap())
            .next()
            .map(|element| element.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.slug_from_url(url));

        let description = document
            .select(&Selector::parse(".ptext").unwrap())
            .next()
            .map(|element| element.text().collect::<String>().trim().to_string());
        let cover = document
            .select(&Selector::parse(".media .media-object").unwrap())
            .next()
            .and_then(|element| element.value().attr("src"))
            .map(|src| util::absolutize(&self.base, src));
        let genres: Vec<String> = document
            .select(&Selector::parse(".tagscat a").unwrap())
            .map(|element| element.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        let status = document
            .select(&Selector::parse(".infoami span").unwrap())
            .map(|element| element.text().collect::<String>())
            .find(|text| text.contains("Status:"))
            .map(|text| text.replace("Status:", "").trim().to_string());

        Ok(Anime {
            id: self.slug_from_url(url),
            title,
            url: url.to_string(),
            provider: self.id().to_string(),
            cover,
            description,
            genres,
            year: None,
            format: None,
            status,
            total_episodes: None,
            anilist_id: None,
            mal_id: None,
        })
    }

    async fn episodes(&self, http: &Http, anime: &Anime) -> Result<Vec<Episode>> {
        let html = http
            .get_text(&anime.url, &self.headers(&format!("{}/", self.base)))
            .await?;
        let mut episodes = self.parse_episode_list(&html);

        // A direct episode URL has no `.newmanga` list. Follow its series
        // breadcrumb and parse the real list, so `ap0dl grab <episode-url>`
        // still has an episode to work with.
        if episodes.is_empty() {
            if let Some(series_url) = self.series_url(&html) {
                if series_url != anime.url {
                    if let Ok(series_html) = http
                        .get_text(&series_url, &self.headers(&format!("{}/", self.base)))
                        .await
                    {
                        episodes = self.parse_episode_list(&series_html);
                    }
                }
            }
        }

        if episodes.is_empty() {
            let number = util::last_number(&anime.url).unwrap_or(1.0);
            let title = Html::parse_document(&html)
                .select(&Selector::parse("h1").unwrap())
                .next()
                .map(|node| node.text().collect::<String>().trim().to_string())
                .filter(|value| !value.is_empty());
            episodes.push(Episode {
                number,
                title,
                id: self.slug_from_url(&anime.url),
                url: anime.url.clone(),
                filler: false,
                recap: false,
                aired: None,
                audio: Audio::Both,
            });
        }

        episodes.sort_by(|a, b| a.number.total_cmp(&b.number));
        Ok(episodes)
    }

    async fn streams(&self, http: &Http, _anime: &Anime, episode: &Episode) -> Result<Vec<Stream>> {
        let html = self.episode_page(http, episode).await?;
        let sources = self.iframe_sources(&html);
        if sources.is_empty() {
            return Err(Error::extraction("animegg episode page has no iframes"));
        }

        let mut streams = Vec::new();
        let mut last_error = None;
        for (embed_url, audio) in sources {
            let host = Url::parse(&embed_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
                .unwrap_or_else(|| self.base.clone());
            match self.extract_embed(http, &embed_url).await {
                Ok(videos) => {
                    for video in videos {
                        let url = if video.file.starts_with("http://")
                            || video.file.starts_with("https://")
                        {
                            video.file
                        } else if video.file.starts_with("//") {
                            format!("https:{}", video.file)
                        } else {
                            format!("https://{host}{}", video.file)
                        };
                        let quality = video
                            .label
                            .as_deref()
                            .and_then(util::last_number)
                            .map(|quality| quality as u32);
                        let label = video.label.unwrap_or_else(|| "stream".to_string());
                        let mut stream_headers = BTreeMap::new();
                        stream_headers.insert("Referer".to_string(), format!("https://{host}/"));
                        streams.push(Stream {
                            url,
                            container: None,
                            quality,
                            title: Some(format!("[{}] AnimeGG {}", audio.label(), label)),
                            audio,
                            server: Some("AnimeGG".to_string()),
                            subtitles: Vec::new(),
                            chapters: Vec::new(),
                            headers: stream_headers,
                            kind: StreamKind::Direct,
                            flix: None,
                        });
                    }
                }
                Err(error) => {
                    tracing::warn!(embed = %embed_url, %error, "animegg embed extraction failed");
                    last_error = Some(error);
                }
            }
        }

        if streams.is_empty() {
            return Err(last_error
                .unwrap_or_else(|| Error::extraction("no animegg streams were extracted")));
        }
        streams.sort_by_key(|stream| std::cmp::Reverse(stream.quality));
        Ok(streams)
    }
}

#[derive(Debug, Deserialize)]
struct GgVideo {
    #[serde(default)]
    file: String,
    #[serde(default)]
    label: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_results() {
        let provider = AnimeGG::new(&AnimeGGConfig::default()).unwrap();
        let html = r#"<a href="/series/one-piece" class="mse"><div class="media"><div class="media-body"><div class="first"><h2>One Piece</h2></div></div><img src="/x.jpg"></div></a>"#;
        let results = provider.parse_search(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "One Piece");
        assert!(results[0].url.ends_with("/series/one-piece"));
    }

    #[test]
    fn parses_video_sources() {
        let raw = r#"[{file: "/play/1/video.mp4?for=1", label: "1080p", bk: "", isBk: false }]"#;
        let key_regex = Regex::new(r"([{,])\s*([A-Za-z_][A-Za-z0-9_]*)\s*:").unwrap();
        let quoted = key_regex.replace_all(raw, r#"$1"$2":"#);
        let videos: Vec<GgVideo> = serde_json::from_str(&quoted).unwrap();
        assert_eq!(videos[0].label.as_deref(), Some("1080p"));
    }
}
