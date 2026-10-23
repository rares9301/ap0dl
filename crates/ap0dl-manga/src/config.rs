use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use ap0dl_core::Config;
use ap0dl_core::error::{Error, Result};

use crate::spread::{PageLayout, ReadingDirection};

/// How downloaded chapters are stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChapterFormat {
    /// One `.cbz` per chapter with a `ComicInfo.xml`, like Aniyomi/Mihon.
    #[default]
    Cbz,
    /// One directory of numbered images per chapter.
    Folder,
}

impl std::str::FromStr for ChapterFormat {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cbz" | "zip" => Ok(Self::Cbz),
            "folder" | "dir" | "images" => Ok(Self::Folder),
            other => Err(format!(
                "unknown chapter format {other:?}; use cbz or folder"
            )),
        }
    }
}

/// The `[manga]` table of the shared ap0dl config file.
///
/// The plugin reads its own table from the file the core config loader
/// found, so enabling or removing the plugin never changes the anime
/// [`Config`] type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MangaConfig {
    pub default_source: String,
    /// Download root. Empty means the anime `download_dir`.
    pub download_dir: Option<PathBuf>,
    pub format: ChapterFormat,
    /// Image viewer. mpv-like players get image URLs plus headers directly;
    /// anything else receives a temporary folder of downloaded pages.
    pub reader: String,
    pub reader_args: Vec<String>,
    /// mpv viewer layout: `double` pairs portrait pages into spreads (needs
    /// ffmpeg), `single` shows one page at a time.
    pub page_layout: PageLayout,
    /// `rtl` (manga) puts the first page of a spread on the right and makes
    /// the Left arrow turn forward; `ltr` is the comic order.
    pub reading_direction: ReadingDirection,
    /// Concurrent image downloads per chapter.
    pub concurrency: usize,
    /// Path of the library JSON. Empty means `<data dir>/ap0dl/manga-library.json`.
    pub library_path: Option<PathBuf>,
    /// Path of the reading-state JSON (chapter and page per series). Empty
    /// means `<config dir>/ap0dl/manga-state.json`.
    pub state_path: Option<PathBuf>,
    pub sources: SourcesConfig,
}

impl Default for MangaConfig {
    fn default() -> Self {
        Self {
            default_source: "mangadex".to_string(),
            download_dir: None,
            format: ChapterFormat::Cbz,
            reader: "mpv".to_string(),
            reader_args: Vec::new(),
            page_layout: PageLayout::Double,
            reading_direction: ReadingDirection::Rtl,
            concurrency: 4,
            library_path: None,
            state_path: None,
            sources: SourcesConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SourcesConfig {
    pub mangadex: MangaDexConfig,
    pub mangapill: MangaPillConfig,
    pub weebcentral: WeebCentralConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MangaDexConfig {
    pub base_url: String,
    pub api_url: String,
    pub uploads_url: String,
    /// Chapter languages (ISO 639-1, MangaDex spelling such as `pt-br`).
    pub languages: Vec<String>,
    pub content_rating: Vec<String>,
    /// Use MangaDex's compressed "data-saver" images.
    pub data_saver: bool,
}

impl Default for MangaDexConfig {
    fn default() -> Self {
        Self {
            base_url: "https://mangadex.org".to_string(),
            api_url: "https://api.mangadex.org".to_string(),
            uploads_url: "https://uploads.mangadex.org".to_string(),
            languages: vec!["en".to_string()],
            content_rating: vec!["safe".to_string(), "suggestive".to_string()],
            data_saver: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MangaPillConfig {
    pub base_url: String,
}

impl Default for MangaPillConfig {
    fn default() -> Self {
        Self {
            base_url: "https://mangapill.com".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WeebCentralConfig {
    pub base_url: String,
}

impl Default for WeebCentralConfig {
    fn default() -> Self {
        Self {
            base_url: "https://weebcentral.com".to_string(),
        }
    }
}

impl MangaConfig {
    /// Loads defaults, then the `[manga]` table of the config file the core
    /// loader uses, then `AP0DL_MANGA*` environment overrides.
    pub fn load() -> Result<Self> {
        let mut config = match Config::find_config_file() {
            Some(path) => {
                let text = std::fs::read_to_string(&path).map_err(|error| {
                    Error::config(format!("could not read {}: {error}", path.display()))
                })?;
                Self::from_toml(&text).map_err(|error| {
                    Error::config(format!(
                        "invalid [manga] table in {}: {error}",
                        path.display()
                    ))
                })?
            }
            None => Self::default(),
        };
        config.apply_env();
        config.validate()?;
        Ok(config)
    }

    /// Parses the `[manga]` table of a whole ap0dl config document. Missing
    /// keys keep their defaults; other tables are ignored.
    pub fn from_toml(text: &str) -> std::result::Result<Self, String> {
        let document: toml::Value = toml::from_str(text).map_err(|error| error.to_string())?;
        match document.get("manga") {
            Some(table) => table.clone().try_into().map_err(|error| error.to_string()),
            None => Ok(Self::default()),
        }
    }

    fn apply_env(&mut self) {
        if let Ok(value) = std::env::var("AP0DL_MANGA_DEFAULT_SOURCE") {
            self.default_source = value;
        }
        if let Ok(value) = std::env::var("AP0DL_MANGA_DIR") {
            self.download_dir = Some(PathBuf::from(value));
        }
        if let Some(value) = std::env::var("AP0DL_MANGA_FORMAT")
            .ok()
            .and_then(|value| value.parse().ok())
        {
            self.format = value;
        }
        if let Ok(value) = std::env::var("AP0DL_MANGA_READER") {
            self.reader = value;
        }
        if let Ok(value) = std::env::var("AP0DL_MANGA_LIBRARY") {
            self.library_path = Some(PathBuf::from(value));
        }
        if let Ok(value) = std::env::var("AP0DL_MANGA_STATE") {
            self.state_path = Some(PathBuf::from(value));
        }
        if let Some(value) = std::env::var("AP0DL_MANGA_LAYOUT")
            .ok()
            .and_then(|value| value.parse().ok())
        {
            self.page_layout = value;
        }
        if let Some(value) = std::env::var("AP0DL_MANGA_DIRECTION")
            .ok()
            .and_then(|value| value.parse().ok())
        {
            self.reading_direction = value;
        }
        if let Ok(value) = std::env::var("AP0DL_MANGADEX_BASE") {
            self.sources.mangadex.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_MANGADEX_API") {
            self.sources.mangadex.api_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_MANGADEX_UPLOADS") {
            self.sources.mangadex.uploads_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_MANGADEX_LANGUAGES") {
            self.sources.mangadex.languages = value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
        }
        if let Ok(value) = std::env::var("AP0DL_MANGAPILL_BASE") {
            self.sources.mangapill.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_WEEBCENTRAL_BASE") {
            self.sources.weebcentral.base_url = value;
        }
    }

    fn validate(&self) -> Result<()> {
        if self.concurrency == 0 {
            return Err(Error::config("manga.concurrency must be at least 1"));
        }
        if self.sources.mangadex.languages.is_empty() {
            return Err(Error::config(
                "manga.sources.mangadex.languages must list at least one language",
            ));
        }
        Ok(())
    }

    pub fn download_root(&self, core: &Config) -> PathBuf {
        self.download_dir
            .clone()
            .unwrap_or_else(|| core.download_dir.clone())
    }

    pub fn library_file(&self) -> PathBuf {
        self.library_path.clone().unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ap0dl")
                .join("manga-library.json")
        })
    }

    pub fn state_file(&self) -> PathBuf {
        self.state_path.clone().unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ap0dl")
                .join("manga-state.json")
        })
    }

    pub fn sample() -> &'static str {
        r#"# Manga plugin settings. Lives in the same file as the anime settings.
[manga]
default_source = "mangadex"   # breaks ties between equally good matches
# download_dir = "/home/you/Manga"  # defaults to the anime download_dir
format = "cbz"                # cbz (with ComicInfo.xml) or folder
reader = "mpv"                # mpv/mpv.exe get URLs; other viewers get a temp folder
reader_args = []
page_layout = "double"        # double (two-page spreads, needs ffmpeg) or single
reading_direction = "rtl"     # rtl (manga: Left arrow turns forward) or ltr
concurrency = 4               # parallel page downloads per chapter
# library_path = "/home/you/.local/share/ap0dl/manga-library.json"
# state_path = "/home/you/.config/ap0dl/manga-state.json"  # last chapter/page per series

[manga.sources.mangadex]
base_url = "https://mangadex.org"
api_url = "https://api.mangadex.org"
uploads_url = "https://uploads.mangadex.org"
languages = ["en"]
content_rating = ["safe", "suggestive"]
data_saver = false

[manga.sources.mangapill]
base_url = "https://mangapill.com"

[manga.sources.weebcentral]
base_url = "https://weebcentral.com"
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manga_table_overrides_defaults_and_ignores_anime_keys() {
        let config = MangaConfig::from_toml(
            r#"
default_provider = "anidb"
[providers.reanime]
base_url = "https://example.test"
[manga]
format = "folder"
concurrency = 2
page_layout = "single"
reading_direction = "ltr"
[manga.sources.mangadex]
languages = ["en", "pt-br"]
"#,
        )
        .unwrap();
        assert_eq!(config.format, ChapterFormat::Folder);
        assert_eq!(config.concurrency, 2);
        assert_eq!(config.page_layout, PageLayout::Single);
        assert_eq!(config.reading_direction, ReadingDirection::Ltr);
        assert_eq!(config.sources.mangadex.languages, ["en", "pt-br"]);
        assert_eq!(config.sources.mangapill.base_url, "https://mangapill.com");
    }

    #[test]
    fn missing_manga_table_uses_defaults() {
        let config = MangaConfig::from_toml("quality = \"best\"\n").unwrap();
        assert_eq!(config.default_source, "mangadex");
        assert_eq!(config.format, ChapterFormat::Cbz);
        assert_eq!(config.page_layout, PageLayout::Double);
        assert_eq!(config.reading_direction, ReadingDirection::Rtl);
    }

    #[test]
    fn sample_parses() {
        let config = MangaConfig::from_toml(MangaConfig::sample()).unwrap();
        assert_eq!(
            config.sources.weebcentral.base_url,
            "https://weebcentral.com"
        );
    }
}
