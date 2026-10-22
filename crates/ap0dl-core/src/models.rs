use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A search hit. Providers return these before details are fetched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub url: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episodes: Option<u32>,
}

impl SearchResult {
    pub fn into_anime(self) -> Anime {
        Anime {
            id: self.id,
            title: self.title,
            url: self.url,
            provider: self.provider,
            cover: self.cover,
            description: None,
            genres: Vec::new(),
            year: self.year,
            format: self.format,
            status: None,
            total_episodes: self.episodes,
            anilist_id: None,
            mal_id: None,
        }
    }
}

/// Full anime metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anime {
    pub id: String,
    pub title: String,
    pub url: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_episodes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anilist_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mal_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Audio {
    Sub,
    Dub,
    Both,
    Raw,
}

impl Audio {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sub => "Sub",
            Self::Dub => "Dub",
            Self::Both => "Sub & Dub",
            Self::Raw => "Raw",
        }
    }
}

/// One playable episode entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// Episode number; floats are supported for 12.5-style specials.
    pub number: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub filler: bool,
    #[serde(default)]
    pub recap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aired: Option<String>,
    pub audio: Audio,
}

impl Episode {
    pub fn label(&self) -> String {
        if self.number.fract() == 0.0 {
            format!("Episode {}", self.number as i64)
        } else {
            format!("Episode {}", self.number)
        }
    }

    pub fn display_title(&self) -> String {
        match self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(title) if !title.to_ascii_lowercase().contains("episode") => {
                format!("{} - {}", self.label(), title)
            }
            _ => self.label(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtitle {
    pub url: String,
    pub lang: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub start: f64,
    pub end: f64,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    Hls,
    Direct,
    Dash,
}

/// Provider-specific data needed by the local flixcloud proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlixInfo {
    pub master_url: String,
    pub w_payload: String,
    pub referer: String,
    #[serde(default)]
    pub xor_mask: Vec<u8>,
}

/// A resolved stream. `url` is always playable by a normal media player; for
/// flixcloud HLS it is the enc-dec proxy master and `flix` contains the data
/// needed to serve it through [`crate::proxy::FlixProxy`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stream {
    pub url: String,
    /// Media container hint when the URL itself does not carry an extension
    /// (for example AnimeToki's `?a=download&id=...` MKV endpoints).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub audio: Audio,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<Subtitle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<Chapter>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    pub kind: StreamKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flix: Option<FlixInfo>,
}

impl Stream {
    pub fn display(&self) -> String {
        let mut out = self
            .title
            .clone()
            .or_else(|| self.server.clone())
            .unwrap_or_else(|| "stream".to_string());
        if let Some(quality) = self.quality {
            out.push_str(&format!(" [{quality}p]"));
        }
        if !self.subtitles.is_empty() {
            out.push_str(" (subtitles)");
        }
        out
    }
}
