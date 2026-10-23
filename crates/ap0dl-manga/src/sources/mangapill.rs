use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use ap0dl_core::error::{Error, Result};
use ap0dl_core::http::{Http, headers};
use ap0dl_core::util;

use crate::config::MangaPillConfig;
use crate::filters::{Filter, FilterOption, FilterValues, option};
use crate::models::{Chapter, Manga, MangaStatus, MangasPage, Page};
use crate::source::{MangaSource, SourceStatus, host_matches};

const GENRES: [&str; 46] = [
    "Action",
    "Adventure",
    "Cars",
    "Comedy",
    "Dementia",
    "Demons",
    "Doujinshi",
    "Drama",
    "Ecchi",
    "Fantasy",
    "Game",
    "Gender Bender",
    "Harem",
    "Historical",
    "Horror",
    "Isekai",
    "Josei",
    "Kids",
    "Magic",
    "Martial Arts",
    "Mecha",
    "Military",
    "Music",
    "Mystery",
    "Parody",
    "Police",
    "Psychological",
    "Romance",
    "Samurai",
    "School",
    "Sci-Fi",
    "Seinen",
    "Shoujo",
    "Shoujo Ai",
    "Shounen",
    "Shounen Ai",
    "Slice of Life",
    "Space",
    "Sports",
    "Super Power",
    "Supernatural",
    "Thriller",
    "Tragedy",
    "Vampire",
    "Yaoi",
    "Yuri",
];

/// MangaPill: server-rendered HTML catalogue, the same selectors as the
/// Aniyomi/Keiyoushi MangaPill extension. Page images are served from a CDN
/// that returns 403 unless the request carries a MangaPill `Referer`.
pub struct MangaPill {
    base: String,
}

impl MangaPill {
    pub fn new(config: &MangaPillConfig) -> Result<Self> {
        let base = config.base_url.trim_end_matches('/').to_string();
        Url::parse(&base).map_err(|error| Error::Url(base.clone(), error))?;
        Ok(Self { base })
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        headers(&[
            (
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
            ("Referer", &format!("{}/", self.base)),
        ])
    }

    fn search_url(&self, query: &str, page: u32, filters: &FilterValues) -> Result<String> {
        let mut url = Url::parse(&format!("{}/search", self.base))
            .map_err(|error| Error::Url(self.base.clone(), error))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", query.trim());
            // An empty query with no filter returns no cards, so browsing
            // without a type lists manga.
            let default_type = if query.trim().is_empty() { "manga" } else { "" };
            pairs.append_pair("type", filters.get("type").unwrap_or(default_type));
            pairs.append_pair("status", filters.get("status").unwrap_or(""));
            for genre in filters.all("genre") {
                pairs.append_pair("genre", genre);
            }
            pairs.append_pair("page", &page.max(1).to_string());
        }
        Ok(url.to_string())
    }

    async fn fetch_search(&self, http: &Http, url: &str) -> Result<MangasPage> {
        let html = http.get_text(url, &self.headers()).await?;
        Ok(self.parse_search(&html))
    }

    fn parse_search(&self, html: &str) -> MangasPage {
        let document = Html::parse_document(html);
        let card = selector("div.grid > div");
        let link = selector("a[href^='/manga/']");
        let title = selector("div.font-black");
        let image = selector("img[data-src]");
        let badge = selector("div.flex.flex-wrap > div");
        let next = selector("a.btn");

        let mut seen = HashSet::new();
        let mut mangas = Vec::new();
        for card in document.select(&card) {
            let Some(href) = card
                .select(&link)
                .next()
                .and_then(|link| link.value().attr("href"))
            else {
                continue;
            };
            let Some(id) = manga_id(href) else {
                continue;
            };
            let Some(name) = card.select(&title).next().map(text) else {
                continue;
            };
            if name.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            let mut manga = Manga::new("mangapill", id, name, util::absolutize(&self.base, href));
            manga.thumbnail_url = card
                .select(&image)
                .next()
                .and_then(|image| image.value().attr("data-src"))
                .map(str::to_string);
            // Badges are, in order: type, year, status.
            let badges: Vec<String> = card.select(&badge).map(text).collect();
            manga.format = badges.first().cloned();
            manga.year = badges.iter().find_map(|value| util::parse_u32(value));
            if let Some(status) = badges
                .iter()
                .find(|value| !matches!(MangaStatus::parse(value), MangaStatus::Unknown))
            {
                manga.status = MangaStatus::parse(status);
            }
            mangas.push(manga);
        }
        let has_next_page = document
            .select(&next)
            .any(|link| text(link) == "Next" && link.value().attr("href").is_some());
        MangasPage {
            mangas,
            has_next_page,
        }
    }

    fn parse_latest(&self, html: &str) -> MangasPage {
        let document = Html::parse_document(html);
        let card = selector("div.grid > div");
        let link = selector("a[href^='/manga/']");
        let title = selector("div.line-clamp-2");
        let image = selector("img[data-src]");
        let mut seen = HashSet::new();
        let mut mangas = Vec::new();
        for card in document.select(&card) {
            let Some(anchor) = card.select(&link).next() else {
                continue;
            };
            let Some(href) = anchor.value().attr("href") else {
                continue;
            };
            let Some(id) = manga_id(href) else {
                continue;
            };
            let name = anchor.select(&title).next().map(text).unwrap_or_default();
            if name.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            let mut manga = Manga::new("mangapill", id, name, util::absolutize(&self.base, href));
            manga.thumbnail_url = card
                .select(&image)
                .next()
                .and_then(|image| image.value().attr("data-src"))
                .map(str::to_string);
            mangas.push(manga);
        }
        MangasPage {
            mangas,
            has_next_page: false,
        }
    }

    fn parse_details(&self, html: &str, url: &str) -> Result<Manga> {
        let document = Html::parse_document(html);
        let title = document
            .select(&selector("h1"))
            .next()
            .map(text)
            .filter(|title| !title.is_empty())
            .ok_or_else(|| Error::Html(url.to_string(), "missing manga title".to_string()))?;
        let id = manga_id(url).unwrap_or_else(|| url.to_string());
        let mut manga = Manga::new("mangapill", id, title, url);
        manga.description = document
            .select(&selector("p.text-sm"))
            .next()
            .map(text)
            .filter(|value| !value.is_empty());
        manga.thumbnail_url = document
            .select(&selector("img[data-src]"))
            .next()
            .and_then(|image| image.value().attr("data-src"))
            .map(str::to_string);
        manga.genres = document
            .select(&selector("a[href^='/search?genre=']"))
            .map(text)
            .filter(|genre| !genre.is_empty())
            .collect();
        for field in document.select(&selector("label")) {
            let label = text(field);
            let value = field
                .next_siblings()
                .filter_map(ElementRef::wrap)
                .next()
                .map(text)
                .unwrap_or_default();
            match label.as_str() {
                "Type" => manga.format = Some(value).filter(|value| !value.is_empty()),
                "Status" => manga.status = MangaStatus::parse(&value),
                "Year" => manga.year = util::parse_u32(&value),
                _ => {}
            }
        }
        manga.initialized = true;
        Ok(manga)
    }

    fn parse_chapters(&self, html: &str) -> Vec<Chapter> {
        let document = Html::parse_document(html);
        document
            .select(&selector("#chapters a[href^='/chapters/']"))
            .filter_map(|link| {
                let href = link.value().attr("href")?;
                let id = href.trim_start_matches("/chapters/").split('/').next()?;
                let name = text(link);
                Some(Chapter {
                    id: id.to_string(),
                    url: util::absolutize(&self.base, href),
                    number: util::last_number(&name).unwrap_or(-1.0),
                    name,
                    volume: None,
                    scanlator: None,
                    date_upload: None,
                    language: Some("en".to_string()),
                    pages: None,
                })
            })
            .collect()
    }

    fn parse_pages(&self, html: &str) -> Vec<Page> {
        let document = Html::parse_document(html);
        let headers = self.image_headers();
        document
            .select(&selector("img.js-page"))
            .filter_map(|image| {
                image
                    .value()
                    .attr("data-src")
                    .or_else(|| image.value().attr("src"))
            })
            .enumerate()
            .map(|(index, url)| Page::image(index, url).with_headers(&headers))
            .collect()
    }
}

#[async_trait]
impl MangaSource for MangaPill {
    fn id(&self) -> &'static str {
        "mangapill"
    }

    fn name(&self) -> &'static str {
        "MangaPill"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> SourceStatus {
        SourceStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "HTML catalogue; images need a MangaPill Referer"
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "mangapill.com")
    }

    fn image_headers(&self) -> BTreeMap<String, String> {
        BTreeMap::from([("Referer".to_string(), format!("{}/", self.base))])
    }

    fn supports_latest(&self) -> bool {
        true
    }

    fn filters(&self) -> Vec<Filter> {
        vec![
            Filter::select(
                "type",
                "Type",
                vec![
                    option("manga", "Manga"),
                    option("novel", "Novel"),
                    option("one-shot", "One-shot"),
                    option("doujinshi", "Doujinshi"),
                    option("manhua", "Manhua"),
                    option("oel", "OEL"),
                ],
            ),
            Filter::select(
                "status",
                "Status",
                vec![
                    option("publishing", "Publishing"),
                    option("finished", "Finished"),
                    option("on hiatus", "On hiatus"),
                    option("discontinued", "Discontinued"),
                    option("not yet published", "Not yet published"),
                ],
            ),
            Filter::multi(
                "genre",
                "Genres",
                GENRES
                    .iter()
                    .map(|genre| FilterOption {
                        value: genre,
                        label: genre,
                    })
                    .collect(),
            ),
        ]
    }

    async fn popular(&self, http: &Http, page: u32) -> Result<MangasPage> {
        // MangaPill has no popularity listing; like Aniyomi's extension,
        // browse the search index in the site's own order.
        let url = self.search_url("", page, &FilterValues::default())?;
        self.fetch_search(http, &url).await
    }

    async fn latest(&self, http: &Http, _page: u32) -> Result<MangasPage> {
        let html = http
            .get_text(&format!("{}/chapters", self.base), &self.headers())
            .await?;
        Ok(self.parse_latest(&html))
    }

    async fn search(
        &self,
        http: &Http,
        query: &str,
        page: u32,
        filters: &FilterValues,
    ) -> Result<MangasPage> {
        let url = self.search_url(query, page, filters)?;
        self.fetch_search(http, &url).await
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Manga> {
        let url = util::absolutize(&self.base, url);
        let html = http.get_text(&url, &self.headers()).await?;
        self.parse_details(&html, &url)
    }

    async fn chapters(&self, http: &Http, manga: &Manga) -> Result<Vec<Chapter>> {
        let html = http.get_text(&manga.url, &self.headers()).await?;
        Ok(self.parse_chapters(&html))
    }

    async fn pages(&self, http: &Http, chapter: &Chapter) -> Result<Vec<Page>> {
        let html = http.get_text(&chapter.url, &self.headers()).await?;
        let pages = self.parse_pages(&html);
        if pages.is_empty() {
            return Err(Error::provider(
                "mangapill",
                format!("no page images on {}", chapter.url),
            ));
        }
        Ok(pages)
    }
}

/// `/manga/5573/bocchi-the-rock` -> `5573`.
fn manga_id(href: &str) -> Option<String> {
    let path = Url::parse(href)
        .map(|url| url.path().to_string())
        .unwrap_or_else(|_| href.to_string());
    let mut segments = path.trim_start_matches('/').split('/');
    (segments.next()? == "manga")
        .then(|| segments.next())
        .flatten()
        .filter(|id| id.chars().all(|c| c.is_ascii_digit()) && !id.is_empty())
        .map(str::to_string)
}

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("static selector")
}

fn text(element: ElementRef<'_>) -> String {
    util::strip_html(&element.text().collect::<Vec<_>>().join(" "))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> MangaPill {
        MangaPill::new(&MangaPillConfig::default()).unwrap()
    }

    const SEARCH: &str = r#"<div class="my-3 grid">
      <div>
        <a href="/manga/5573/bocchi-the-rock" class="relative block">
          <figure><img data-src="https://cdn.example/i/5573.jpeg" alt="Bocchi"/></figure>
        </a>
        <div class="flex flex-col justify-end">
          <a href="/manga/5573/bocchi-the-rock" class="mb-2">
            <div class="mt-3 font-black leading-tight line-clamp-2">Bocchi the Rock!</div>
          </a>
          <div class="flex flex-wrap gap-1 mt-1">
            <div class="text-xs">manga</div><div class="text-xs">2017</div><div class="text-xs">publishing</div>
          </div>
        </div>
      </div>
    </div>
    <a href="/search?q=bocchi&amp;page=2" class="btn btn-sm">Next</a>"#;

    #[test]
    fn parses_search_cards_and_next_page() {
        let page = source().parse_search(SEARCH);
        assert_eq!(page.mangas.len(), 1);
        let manga = &page.mangas[0];
        assert_eq!(manga.id, "5573");
        assert_eq!(manga.title, "Bocchi the Rock!");
        assert_eq!(
            manga.url,
            "https://mangapill.com/manga/5573/bocchi-the-rock"
        );
        assert_eq!(manga.year, Some(2017));
        assert_eq!(manga.status, MangaStatus::Ongoing);
        assert_eq!(manga.format.as_deref(), Some("manga"));
        assert!(page.has_next_page);
    }

    #[test]
    fn parses_details_chapters_and_pages() {
        let html = r#"<h1 class="font-bold">Bocchi the Rock!</h1>
          <p class="text-sm text--secondary">Hitori Gotou is a girl&#39;s guitarist.</p>
          <div><label class="text-secondary">Status</label><div>publishing</div></div>
          <div><label class="text-secondary">Year</label><div>2017</div></div>
          <a class="text-sm" href="/search?genre=Comedy">Comedy</a>
          <div id="chapters">
            <a href="/chapters/5573-10096000/bocchi-the-rock-chapter-96" title=" Chapter 96">Chapter 96</a>
            <a href="/chapters/5573-10010500/bocchi-the-rock-chapter-10.5">Chapter 10.5</a>
          </div>"#;
        let source = source();
        let manga = source
            .parse_details(html, "https://mangapill.com/manga/5573/bocchi-the-rock")
            .unwrap();
        assert_eq!(manga.id, "5573");
        assert_eq!(manga.status, MangaStatus::Ongoing);
        assert_eq!(manga.year, Some(2017));
        assert_eq!(manga.genres, ["Comedy"]);
        assert_eq!(
            manga.description.as_deref(),
            Some("Hitori Gotou is a girl's guitarist.")
        );

        let chapters = source.parse_chapters(html);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].id, "5573-10096000");
        assert_eq!(chapters[0].number, 96.0);
        assert_eq!(chapters[1].number, 10.5);

        let pages = source.parse_pages(
            r#"<img class="js-page" data-src="https://cdn.example/5573/1.jpeg"/>
               <img class="js-page" data-src="https://cdn.example/5573/2.jpeg"/>"#,
        );
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[1].index, 1);
        assert_eq!(pages[0].headers["Referer"], "https://mangapill.com/");
    }
}
