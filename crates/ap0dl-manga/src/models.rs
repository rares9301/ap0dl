use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Publication status. Mirrors Aniyomi's `SManga` status constants so source
/// adapters ported from Aniyomi extensions map one-to-one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MangaStatus {
    #[default]
    Unknown,
    Ongoing,
    Completed,
    Licensed,
    PublishingFinished,
    Cancelled,
    OnHiatus,
}

impl MangaStatus {
    /// Parses the free-form status strings sites use ("publishing",
    /// "Ongoing", "hiatus", "finished", ...).
    pub fn parse(value: &str) -> Self {
        let value = value.trim().to_ascii_lowercase();
        match value.as_str() {
            "ongoing" | "publishing" | "releasing" | "on going" => Self::Ongoing,
            "completed" | "complete" | "finished" | "ended" => Self::Completed,
            "licensed" => Self::Licensed,
            "publishing finished" | "publishing_finished" => Self::PublishingFinished,
            "cancelled" | "canceled" | "discontinued" | "dropped" => Self::Cancelled,
            "hiatus" | "on hiatus" | "on_hiatus" => Self::OnHiatus,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Ongoing => "Ongoing",
            Self::Completed => "Completed",
            Self::Licensed => "Licensed",
            Self::PublishingFinished => "Publishing finished",
            Self::Cancelled => "Cancelled",
            Self::OnHiatus => "On hiatus",
        }
    }
}

/// One manga entry (Aniyomi's `SManga`). Search results and browse pages
/// return partially filled entries; [`crate::MangaSource::details`] fills the
/// rest and sets `initialized`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manga {
    /// Source-local identifier (MangaDex UUID, MangaPill numeric id, ...).
    pub id: String,
    pub title: String,
    /// Absolute URL of the manga page on the source site.
    pub url: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alt_titles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(default)]
    pub status: MangaStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default)]
    pub initialized: bool,
}

impl Manga {
    pub fn new(
        source: &str,
        id: impl Into<String>,
        title: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            url: url.into(),
            source: source.to_string(),
            alt_titles: Vec::new(),
            author: None,
            artist: None,
            description: None,
            genres: Vec::new(),
            status: MangaStatus::Unknown,
            thumbnail_url: None,
            year: None,
            format: None,
            initialized: false,
        }
    }
}

/// A page of browse/search results (Aniyomi's `MangasPage`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MangasPage {
    pub mangas: Vec<Manga>,
    pub has_next_page: bool,
}

/// One chapter entry (Aniyomi's `SChapter`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    /// Absolute URL of the chapter reader page.
    pub url: String,
    pub name: String,
    /// Parsed chapter number; `-1.0` when the source gives none (Aniyomi
    /// convention). Fractions are kept for `10.5`-style extras.
    pub number: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanlator: Option<String>,
    /// Upload time in RFC 3339, when the source exposes one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_upload: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,
}

impl Chapter {
    pub fn label(&self) -> String {
        if self.number < 0.0 {
            return self.name.clone();
        }
        if self.number.fract() == 0.0 {
            format!("Chapter {}", self.number as i64)
        } else {
            format!("Chapter {}", self.number)
        }
    }

    pub fn display_title(&self) -> String {
        static NUMBER_PREFIX: OnceLock<Regex> = OnceLock::new();
        // Drops "Vol.1 Ch.3 - " / "Chapter 3: " style prefixes that repeat
        // what `label()` already says.
        let prefix = NUMBER_PREFIX.get_or_init(|| {
            Regex::new(r"(?i)^(vol(ume)?\.?\s*\S+\s*)?(ch(apter)?\.?\s*[\d.]+)\s*([-:]\s*)?")
                .expect("static regex")
        });
        let mut out = self.label();
        let name = if self.number >= 0.0 {
            prefix.replace(self.name.trim(), "").trim().to_string()
        } else {
            self.name.trim().to_string()
        };
        let name = name.as_str();
        if !name.is_empty() && !name.eq_ignore_ascii_case(&out) && !name.contains(&out) {
            out.push_str(" - ");
            out.push_str(name);
        }
        if let Some(scanlator) = self.scanlator.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!(" [{scanlator}]"));
        }
        out
    }
}

/// One page image of a chapter (Aniyomi's `Page`). `image_url` is filled
/// immediately by most sources; sources that need a second request per page
/// leave it empty and implement [`crate::MangaSource::image_url`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// Zero-based page index.
    pub index: usize,
    /// Page URL in the reader, when distinct from the image.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_url: String,
    /// Mirrors tried in order when `image_url` fails.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_urls: Vec<String>,
    /// Request headers required by the image CDN (usually `Referer`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

impl Page {
    pub fn image(index: usize, image_url: impl Into<String>) -> Self {
        Self {
            index,
            url: String::new(),
            image_url: image_url.into(),
            fallback_urls: Vec::new(),
            headers: BTreeMap::new(),
        }
    }

    pub fn with_headers(mut self, headers: &BTreeMap<String, String>) -> Self {
        self.headers = headers.clone();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(number: f32, name: &str) -> Chapter {
        Chapter {
            id: "1".to_string(),
            url: "https://example.test/1".to_string(),
            name: name.to_string(),
            number,
            volume: None,
            scanlator: None,
            date_upload: None,
            language: None,
            pages: None,
        }
    }

    #[test]
    fn status_parsing_matches_aniyomi_buckets() {
        assert_eq!(MangaStatus::parse("publishing"), MangaStatus::Ongoing);
        assert_eq!(MangaStatus::parse("Completed"), MangaStatus::Completed);
        assert_eq!(MangaStatus::parse("hiatus"), MangaStatus::OnHiatus);
        assert_eq!(MangaStatus::parse("whatever"), MangaStatus::Unknown);
    }

    #[test]
    fn chapter_titles_do_not_repeat_the_number() {
        assert_eq!(chapter(1.0, "Chapter 1").display_title(), "Chapter 1");
        assert_eq!(
            chapter(10.5, "Extra").display_title(),
            "Chapter 10.5 - Extra"
        );
        assert_eq!(chapter(-1.0, "Oneshot").display_title(), "Oneshot");
        assert_eq!(chapter(3.0, "Vol.1 Ch.3").display_title(), "Chapter 3");
        assert_eq!(
            chapter(3.0, "Vol.1 Ch.3 - Hello").display_title(),
            "Chapter 3 - Hello"
        );
        assert_eq!(chapter(96.0, "Chapter 96").display_title(), "Chapter 96");
    }
}
