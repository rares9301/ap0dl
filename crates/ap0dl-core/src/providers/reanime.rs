use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::OnceCell;
use url::Url;

use crate::config::ReAnimeConfig;
use crate::error::{Error, Result};
use crate::http::{Http, headers};
use crate::models::{
    Anime, Audio, Chapter, Episode, FlixInfo, SearchResult, Stream, StreamKind, Subtitle,
};
use crate::provider::{Provider, ProviderStatus, host_matches};
use crate::util;

const FLIX_ORIGIN: &str = "https://flixcloud.cc";
const DEFAULT_XOR_MASK: [u8; 16] = [
    157, 42, 241, 71, 179, 142, 92, 112, 166, 25, 228, 59, 216, 98, 15, 197,
];

pub struct ReAnime {
    base: String,
    api: String,
    flix: String,
    enc_dec: String,
    preferred_audio: String,
    xor_mask: OnceCell<Vec<u8>>,
}

impl ReAnime {
    pub fn new(config: &ReAnimeConfig) -> Result<Self> {
        let base = config.base_url.trim_end_matches('/').to_string();
        Url::parse(&base).map_err(|error| Error::Url(base.clone(), error))?;
        Ok(Self {
            api: format!("{base}/api/v1"),
            flix: format!("{base}/api/flix"),
            enc_dec: config.enc_dec_url.trim_end_matches('/').to_string(),
            preferred_audio: config.preferred_audio.to_ascii_lowercase(),
            base,
            xor_mask: OnceCell::new(),
        })
    }

    fn api_headers(&self, referer: &str) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "application/json, text/plain, */*"),
            ("Accept-Language", "en-US,en;q=0.9"),
            ("Referer", referer),
            ("Sec-Fetch-Dest", "empty"),
            ("Sec-Fetch-Mode", "cors"),
            ("Sec-Fetch-Site", "same-origin"),
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

    fn anime_url(&self, slug: &str) -> String {
        format!("{}/anime/{}", self.base, slug)
    }

    fn watch_url(&self, slug: &str, episode: f32) -> String {
        format!("{}/watch/{}?ep={}", self.base, slug, format_number(episode))
    }

    async fn anime_meta(&self, http: &Http, anime: &Anime) -> Result<(String, i64)> {
        let slug = self.slug_from_url(&anime.url);
        let anilist_id = anime.anilist_id.unwrap_or(0);
        if anilist_id > 0 {
            return Ok((slug, anilist_id));
        }

        let detail: AnimeDto = http
            .get_json(
                &format!("{}/anime/{}", self.api, slug),
                &self.api_headers(&self.anime_url(&slug)),
            )
            .await?;
        Ok((slug, detail.anilist_id.unwrap_or(0)))
    }

    async fn fetch_embed_data(&self, http: &Http, data_link: &str) -> Result<Value> {
        let embed_headers = headers(&[
            ("Accept", "*/*"),
            ("Origin", FLIX_ORIGIN),
            ("Referer", "https://flixcloud.cc/"),
        ]);
        let html = http.get_text(data_link, &embed_headers).await?;
        let raw = extract_embed_json(&html)
            .ok_or_else(|| Error::extraction("flixcloud embed data was not found"))?;
        parse_js_object(&raw)
    }

    async fn xor_mask(&self, http: &Http) -> Vec<u8> {
        self.xor_mask
            .get_or_init(|| async {
                let url = "https://flixcloud.cc/artplayer-new/hls.js?v=104";
                let value = http
                    .get_text(url, &headers(&[("Referer", "https://flixcloud.cc/")]))
                    .await
                    .ok();
                value
                    .as_deref()
                    .and_then(extract_xor_mask)
                    .unwrap_or_else(|| DEFAULT_XOR_MASK.to_vec())
            })
            .await
            .clone()
    }

    async fn extract_flix_stream(
        &self,
        http: &Http,
        data_link: &str,
        data_type: &str,
        server_name: &str,
        softsub: bool,
    ) -> Result<Stream> {
        let data = self.fetch_embed_data(http, data_link).await?;
        let subtitles = parse_subtitles(&data);
        let chapters = parse_chapters(&data);

        let mut token_data = data.clone();
        if let Some(object) = token_data.as_object_mut() {
            object.remove("subtitles");
            object.remove("intro_chapter");
            object.remove("outro_chapter");
        }

        let dec_headers = headers(&[("Accept", "*/*")]);
        let token: DecTokenResponse = http
            .post_json(
                &format!("{}/api/dec-flixcloud?type=token", self.enc_dec),
                &dec_headers,
                &json!({ "data": token_data }),
            )
            .await?;
        let token_result = token.result.ok_or_else(|| {
            Error::extraction(format!("flixcloud token request failed: {}", token.status))
        })?;

        let m3u8_url = format!("{}/api/m3u8/{}", FLIX_ORIGIN, token_result.token);
        let mut stream_response: Value = http
            .get_json(
                &m3u8_url,
                &headers(&[
                    ("Accept", "*/*"),
                    ("Origin", FLIX_ORIGIN),
                    ("Referer", "https://flixcloud.cc/"),
                ]),
            )
            .await?;
        if !stream_response.is_object() {
            stream_response = json!({ "data": stream_response });
        }

        let decrypted: DecStreamResponse = http
            .post_json(
                &format!("{}/api/dec-flixcloud?type=stream", self.enc_dec),
                &dec_headers,
                &json!({
                    "data": {
                        "context": token_result.context,
                        "stream_response": stream_response
                    }
                }),
            )
            .await?;
        let result = decrypted.result.ok_or_else(|| {
            Error::extraction(format!(
                "flixcloud stream decrypt failed: {}",
                decrypted.status
            ))
        })?;
        let w_payload = result
            .context
            .get("w_payload")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::extraction("flixcloud stream context has no w_payload"))?
            .to_string();

        let master_url = format!(
            "{}/api/parse-flixcloud?url={}&w_payload={}",
            self.enc_dec,
            urlencoding::encode(&result.stream),
            urlencoding::encode(&w_payload)
        );

        let manifest = http
            .get_text(&master_url, &headers(&[("Accept", "*/*")]))
            .await
            .ok();
        let quality = manifest.as_deref().and_then(parse_max_height);

        let audio = match data_type.to_ascii_lowercase().as_str() {
            "dub" => Audio::Dub,
            "sub" => Audio::Sub,
            "raw" => Audio::Raw,
            _ => Audio::Both,
        };
        let mut title = format!("[{}] {}", audio.label(), server_name);
        if softsub {
            title.push_str(" [Softsub]");
        }

        let mut stream_headers = BTreeMap::new();
        stream_headers.insert("Referer".to_string(), "https://flixcloud.cc/".to_string());
        stream_headers.insert("Origin".to_string(), FLIX_ORIGIN.to_string());

        Ok(Stream {
            url: master_url.clone(),
            quality,
            title: Some(title),
            audio,
            server: Some(server_name.to_string()),
            subtitles,
            chapters,
            headers: stream_headers,
            kind: StreamKind::Hls,
            flix: Some(FlixInfo {
                master_url,
                w_payload,
                referer: "https://flixcloud.cc/".to_string(),
                xor_mask: self.xor_mask(http).await,
            }),
        })
    }
}

#[async_trait]
impl Provider for ReAnime {
    fn id(&self) -> &'static str {
        "reanime"
    }

    fn name(&self) -> &'static str {
        "Re:ANIME"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "Primary source. Public JSON API plus enc-dec flixcloud decrypt proxy."
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "reanime.to")
    }

    async fn search(&self, http: &Http, query: &str, page: u32) -> Result<Vec<SearchResult>> {
        let limit = 36u32;
        let offset = page.saturating_sub(1) * limit;
        let mut url = Url::parse(&format!("{}/search", self.api))
            .map_err(|error| Error::Url(self.api.clone(), error))?;
        url.query_pairs_mut()
            .append_pair("limit", &limit.to_string())
            .append_pair("offset", &offset.to_string());
        if !query.trim().is_empty() {
            url.query_pairs_mut().append_pair("q", query.trim());
        }

        let response: SearchResponse = http
            .get_json(
                url.as_str(),
                &self.api_headers(&format!("{}/search", self.base)),
            )
            .await?;
        Ok(response
            .results
            .into_iter()
            .map(|item| item.into_search_result(self))
            .collect())
    }

    async fn latest(&self, http: &Http, page: u32) -> Result<Vec<SearchResult>> {
        let limit = 24u32;
        let offset = page.saturating_sub(1) * limit;
        let mut url = Url::parse(&format!("{}/home/latest-aired", self.api))
            .map_err(|error| Error::Url(self.api.clone(), error))?;
        url.query_pairs_mut()
            .append_pair("limit", &limit.to_string())
            .append_pair("offset", &offset.to_string())
            .append_pair("lang", "en");
        let response: LatestResponse = http
            .get_json(
                url.as_str(),
                &self.api_headers(&format!("{}/home", self.base)),
            )
            .await?;
        Ok(response
            .data
            .into_iter()
            .map(|item| item.into_search_result(self))
            .collect())
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Anime> {
        let slug = self.slug_from_url(url);
        let detail: AnimeDto = http
            .get_json(
                &format!("{}/anime/{}", self.api, slug),
                &self.api_headers(&self.anime_url(&slug)),
            )
            .await?;
        Ok(detail.into_anime(self, &slug))
    }

    async fn episodes(&self, http: &Http, anime: &Anime) -> Result<Vec<Episode>> {
        let slug = self.slug_from_url(&anime.url);
        let url = format!("{}/anime/{}/episodes?limit=2000", self.api, slug);
        let response: EpisodeListResponse = http
            .get_json(&url, &self.api_headers(&self.anime_url(&slug)))
            .await?;

        let mut episodes: Vec<Episode> = response
            .data
            .into_iter()
            .filter(|episode| episode.playable)
            .map(|episode| {
                let number = episode.episode_number;
                let audio = match (episode.subbed, episode.dubbed) {
                    (true, true) => Audio::Both,
                    (false, true) => Audio::Dub,
                    _ => Audio::Sub,
                };
                Episode {
                    number,
                    title: Some(episode.title)
                        .filter(|title| !title.trim().is_empty())
                        .map(|title| title.trim().to_string()),
                    id: format!("{}/{}", slug, episode.episode_id),
                    url: self.watch_url(&slug, number),
                    filler: episode.is_filler,
                    recap: episode.is_recap,
                    aired: episode.aired,
                    audio,
                }
            })
            .collect();
        episodes.sort_by(|a, b| a.number.total_cmp(&b.number));
        Ok(episodes)
    }

    async fn streams(&self, http: &Http, anime: &Anime, episode: &Episode) -> Result<Vec<Stream>> {
        let (slug, anilist_id) = self.anime_meta(http, anime).await?;
        if anilist_id <= 0 {
            return Err(Error::provider(
                self.id(),
                "the detail API did not return an AniList id for this show",
            ));
        }

        let url = format!(
            "{}/{}/{}",
            self.flix,
            anilist_id,
            format_number(episode.number)
        );
        let response: FlixResponse = http
            .get_json(
                &url,
                &self.api_headers(&self.watch_url(&slug, episode.number)),
            )
            .await?;
        if !response.success {
            return Err(Error::provider(
                self.id(),
                "flix API returned success=false",
            ));
        }

        let mut streams = Vec::new();
        let mut seen = HashSet::new();
        let mut last_error = None;

        for server in response.servers {
            let Some(data_link) = server.data_link.as_deref() else {
                continue;
            };
            if data_link.trim().is_empty() {
                continue;
            }
            let data_type = server.data_type.as_deref().unwrap_or("sub");
            if (self.preferred_audio == "sub" && data_type != "sub")
                || (self.preferred_audio == "dub" && data_type != "dub")
            {
                continue;
            }

            match self
                .extract_flix_stream(
                    http,
                    data_link,
                    data_type,
                    server.server_name.as_deref().unwrap_or("flixcloud"),
                    server.softsub,
                )
                .await
            {
                Ok(stream) => {
                    let key = (stream.url.clone(), stream.audio);
                    if seen.insert(key) {
                        streams.push(stream);
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        server = server.server_name.as_deref().unwrap_or("?"),
                        %error,
                        "reanime stream extraction failed"
                    );
                    last_error = Some(error);
                }
            }
        }

        if streams.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                Error::provider(
                    self.id(),
                    "no playable servers were returned for this episode",
                )
            }));
        }
        Ok(streams)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Title {
    #[serde(default)]
    english: Option<String>,
    #[serde(default)]
    romaji: Option<String>,
    #[serde(default)]
    native: Option<String>,
    #[serde(default)]
    user_preferred: Option<String>,
}

impl Title {
    fn preferred(&self) -> String {
        [
            &self.user_preferred,
            &self.english,
            &self.romaji,
            &self.native,
        ]
        .into_iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or("Unknown")
        .to_string()
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Cover {
    #[serde(default)]
    extra_large: Option<String>,
    #[serde(default)]
    large: Option<String>,
    #[serde(default)]
    medium: Option<String>,
}

impl Cover {
    fn preferred(&self) -> Option<String> {
        self.extra_large
            .clone()
            .or_else(|| self.large.clone())
            .or_else(|| self.medium.clone())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AnimeDto {
    anime_id: String,
    #[serde(default)]
    anilist_id: Option<i64>,
    #[serde(default)]
    mal_id: Option<i64>,
    #[serde(default)]
    title: Title,
    #[serde(default)]
    cover_image: Option<Cover>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    season_year: Option<u32>,
    #[serde(default)]
    episodes: Option<u32>,
}

impl AnimeDto {
    fn into_search_result(self, provider: &ReAnime) -> SearchResult {
        SearchResult {
            url: provider.anime_url(&self.anime_id),
            title: self.title.preferred(),
            id: self.anime_id,
            provider: provider.id().to_string(),
            cover: self.cover_image.as_ref().and_then(Cover::preferred),
            year: self.season_year,
            format: self.format,
            episodes: self.episodes.filter(|episodes| *episodes > 0),
        }
    }

    fn into_anime(self, provider: &ReAnime, slug: &str) -> Anime {
        Anime {
            id: self.anime_id,
            title: self.title.preferred(),
            url: provider.anime_url(slug),
            provider: provider.id().to_string(),
            cover: self.cover_image.as_ref().and_then(Cover::preferred),
            description: self.description.map(|value| util::strip_html(&value)),
            genres: self.genres,
            year: self.season_year,
            format: self.format,
            status: self.status,
            total_episodes: self.episodes.filter(|episodes| *episodes > 0),
            anilist_id: self.anilist_id.filter(|id| *id > 0),
            mal_id: self.mal_id.filter(|id| *id > 0),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<AnimeDto>,
}

#[derive(Debug, Deserialize)]
struct LatestResponse {
    #[serde(default)]
    data: Vec<AnimeDto>,
}

#[derive(Debug, Deserialize)]
struct EpisodeListResponse {
    #[serde(default)]
    data: Vec<EpisodeDto>,
}

#[derive(Debug, Deserialize)]
struct EpisodeDto {
    #[serde(default, rename = "episodeId")]
    episode_id: String,
    #[serde(default)]
    episode_number: f32,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "is_filler")]
    is_filler: bool,
    #[serde(default, rename = "is_recap")]
    is_recap: bool,
    #[serde(default)]
    aired: Option<String>,
    #[serde(default)]
    playable: bool,
    #[serde(default)]
    subbed: bool,
    #[serde(default)]
    dubbed: bool,
}

#[derive(Debug, Deserialize)]
struct FlixResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    servers: Vec<FlixServer>,
}

#[derive(Debug, Deserialize)]
struct FlixServer {
    #[serde(default, rename = "serverName")]
    server_name: Option<String>,
    #[serde(default, rename = "dataLink")]
    data_link: Option<String>,
    #[serde(default, rename = "dataType")]
    data_type: Option<String>,
    #[serde(default)]
    softsub: bool,
}

#[derive(Debug, Deserialize)]
struct DecTokenResponse {
    #[serde(default)]
    status: u16,
    #[serde(default)]
    result: Option<DecTokenResult>,
}

#[derive(Debug, Deserialize)]
struct DecTokenResult {
    token: String,
    #[serde(default)]
    context: Value,
}

#[derive(Debug, Deserialize)]
struct DecStreamResponse {
    #[serde(default)]
    status: u16,
    #[serde(default)]
    result: Option<DecStreamResult>,
}

#[derive(Debug, Deserialize)]
struct DecStreamResult {
    stream: String,
    #[serde(default)]
    context: Value,
}

fn format_number(number: f32) -> String {
    if number.fract() == 0.0 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

fn extract_embed_json(html: &str) -> Option<String> {
    let regex = Regex::new(r#"(?s)type:\s*"data",\s*data:\s*(\{.*?\})\s*,\s*uses:"#).ok()?;
    regex
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
}

fn parse_js_object(raw: &str) -> Result<Value> {
    let cleaned = raw.replace("undefined", "null");
    if let Ok(value) = json5::from_str::<Value>(&cleaned) {
        return Ok(value);
    }

    // Fallback for object literals that JSON5 rejects: quote bare keys.
    let key_regex = Regex::new(r"([{,])\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*:").unwrap();
    let quoted = key_regex.replace_all(&cleaned, r#"$1"$2":"#).to_string();
    serde_json::from_str(&quoted)
        .map_err(|error| Error::extraction(format!("could not parse flixcloud data: {error}")))
}

fn extract_xor_mask(script: &str) -> Option<Vec<u8>> {
    let regex = Regex::new(r"for\(var f=\[([0-9]{1,3}(?:,[0-9]{1,3}){15})\]").ok()?;
    let values = regex.captures(script)?.get(1)?.as_str();
    let mask: Vec<u8> = values
        .split(',')
        .filter_map(|value| value.trim().parse::<u8>().ok())
        .collect();
    (mask.len() == 16).then_some(mask)
}

fn parse_subtitles(data: &Value) -> Vec<Subtitle> {
    data.get("subtitles")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item.get("url")?.as_str()?.to_string();
                    let lang = item
                        .get("language")
                        .or_else(|| item.get("lang"))
                        .and_then(Value::as_str)
                        .unwrap_or("Unknown")
                        .to_string();
                    let format = item
                        .get("format")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let default = item
                        .get("default")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Some(Subtitle {
                        url,
                        lang,
                        format,
                        default,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_chapters(data: &Value) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    for key in ["intro_chapter", "outro_chapter"] {
        let Some(value) = data.get(key) else {
            continue;
        };
        let start = value.get("start").and_then(Value::as_f64);
        let end = value.get("end").and_then(Value::as_f64);
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        if end <= start {
            continue;
        }
        chapters.push(Chapter {
            start,
            end,
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(key)
                .to_string(),
        });
    }
    chapters.sort_by(|a, b| a.start.total_cmp(&b.start));
    chapters
}

fn parse_max_height(manifest: &str) -> Option<u32> {
    let regex = Regex::new(r"RESOLUTION=\d+x(\d+)").ok()?;
    regex
        .captures_iter(manifest)
        .filter_map(|captures| captures.get(1)?.as_str().parse::<u32>().ok())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_embed_json() {
        let html = r#"<script>data: [null,null,{type:"data",data:{video_title:"x",intro_chapter:{start:1,end:2}},uses:{}}]</script>"#;
        let raw = extract_embed_json(html).unwrap();
        let value = parse_js_object(&raw).unwrap();
        assert_eq!(value["video_title"], "x");
    }

    #[test]
    fn parses_xor_mask() {
        let script = "for(var f=[157,42,241,71,179,142,92,112,166,25,228,59,216,98,15,197]";
        let mask = extract_xor_mask(script).unwrap();
        assert_eq!(mask.len(), 16);
        assert_eq!(mask[0], 157);
    }

    #[test]
    fn parses_quality_from_manifest() {
        let manifest = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1,RESOLUTION=1920x1080\nx\n#EXT-X-STREAM-INF:RESOLUTION=1280x720\ny\n";
        assert_eq!(parse_max_height(manifest), Some(1080));
    }

    #[test]
    fn parses_subtitles_and_chapters() {
        let value: Value = serde_json::from_str(
            r#"{"subtitles":[{"url":"https://x/sub.ass","language":"English","format":"ass","default":true}],"intro_chapter":{"start":30,"end":111,"title":"OP"}}"#,
        )
        .unwrap();
        assert_eq!(parse_subtitles(&value).len(), 1);
        assert_eq!(parse_chapters(&value)[0].start, 30.0);
    }
}
