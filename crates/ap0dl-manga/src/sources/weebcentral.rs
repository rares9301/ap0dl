use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use ap0dl_core::error::{Error, Result};
use ap0dl_core::http::{Http, headers};
use ap0dl_core::util;

use crate::config::WeebCentralConfig;
use crate::filters::{Filter, FilterOption, FilterValues, option};
use crate::models::{Chapter, Manga, MangaStatus, MangasPage, Page};
use crate::source::{MangaSource, SourceStatus, host_matches};

const PAGE_SIZE: u32 = 32;

const TAGS: [&str; 32] = [
    "Action",
    "Adventure",
    "Comedy",
    "Doujinshi",
    "Drama",
    "Ecchi",
    "Fantasy",
    "Gender Bender",
    "Harem",
    "Historical",
    "Horror",
    "Isekai",
    "Josei",
    "Martial Arts",
    "Mature",
    "Mecha",
    "Mystery",
    "Psychological",
    "Romance",
    "School Life",
    "Sci-fi",
    "Seinen",
    "Shoujo",
    "Shoujo Ai",
    "Shounen",
    "Shounen Ai",
    "Slice of Life",
    "Sports",
    "Supernatural",
    "Tragedy",
    "Yaoi",
    "Yuri",
];

/// Weeb Central (the MangaSee/MangaLife successor). The site is htmx-driven:
/// search results, the full chapter list and the reader images are HTML
/// fragments from dedicated endpoints, as used by the Aniyomi/Keiyoushi
/// Weeb Central extension.
pub struct WeebCentral {
    base: String,
}

impl WeebCentral {
    pub fn new(config: &WeebCentralConfig) -> Result<Self> {
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
            // htmx fragments are served for any request, but the header keeps
            // the site from wrapping them in the full layout.
            ("HX-Request", "true"),
        ])
    }

    fn search_url(
        &self,
        query: &str,
        page: u32,
        sort: &str,
        filters: &FilterValues,
    ) -> Result<String> {
        let mut url = Url::parse(&format!("{}/search/data", self.base))
            .map_err(|error| Error::Url(self.base.clone(), error))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("limit", &PAGE_SIZE.to_string());
            pairs.append_pair("offset", &(page.saturating_sub(1) * PAGE_SIZE).to_string());
            pairs.append_pair("text", query.trim());
            pairs.append_pair("sort", sort);
            pairs.append_pair(
                "order",
                if sort == "Alphabet" {
                    "Ascending"
                } else {
                    "Descending"
                },
            );
            pairs.append_pair("official", filters.get("official").unwrap_or("Any"));
            pairs.append_pair("anime", "Any");
            pairs.append_pair("adult", filters.get("adult").unwrap_or("False"));
            for status in filters.all("status") {
                pairs.append_pair("included_status", status);
            }
            for kind in filters.all("type") {
                pairs.append_pair("included_type", kind);
            }
            for tag in filters.all("tag") {
                pairs.append_pair("included_tag", tag);
            }
            pairs.append_pair("display_mode", "Full Display");
        }
        Ok(url.to_string())
    }

    async fn fetch_search(&self, http: &Http, url: &str) -> Result<MangasPage> {
        let html = http.get_text(url, &self.headers()).await?;
        Ok(self.parse_search(&html))
    }

    fn parse_search(&self, html: &str) -> MangasPage {
        let document = Html::parse_fragment(html);
        let card = selector("article.bg-base-300");
        let link = selector("a.line-clamp-1[href*='/series/']");
        let image = selector("img[src]");
        let field = selector("div");
        let strong = selector("strong");
        let span = selector("span");

        let mut seen = HashSet::new();
        let mut mangas = Vec::new();
        for card in document.select(&card) {
            let Some(anchor) = card.select(&link).next() else {
                continue;
            };
            let Some(href) = anchor.value().attr("href") else {
                continue;
            };
            let Some(id) = series_id(href) else {
                continue;
            };
            let title = text(anchor);
            if title.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            let mut manga =
                Manga::new("weebcentral", id, title, util::absolutize(&self.base, href));
            manga.thumbnail_url = card
                .select(&image)
                .next()
                .and_then(|image| image.value().attr("src"))
                .map(str::to_string);
            for row in card.select(&field) {
                let Some(label) = row.select(&strong).next().map(text) else {
                    continue;
                };
                let values: Vec<String> = row
                    .select(&span)
                    .map(text)
                    .map(|value| value.trim_end_matches(',').trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect();
                match label.trim_end_matches(':').trim() {
                    "Year" => manga.year = values.first().and_then(|v| util::parse_u32(v)),
                    "Status" => {
                        manga.status = values
                            .first()
                            .map(|v| MangaStatus::parse(v))
                            .unwrap_or_default()
                    }
                    "Type" => manga.format = values.first().cloned(),
                    "Author(s)" => manga.author = Some(values.join(", ")).filter(|v| !v.is_empty()),
                    "Tag(s)" => manga.genres = values,
                    _ => {}
                }
            }
            mangas.push(manga);
        }
        let has_next_page = document
            .select(&selector("button[hx-get*='/search/data']"))
            .next()
            .is_some();
        MangasPage {
            mangas,
            has_next_page,
        }
    }

    fn parse_details(&self, html: &str, url: &str) -> Result<Manga> {
        let document = Html::parse_document(html);
        let title = document
            .select(&selector("h1"))
            .next()
            .map(text)
            .filter(|title| !title.is_empty())
            .ok_or_else(|| Error::Html(url.to_string(), "missing series title".to_string()))?;
        let id = series_id(url).unwrap_or_else(|| url.to_string());
        let mut manga = Manga::new("weebcentral", id, title, url);
        manga.thumbnail_url = document
            .select(&selector("meta[property='og:image']"))
            .next()
            .and_then(|meta| meta.value().attr("content"))
            .map(str::to_string);
        manga.description = document
            .select(&selector("p.whitespace-pre-wrap"))
            .next()
            .map(text)
            .filter(|value| !value.is_empty());

        let link = selector("a");
        let span = selector("span");
        for item in document.select(&selector("li")) {
            let Some(label) = item.select(&selector("strong")).next().map(text) else {
                continue;
            };
            let links: Vec<String> = item
                .select(&link)
                .map(text)
                .filter(|value| !value.is_empty())
                .collect();
            let spans: Vec<String> = item
                .select(&span)
                .map(text)
                .map(|value| value.trim_end_matches(',').trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            match label.trim_end_matches(':').trim() {
                "Author(s)" => manga.author = Some(links.join(", ")).filter(|v| !v.is_empty()),
                "Tags(s)" | "Tag(s)" => manga.genres = links,
                "Type" => manga.format = links.into_iter().next(),
                "Status" => {
                    manga.status = links
                        .first()
                        .map(|value| MangaStatus::parse(value))
                        .unwrap_or_default()
                }
                "Released" => manga.year = spans.first().and_then(|v| util::parse_u32(v)),
                "Associated Name(s)" => {
                    manga.alt_titles = item
                        .select(&selector("li"))
                        .map(text)
                        .filter(|value| !value.is_empty())
                        .collect()
                }
                _ => {}
            }
        }
        manga.initialized = true;
        Ok(manga)
    }

    fn parse_chapters(&self, html: &str) -> Vec<Chapter> {
        let document = Html::parse_fragment(html);
        let name = selector("span.grow > span");
        let time = selector("time[datetime]");
        document
            .select(&selector("a[href*='/chapters/']"))
            .filter_map(|link| {
                let href = link.value().attr("href")?;
                let id = href.rsplit('/').next()?.to_string();
                if id.is_empty() {
                    return None;
                }
                let title = link
                    .select(&name)
                    .next()
                    .map(text)
                    .unwrap_or_else(|| text(link));
                Some(Chapter {
                    id,
                    url: util::absolutize(&self.base, href),
                    number: util::last_number(&title).unwrap_or(-1.0),
                    name: title,
                    volume: None,
                    scanlator: None,
                    date_upload: link
                        .select(&time)
                        .next()
                        .and_then(|time| time.value().attr("datetime"))
                        .map(str::to_string),
                    language: Some("en".to_string()),
                    pages: None,
                })
            })
            .collect()
    }

    fn parse_pages(&self, html: &str) -> Vec<Page> {
        let document = Html::parse_fragment(html);
        let headers = self.image_headers();
        document
            .select(&selector("img[src]"))
            .filter_map(|image| image.value().attr("src"))
            .filter(|src| src.starts_with("http") && !src.contains("/static/"))
            .enumerate()
            .map(|(index, src)| Page::image(index, src).with_headers(&headers))
            .collect()
    }
}

#[async_trait]
impl MangaSource for WeebCentral {
    fn id(&self) -> &'static str {
        "weebcentral"
    }

    fn name(&self) -> &'static str {
        "Weeb Central"
    }

    fn base_url(&self) -> &str {
        &self.base
    }

    fn status(&self) -> SourceStatus {
        SourceStatus::Stable
    }

    fn notes(&self) -> &'static str {
        "htmx fragments for search, chapter list and reader images"
    }

    fn matches(&self, url: &Url) -> bool {
        host_matches(url, "weebcentral.com")
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
                "sort",
                "Sort",
                vec![
                    option("Best Match", "Best match"),
                    option("Popularity", "Popular"),
                    option("Subscribers", "Subscribers"),
                    option("Latest Updates", "Latest updates"),
                    option("Recently Added", "Recently added"),
                    option("Alphabet", "Title"),
                ],
            ),
            Filter::multi(
                "status",
                "Status",
                vec![
                    option("Ongoing", "Ongoing"),
                    option("Complete", "Complete"),
                    option("Hiatus", "Hiatus"),
                    option("Canceled", "Canceled"),
                ],
            ),
            Filter::multi(
                "type",
                "Type",
                vec![
                    option("Manga", "Manga"),
                    option("Manhwa", "Manhwa"),
                    option("Manhua", "Manhua"),
                    option("OEL", "OEL"),
                ],
            ),
            Filter::multi(
                "tag",
                "Tags",
                TAGS.iter()
                    .map(|tag| FilterOption {
                        value: tag,
                        label: tag,
                    })
                    .collect(),
            ),
            Filter::select(
                "official",
                "Official translation",
                vec![
                    option("Any", "Any"),
                    option("True", "Yes"),
                    option("False", "No"),
                ],
            ),
            Filter::select(
                "adult",
                "Adult content",
                vec![
                    option("False", "Hide"),
                    option("Any", "Show"),
                    option("True", "Only"),
                ],
            ),
        ]
    }

    async fn popular(&self, http: &Http, page: u32) -> Result<MangasPage> {
        let url = self.search_url("", page, "Popularity", &FilterValues::default())?;
        self.fetch_search(http, &url).await
    }

    async fn latest(&self, http: &Http, page: u32) -> Result<MangasPage> {
        let url = self.search_url("", page, "Latest Updates", &FilterValues::default())?;
        self.fetch_search(http, &url).await
    }

    async fn search(
        &self,
        http: &Http,
        query: &str,
        page: u32,
        filters: &FilterValues,
    ) -> Result<MangasPage> {
        let sort = filters.get("sort").unwrap_or(if query.trim().is_empty() {
            "Popularity"
        } else {
            "Best Match"
        });
        let url = self.search_url(query, page, sort, filters)?;
        self.fetch_search(http, &url).await
    }

    async fn details(&self, http: &Http, url: &str) -> Result<Manga> {
        let url = util::absolutize(&self.base, url);
        let html = http.get_text(&url, &self.headers()).await?;
        self.parse_details(&html, &url)
    }

    async fn chapters(&self, http: &Http, manga: &Manga) -> Result<Vec<Chapter>> {
        let id = series_id(&manga.url).unwrap_or_else(|| manga.id.clone());
        let url = format!("{}/series/{id}/full-chapter-list", self.base);
        let html = http.get_text(&url, &self.headers()).await?;
        Ok(self.parse_chapters(&html))
    }

    async fn pages(&self, http: &Http, chapter: &Chapter) -> Result<Vec<Page>> {
        let url = format!(
            "{}/chapters/{}/images?is_prev=False&current_page=1&reading_style=long_strip",
            self.base, chapter.id
        );
        let html = http.get_text(&url, &self.headers()).await?;
        let pages = self.parse_pages(&html);
        if pages.is_empty() {
            return Err(Error::provider(
                "weebcentral",
                format!("no page images for chapter {}", chapter.id),
            ));
        }
        Ok(pages)
    }
}

/// `https://weebcentral.com/series/01J76XYEDHDJ7NA7WJ1WG3Y42S/Bocchi-the-Rock`
/// -> `01J76XYEDHDJ7NA7WJ1WG3Y42S`.
fn series_id(href: &str) -> Option<String> {
    let path = Url::parse(href)
        .map(|url| url.path().to_string())
        .unwrap_or_else(|_| href.to_string());
    let mut segments = path.trim_start_matches('/').split('/');
    (segments.next()? == "series")
        .then(|| segments.next())
        .flatten()
        .filter(|id| !id.is_empty())
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

    fn source() -> WeebCentral {
        WeebCentral::new(&WeebCentralConfig::default()).unwrap()
    }

    #[test]
    fn parses_search_fragment_and_view_more_button() {
        let html = r#"<article class="bg-base-300 flex gap-4 p-4">
          <section><a href="https://weebcentral.com/series/01J76XYEDHDJ7NA7WJ1WG3Y42S/Bocchi-the-Rock">
            <img src="https://temp.example/cover/fallback/01J76.jpg" alt="cover"></a></section>
          <section>
            <div><span class="tooltip"><a href="https://weebcentral.com/series/01J76XYEDHDJ7NA7WJ1WG3Y42S/Bocchi-the-Rock" class="line-clamp-1 link">Bocchi the Rock!</a></span></div>
            <div class="opacity-70"><strong>Year:</strong> <span>2018</span></div>
            <div class="opacity-70"><strong>Status:</strong> <span>Ongoing</span></div>
            <div class="opacity-70"><strong>Type:</strong> <span>Manga</span></div>
            <div class="opacity-70"><strong>Tag(s): </strong> <span>Comedy,</span> <span>Music</span></div>
          </section>
        </article>
        <button class="btn" hx-get="/search/data?limit=32&amp;offset=32&amp;text=bocchi">View More Results...</button>"#;
        let page = source().parse_search(html);
        assert!(page.has_next_page);
        assert_eq!(page.mangas.len(), 1);
        let manga = &page.mangas[0];
        assert_eq!(manga.id, "01J76XYEDHDJ7NA7WJ1WG3Y42S");
        assert_eq!(manga.title, "Bocchi the Rock!");
        assert_eq!(manga.year, Some(2018));
        assert_eq!(manga.status, MangaStatus::Ongoing);
        assert_eq!(manga.genres, ["Comedy", "Music"]);
    }

    #[test]
    fn parses_chapter_list_and_reader_images() {
        let chapters = source().parse_chapters(
            r#"<div class="flex items-center"><a href="/chapters/01M0NVEBJHA11CY61410MSGV7Q" class="flex-1">
                 <span class="grow flex"><span class="">Chapter 96</span><span>Last Read</span></span>
                 <time datetime="2026-08-22T23:03:40.370Z">x</time></a></div>
               <div class="flex items-center"><a href="https://weebcentral.com/chapters/01KX1">
                 <span class="grow flex"><span class="">Chapter 95.5</span></span></a></div>"#,
        );
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].id, "01M0NVEBJHA11CY61410MSGV7Q");
        assert_eq!(chapters[0].name, "Chapter 96");
        assert_eq!(chapters[0].number, 96.0);
        assert_eq!(
            chapters[0].date_upload.as_deref(),
            Some("2026-08-22T23:03:40.370Z")
        );
        assert_eq!(chapters[1].number, 95.5);
        assert_eq!(
            chapters[0].url,
            "https://weebcentral.com/chapters/01M0NVEBJHA11CY61410MSGV7Q"
        );

        let pages = source().parse_pages(
            r#"<section><img src="https://scans.example/manga/Bocchi/0096-001.png" />
               <img src="https://scans.example/manga/Bocchi/0096-002.png" />
               <img src="/static/images/broken_image.jpg" /></section>"#,
        );
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].headers["Referer"], "https://weebcentral.com/");
    }

    #[test]
    fn series_ids_come_from_urls() {
        assert_eq!(
            series_id("https://weebcentral.com/series/01J76/Bocchi-the-Rock").as_deref(),
            Some("01J76")
        );
        assert_eq!(series_id("https://weebcentral.com/chapters/01J76"), None);
    }
}
