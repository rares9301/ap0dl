use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use url::Url;

use ap0dl_core::error::{Error, Result};
use ap0dl_core::http::{Http, headers};

use crate::config::MangaDexConfig;
use crate::filters::{Filter, FilterValues, option};
use crate::models::{Chapter, Manga, MangaStatus, MangasPage, Page};
use crate::source::{MangaSource, SourceStatus, host_matches};

const PAGE_SIZE: u32 = 20;
const FEED_PAGE_SIZE: u32 = 500;
/// The feed endpoint filters by content rating independently of the manga.
/// Without all four values chapters of an allowed manga can disappear.
const ALL_RATINGS: [&str; 4] = ["safe", "suggestive", "erotica", "pornographic"];
/// `/at-home/server` allows 40 requests per minute per client. Bulk
/// downloads ask for one per chapter, so space them out instead of
/// tripping HTTP 429.
const AT_HOME_INTERVAL: Duration = Duration::from_millis(1600);

/// MangaDex through its public JSON API, following the Aniyomi/Mihon
/// MangaDex extension: `/manga` for browse and search, `/manga/{id}/feed`
/// for chapters and `/at-home/server/{chapter}` for page images.
pub struct MangaDex {
    base: String,
    api: String,
    uploads: String,
    languages: Vec<String>,
    content_rating: Vec<String>,
    data_saver: bool,
    last_at_home: tokio::sync::Mutex<Option<Instant>>,
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Collection<T> {
    #[serde(default)]
    data: Vec<T>,
    #[serde(default)]
    total: u32,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    limit: u32,
}

#[derive(Debug, Deserialize)]
struct Entity<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct MangaData {
    id: String,
    attributes: MangaAttributes,
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct MangaAttributes {
    title: BTreeMap<String, String>,
    alt_titles: Vec<BTreeMap<String, String>>,
    description: BTreeMap<String, String>,
    status: Option<String>,
    year: Option<u32>,
    publication_demographic: Option<String>,
    tags: Vec<Tag>,
}

#[derive(Debug, Deserialize)]
struct Tag {
    attributes: TagAttributes,
}

#[derive(Debug, Deserialize)]
struct TagAttributes {
    #[serde(default)]
    name: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Relationship {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    attributes: Option<RelationshipAttributes>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RelationshipAttributes {
    name: Option<String>,
    file_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChapterData {
    id: String,
    attributes: ChapterAttributes,
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ChapterAttributes {
    volume: Option<String>,
    chapter: Option<String>,
    title: Option<String>,
    translated_language: Option<String>,
    external_url: Option<String>,
    is_unavailable: bool,
    publish_at: Option<String>,
    pages: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AtHome {
    base_url: String,
    chapter: AtHomeChapter,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AtHomeChapter {
    hash: String,
    #[serde(default)]
    data: Vec<String>,
    #[serde(default)]
    data_saver: Vec<String>,
}

impl MangaDex {
    pub fn new(config: &MangaDexConfig) -> Result<Self> {
        for value in [&config.base_url, &config.api_url, &config.uploads_url] {
            Url::parse(value.trim_end_matches('/'))
                .map_err(|error| Error::Url(value.clone(), error))?;
        }
        Ok(Self {
            base: config.base_url.trim_end_matches('/').to_string(),
            api: config.api_url.trim_end_matches('/').to_string(),
            uploads: config.uploads_url.trim_end_matches('/').to_string(),
            languages: config.languages.clone(),
            content_rating: config.content_rating.clone(),
            data_saver: config.data_saver,
            last_at_home: tokio::sync::Mutex::new(None),
        })
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        headers(&[
            ("Accept", "application/json"),
            ("Referer", &format!("{}/", self.base)),
        ])
    }

    fn url(&self, path: &str, query: &[(String, String)]) -> Result<String> {
        let mut url = Url::parse(&format!("{}{path}", self.api))
            .map_err(|error| Error::Url(path.to_string(), error))?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url.to_string())
    }

    fn browse_query(&self, page: u32, filters: &FilterValues) -> Vec<(String, String)> {
        let mut query = vec![
            ("limit".to_string(), PAGE_SIZE.to_string()),
            (
                "offset".to_string(),
                (page.saturating_sub(1) * PAGE_SIZE).to_string(),
            ),
            ("includes[]".to_string(), "cover_art".to_string()),
        ];
        let ratings = filters.all("rating");
        let ratings: Vec<String> = if ratings.is_empty() {
            self.content_rating.clone()
        } else {
            ratings.to_vec()
        };
        for rating in ratings {
            query.push(("contentRating[]".to_string(), rating));
        }
        for language in &self.languages {
            query.push((
                "availableTranslatedLanguage[]".to_string(),
                language.clone(),
            ));
        }
        for status in filters.all("status") {
            query.push(("status[]".to_string(), status.clone()));
        }
        for demographic in filters.all("demographic") {
            query.push(("publicationDemographic[]".to_string(), demographic.clone()));
        }
        query
    }

    async fn manga_list(&self, http: &Http, query: Vec<(String, String)>) -> Result<MangasPage> {
        let url = self.url("/manga", &query)?;
        let response: Collection<MangaData> = http.get_json(&url, &self.headers()).await?;
        let has_next_page = response.offset + response.limit.max(1) < response.total;
        Ok(MangasPage {
            mangas: response
                .data
                .into_iter()
                .map(|data| self.to_manga(data))
                .collect(),
            has_next_page,
        })
    }

    fn to_manga(&self, data: MangaData) -> Manga {
        let attributes = &data.attributes;
        let title = localized(&attributes.title)
            .or_else(|| {
                attributes
                    .alt_titles
                    .iter()
                    .find_map(|titles| titles.get("en").cloned())
            })
            .unwrap_or_else(|| data.id.clone());
        let mut manga = Manga::new(
            "mangadex",
            data.id.clone(),
            title.clone(),
            format!("{}/title/{}", self.base, data.id),
        );
        manga.alt_titles = attributes
            .alt_titles
            .iter()
            .filter_map(|titles| {
                titles
                    .get("en")
                    .or_else(|| titles.get("ja-ro"))
                    .or_else(|| titles.get("ja"))
                    .cloned()
            })
            .filter(|alt| alt != &title)
            .collect();
        manga.description = localized(&attributes.description);
        manga.status = attributes
            .status
            .as_deref()
            .map(MangaStatus::parse)
            .unwrap_or_default();
        manga.year = attributes.year;
        manga.format = attributes.publication_demographic.clone();
        manga.genres = attributes
            .tags
            .iter()
            .filter_map(|tag| localized(&tag.attributes.name))
            .collect();
        manga.author = relationship_names(&data.relationships, "author");
        manga.artist = relationship_names(&data.relationships, "artist");
        manga.thumbnail_url = data
            .relationships
            .iter()
            .find(|relationship| relationship.kind == "cover_art")
            .and_then(|relationship| relationship.attributes.as_ref()?.file_name.clone())
            .map(|file| format!("{}/covers/{}/{file}.256.jpg", self.uploads, data.id));
        manga
    }

    fn id_from_url(value: &str, kind: &str) -> Option<String> {
        let url = Url::parse(value).ok()?;
        let mut segments = url.path_segments()?;
        while let Some(segment) = segments.next() {
            if segment == kind {
                return segments.next().map(str::to_string);
            }
        }
        None
    }

    fn manga_id(value: &str) -> String {
        Self::id_from_url(value, "title")
            .or_else(|| Self::id_from_url(value, "manga"))
            .unwrap_or_else(|| value.trim().to_string())
    }

    fn chapter_id(value: &str) -> String {
        Self::id_from_url(value, "chapter").unwrap_or_else(|| value.trim().to_string())
    }
}

#[async_trait]
impl MangaSource for MangaDex {
    fn id(&self) -> &'static str {
        "mangadex"
    }

    fn name(&self) -> &'static str {
        "MangaDex"
    }

    fn lang(&self) -> &str {
        self.languages.first().map(String::as_str).unwrap_or("en")
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> SourceStatus {
        SourceStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "Public JSON API; licensed series often have no English chapters"
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "mangadex.org")
    }

    fn supports_latest(&self) -> bool {
        true
    }

    fn filters(&self) -> Vec<Filter> {
        vec![
            Filter::select(
                "sort",
                "Sort",
                vec![
                    option("relevance", "Relevance"),
                    option("followedCount", "Popular"),
                    option("latestUploadedChapter", "Latest upload"),
                    option("rating", "Rating"),
                    option("createdAt", "Recently added"),
                    option("title", "Title"),
                    option("year", "Year"),
                ],
            ),
            Filter::multi(
                "status",
                "Status",
                vec![
                    option("ongoing", "Ongoing"),
                    option("completed", "Completed"),
                    option("hiatus", "Hiatus"),
                    option("cancelled", "Cancelled"),
                ],
            ),
            Filter::multi(
                "demographic",
                "Demographic",
                vec![
                    option("shounen", "Shounen"),
                    option("shoujo", "Shoujo"),
                    option("seinen", "Seinen"),
                    option("josei", "Josei"),
                    option("none", "None"),
                ],
            ),
            Filter::multi(
                "rating",
                "Content rating",
                vec![
                    option("safe", "Safe"),
                    option("suggestive", "Suggestive"),
                    option("erotica", "Erotica"),
                    option("pornographic", "Pornographic"),
                ],
            ),
        ]
    }

    async fn popular(&self, http: &Http, page: u32) -> Result<MangasPage> {
        let mut query = self.browse_query(page, &FilterValues::default());
        query.push(("order[followedCount]".to_string(), "desc".to_string()));
        self.manga_list(http, query).await
    }

    async fn latest(&self, http: &Http, page: u32) -> Result<MangasPage> {
        let mut query = self.browse_query(page, &FilterValues::default());
        query.push((
            "order[latestUploadedChapter]".to_string(),
            "desc".to_string(),
        ));
        self.manga_list(http, query).await
    }

    async fn search(
        &self,
        http: &Http,
        query: &str,
        page: u32,
        filters: &FilterValues,
    ) -> Result<MangasPage> {
        let mut params = self.browse_query(page, filters);
        let query = query.trim();
        if !query.is_empty() {
            params.push(("title".to_string(), query.to_string()));
        }
        let sort = match filters.get("sort") {
            // Relevance only exists when a title is given.
            Some("relevance") | None if query.is_empty() => "followedCount",
            Some(sort) => sort,
            None => "relevance",
        };
        let direction = if sort == "title" { "asc" } else { "desc" };
        params.push((format!("order[{sort}]"), direction.to_string()));
        self.manga_list(http, params).await
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Manga> {
        let id = Self::manga_id(url);
        let url = self.url(
            &format!("/manga/{id}"),
            &[
                ("includes[]".to_string(), "author".to_string()),
                ("includes[]".to_string(), "artist".to_string()),
                ("includes[]".to_string(), "cover_art".to_string()),
            ],
        )?;
        let response: Entity<MangaData> = http.get_json(&url, &self.headers()).await?;
        let mut manga = self.to_manga(response.data);
        manga.initialized = true;
        Ok(manga)
    }

    async fn chapters(&self, http: &Http, manga: &Manga) -> Result<Vec<Chapter>> {
        let id = Self::manga_id(&manga.id);
        let mut chapters = Vec::new();
        let mut offset = 0u32;
        loop {
            let mut query = vec![
                ("limit".to_string(), FEED_PAGE_SIZE.to_string()),
                ("offset".to_string(), offset.to_string()),
                ("includes[]".to_string(), "scanlation_group".to_string()),
                ("order[volume]".to_string(), "asc".to_string()),
                ("order[chapter]".to_string(), "asc".to_string()),
            ];
            for language in &self.languages {
                query.push(("translatedLanguage[]".to_string(), language.clone()));
            }
            for rating in ALL_RATINGS {
                query.push(("contentRating[]".to_string(), rating.to_string()));
            }
            let url = self.url(&format!("/manga/{id}/feed"), &query)?;
            let response: Collection<ChapterData> = http.get_json(&url, &self.headers()).await?;
            let fetched = response.data.len() as u32;
            for data in response.data {
                if let Some(chapter) = self.to_chapter(data) {
                    chapters.push(chapter);
                }
            }
            offset += fetched;
            if fetched == 0 || offset >= response.total {
                break;
            }
        }
        Ok(chapters)
    }

    async fn pages(&self, http: &Http, chapter: &Chapter) -> Result<Vec<Page>> {
        let id = Self::chapter_id(&chapter.id);
        let url = self.url(&format!("/at-home/server/{id}"), &[])?;
        {
            let mut last = self.last_at_home.lock().await;
            if let Some(previous) = *last {
                let elapsed = previous.elapsed();
                if elapsed < AT_HOME_INTERVAL {
                    tokio::time::sleep(AT_HOME_INTERVAL - elapsed).await;
                }
            }
            *last = Some(Instant::now());
        }
        let response: AtHome = http.get_json(&url, &self.headers()).await?;
        let (quality, files) = if self.data_saver && !response.chapter.data_saver.is_empty() {
            ("data-saver", response.chapter.data_saver)
        } else {
            ("data", response.chapter.data)
        };
        if files.is_empty() {
            return Err(Error::provider(
                "mangadex",
                format!("chapter {id} has no hosted pages"),
            ));
        }
        let base = response.base_url.trim_end_matches('/');
        let hash = response.chapter.hash;
        // @Home cache nodes answer 404 for pages they have not pulled yet;
        // the origin server always has them, so it is every page's fallback.
        Ok(files
            .into_iter()
            .enumerate()
            .map(|(index, file)| {
                let mut page = Page::image(index, format!("{base}/{quality}/{hash}/{file}"));
                page.fallback_urls = vec![format!("{}/{quality}/{hash}/{file}", self.uploads)];
                page
            })
            .collect())
    }
}

impl MangaDex {
    fn to_chapter(&self, data: ChapterData) -> Option<Chapter> {
        let attributes = data.attributes;
        // External chapters (publisher links) have no MangaDex-hosted pages.
        if attributes.external_url.is_some() || attributes.is_unavailable || attributes.pages == 0 {
            return None;
        }
        let number = attributes
            .chapter
            .as_deref()
            .and_then(|value| value.trim().parse::<f32>().ok())
            .unwrap_or(-1.0);
        let mut name = String::new();
        if let Some(volume) = attributes.volume.as_deref().filter(|v| !v.is_empty()) {
            name.push_str(&format!("Vol.{volume} "));
        }
        match attributes.chapter.as_deref().filter(|c| !c.is_empty()) {
            Some(chapter) => name.push_str(&format!("Ch.{chapter}")),
            None => name.push_str("Oneshot"),
        }
        if let Some(title) = attributes.title.as_deref().filter(|t| !t.trim().is_empty()) {
            name.push_str(" - ");
            name.push_str(title.trim());
        }
        Some(Chapter {
            url: format!("{}/chapter/{}", self.base, data.id),
            id: data.id,
            name,
            number,
            volume: attributes.volume,
            scanlator: relationship_names(&data.relationships, "scanlation_group"),
            date_upload: attributes.publish_at,
            language: attributes.translated_language,
            pages: Some(attributes.pages),
        })
    }
}

fn localized(values: &BTreeMap<String, String>) -> Option<String> {
    ["en", "ja-ro", "ja"]
        .iter()
        .find_map(|language| values.get(*language))
        .or_else(|| values.values().next())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn relationship_names(relationships: &[Relationship], kind: &str) -> Option<String> {
    let mut names: Vec<String> = Vec::new();
    for name in relationships
        .iter()
        .filter(|relationship| relationship.kind == kind)
        .filter_map(|relationship| relationship.attributes.as_ref()?.name.clone())
    {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    (!names.is_empty()).then(|| names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> MangaDex {
        MangaDex::new(&MangaDexConfig::default()).unwrap()
    }

    #[test]
    fn parses_manga_with_cover_authors_and_tags() {
        let json = r#"{"data":{"id":"2e0fdb3b-632c-4f8f-a311-5b56952db647","type":"manga",
            "attributes":{"title":{"ja-ro":"Bocchi the Rock!"},
              "altTitles":[{"ja":"ぼっち・ざ・ろっく！"},{"en":"Bocchi the Rock!"}],
              "description":{"en":"Hitori Gotou..."},"status":"ongoing","year":2018,
              "publicationDemographic":"seinen",
              "tags":[{"attributes":{"name":{"en":"Comedy"}}}]},
            "relationships":[
              {"id":"a","type":"author","attributes":{"name":"Hamaji Aki"}},
              {"id":"a","type":"artist","attributes":{"name":"Hamaji Aki"}},
              {"id":"c","type":"cover_art","attributes":{"fileName":"cover.jpg"}}]}}"#;
        let entity: Entity<MangaData> = serde_json::from_str(json).unwrap();
        let manga = source().to_manga(entity.data);
        assert_eq!(manga.title, "Bocchi the Rock!");
        assert_eq!(manga.alt_titles, ["ぼっち・ざ・ろっく！"]);
        assert_eq!(manga.status, MangaStatus::Ongoing);
        assert_eq!(manga.author.as_deref(), Some("Hamaji Aki"));
        assert_eq!(manga.genres, ["Comedy"]);
        assert_eq!(
            manga.thumbnail_url.as_deref(),
            Some(
                "https://uploads.mangadex.org/covers/2e0fdb3b-632c-4f8f-a311-5b56952db647/cover.jpg.256.jpg"
            )
        );
        assert_eq!(
            manga.url,
            "https://mangadex.org/title/2e0fdb3b-632c-4f8f-a311-5b56952db647"
        );
    }

    #[test]
    fn skips_external_chapters_and_names_hosted_ones() {
        let json = r#"{"data":[
            {"id":"c1","attributes":{"volume":"1","chapter":"2","title":"Hello",
              "translatedLanguage":"en","externalUrl":null,"isUnavailable":false,"pages":11},
              "relationships":[{"type":"scanlation_group","attributes":{"name":"Aoi Ichigo"}}]},
            {"id":"c2","attributes":{"chapter":"3","externalUrl":"https://example.test","pages":0}},
            {"id":"c3","attributes":{"chapter":null,"pages":5}}],
            "total":3,"offset":0,"limit":500}"#;
        let response: Collection<ChapterData> = serde_json::from_str(json).unwrap();
        let chapters: Vec<Chapter> = response
            .data
            .into_iter()
            .filter_map(|data| source().to_chapter(data))
            .collect();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].name, "Vol.1 Ch.2 - Hello");
        assert_eq!(chapters[0].number, 2.0);
        assert_eq!(chapters[0].scanlator.as_deref(), Some("Aoi Ichigo"));
        assert_eq!(chapters[0].url, "https://mangadex.org/chapter/c1");
        assert_eq!(chapters[1].number, -1.0);
        assert_eq!(chapters[1].name, "Oneshot");
    }

    #[test]
    fn ids_come_from_title_and_chapter_urls() {
        assert_eq!(
            MangaDex::manga_id("https://mangadex.org/title/abc-123/bocchi-the-rock"),
            "abc-123"
        );
        assert_eq!(MangaDex::manga_id("abc-123"), "abc-123");
        assert_eq!(
            MangaDex::chapter_id("https://mangadex.org/chapter/def-456"),
            "def-456"
        );
    }
}
