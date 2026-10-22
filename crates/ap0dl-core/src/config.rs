use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_provider: String,
    pub quality: String,
    pub download_dir: PathBuf,
    pub player: String,
    pub player_args: Vec<String>,
    pub ffmpeg: String,
    pub merge_subtitles: bool,
    pub threaded_download: bool,
    pub auto_retry: u64,
    pub request_delay_ms: u64,
    pub timeout_secs: u64,
    pub use_aniskip: bool,
    pub providers: ProvidersConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_provider: "reanime".to_string(),
            quality: "best".to_string(),
            download_dir: PathBuf::from("."),
            player: "mpv".to_string(),
            player_args: Vec::new(),
            ffmpeg: "ffmpeg".to_string(),
            merge_subtitles: true,
            threaded_download: false,
            auto_retry: 300,
            request_delay_ms: 250,
            timeout_secs: 30,
            use_aniskip: false,
            providers: ProvidersConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub reanime: ReAnimeConfig,
    pub animegg: AnimeGGConfig,
    pub anidb: AniDBConfig,
    pub kisskh: KissKhConfig,
    pub animeparadise: AnimeParadiseConfig,
    pub animetoki: AnimeTokiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReAnimeConfig {
    pub base_url: String,
    pub enc_dec_url: String,
    pub preferred_audio: String,
}

impl Default for ReAnimeConfig {
    fn default() -> Self {
        Self {
            base_url: "https://reanime.to".to_string(),
            enc_dec_url: "https://enc-dec.app".to_string(),
            preferred_audio: "both".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimeGGConfig {
    pub base_url: String,
}

impl Default for AnimeGGConfig {
    fn default() -> Self {
        Self {
            base_url: "https://www.animegg.org".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AniDBConfig {
    pub base_url: String,
    pub play_url: String,
    pub public_url: String,
    pub web_url: String,
}

impl Default for AniDBConfig {
    fn default() -> Self {
        Self {
            base_url: "https://anilab2.amdapi.click".to_string(),
            play_url: "https://play.anidb.app".to_string(),
            public_url: "https://anilab.so".to_string(),
            web_url: "https://anidb.se".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KissKhConfig {
    pub base_url: String,
    pub video_key_url: String,
    pub subtitle_key_url: String,
}

impl Default for KissKhConfig {
    fn default() -> Self {
        Self {
            base_url: "https://kisskh.co".to_string(),
            video_key_url: "https://script.google.com/macros/s/AKfycbzn8B31PuDxzaMa9_CQ0VGEDasFqfzI5bXvjaIZH4DM8DNq9q6xj1ALvZNz_JT3jF0suA/exec?id=".to_string(),
            subtitle_key_url: "https://script.google.com/macros/s/AKfycbyq6hTj0ZhlinYC6xbggtgo166tp6XaDKBCGtnYk8uOfYBUFwwxBui0sGXiu_zIFmA/exec?id=".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimeParadiseConfig {
    pub base_url: String,
    pub api_url: String,
    pub stream_url: String,
}

impl Default for AnimeParadiseConfig {
    fn default() -> Self {
        Self {
            base_url: "https://www.animeparadise.moe".to_string(),
            api_url: "https://api.animeparadise.moe".to_string(),
            stream_url: "https://stream.animeparadise.moe".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimeTokiConfig {
    pub base_url: String,
    pub cloud_url: String,
    pub drive_url: String,
}

impl Default for AnimeTokiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://animetoki.com".to_string(),
            cloud_url: "https://cloud.animetoki.com".to_string(),
            drive_url: "https://drive.animetoki.com".to_string(),
        }
    }
}

impl Config {
    /// Loads configuration in this order: built-in defaults, the first
    /// existing config file, then environment overrides.
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        if let Some(path) = Self::find_config_file() {
            let text = std::fs::read_to_string(&path).map_err(|error| {
                Error::config(format!("could not read {}: {error}", path.display()))
            })?;
            let overlay: toml::Value = toml::from_str(&text).map_err(|error| {
                Error::config(format!("invalid TOML in {}: {error}", path.display()))
            })?;
            let mut base = toml::Value::try_from(&config)
                .map_err(|error| Error::config(format!("could not serialize defaults: {error}")))?;
            merge_toml(&mut base, overlay);
            let merged = toml::to_string(&base)
                .map_err(|error| Error::config(format!("could not serialize config: {error}")))?;
            config = toml::from_str(&merged)
                .map_err(|error| Error::config(format!("invalid merged config: {error}")))?;
        }

        config.apply_env();
        config.validate()?;
        Ok(config)
    }

    pub fn find_config_file() -> Option<PathBuf> {
        if let Ok(value) = std::env::var("AP0DL_CONFIG") {
            let path = PathBuf::from(value);
            if path.exists() {
                return Some(path);
            }
        }

        let candidates = [
            PathBuf::from("ap0dl.toml"),
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ap0dl")
                .join("config.toml"),
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".ap0dl")
                .join("config.toml"),
        ];
        candidates.into_iter().find(|path| path.exists())
    }

    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ap0dl")
            .join("config.toml")
    }

    fn apply_env(&mut self) {
        if let Ok(value) = std::env::var("AP0DL_DEFAULT_PROVIDER") {
            self.default_provider = value;
        }
        if let Ok(value) = std::env::var("AP0DL_QUALITY") {
            self.quality = value;
        }
        if let Ok(value) = std::env::var("AP0DL_DOWNLOAD_DIR") {
            self.download_dir = PathBuf::from(value);
        }
        if let Ok(value) = std::env::var("AP0DL_PLAYER") {
            self.player = value;
        }
        if let Ok(value) = std::env::var("AP0DL_FFMPEG") {
            self.ffmpeg = value;
        }
        if let Some(value) = std::env::var("AP0DL_REQUEST_DELAY_MS")
            .ok()
            .and_then(|value| value.parse().ok())
        {
            self.request_delay_ms = value;
        }
        if let Ok(value) = std::env::var("AP0DL_REANIME_BASE") {
            self.providers.reanime.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIMEGG_BASE") {
            self.providers.animegg.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIDB_BASE") {
            self.providers.anidb.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIDB_PLAY") {
            self.providers.anidb.play_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIDB_WEB") {
            self.providers.anidb.web_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_KISSKH_BASE") {
            self.providers.kisskh.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIMEPARADISE_BASE") {
            self.providers.animeparadise.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIMEPARADISE_API") {
            self.providers.animeparadise.api_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIMEPARADISE_STREAM") {
            self.providers.animeparadise.stream_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIMETOKI_BASE") {
            self.providers.animetoki.base_url = value;
        }
        if let Ok(value) = std::env::var("AP0DL_ANIMETOKI_CLOUD") {
            self.providers.animetoki.cloud_url = value;
        }
    }

    fn validate(&self) -> Result<()> {
        if self.default_provider.trim().is_empty() {
            return Err(Error::config("default_provider must not be empty"));
        }
        if self.quality.trim().is_empty() {
            return Err(Error::config("quality must not be empty"));
        }
        for (name, url) in [
            (
                "providers.reanime.base_url",
                &self.providers.reanime.base_url,
            ),
            (
                "providers.animegg.base_url",
                &self.providers.animegg.base_url,
            ),
            ("providers.anidb.base_url", &self.providers.anidb.base_url),
            ("providers.anidb.play_url", &self.providers.anidb.play_url),
            ("providers.anidb.web_url", &self.providers.anidb.web_url),
            ("providers.kisskh.base_url", &self.providers.kisskh.base_url),
            (
                "providers.animeparadise.base_url",
                &self.providers.animeparadise.base_url,
            ),
            (
                "providers.animeparadise.api_url",
                &self.providers.animeparadise.api_url,
            ),
            (
                "providers.animeparadise.stream_url",
                &self.providers.animeparadise.stream_url,
            ),
            (
                "providers.animetoki.base_url",
                &self.providers.animetoki.base_url,
            ),
            (
                "providers.animetoki.cloud_url",
                &self.providers.animetoki.cloud_url,
            ),
        ] {
            url::Url::parse(url)
                .map_err(|error| Error::config(format!("{name} is not a valid URL: {error}")))?;
        }
        Ok(())
    }

    pub fn sample() -> &'static str {
        r#"# ap0dl configuration (Rust engine)
default_provider = "reanime"
quality = "best"
download_dir = "."
player = "mpv"
player_args = []
ffmpeg = "ffmpeg"
merge_subtitles = true
threaded_download = false
auto_retry = 300
request_delay_ms = 250
timeout_secs = 30
use_aniskip = false

[providers.reanime]
base_url = "https://reanime.to"
enc_dec_url = "https://enc-dec.app"
preferred_audio = "both"

[providers.animegg]
base_url = "https://www.animegg.org"

[providers.anidb]
base_url = "https://anilab2.amdapi.click"
play_url = "https://play.anidb.app"
public_url = "https://anilab.so"
web_url = "https://anidb.se"

[providers.kisskh]
base_url = "https://kisskh.co"
video_key_url = "https://script.google.com/macros/s/AKfycbzn8B31PuDxzaMa9_CQ0VGEDasFqfzI5bXvjaIZH4DM8DNq9q6xj1ALvZNz_JT3jF0suA/exec?id="
subtitle_key_url = "https://script.google.com/macros/s/AKfycbyq6hTj0ZhlinYC6xbggtgo166tp6XaDKBCGtnYk8uOfYBUFwwxBui0sGXiu_zIFmA/exec?id="

[providers.animeparadise]
base_url = "https://www.animeparadise.moe"
api_url = "https://api.animeparadise.moe"
stream_url = "https://stream.animeparadise.moe"

[providers.animetoki]
base_url = "https://animetoki.com"
cloud_url = "https://cloud.animetoki.com"
drive_url = "https://drive.animetoki.com"
"#
    }
}

fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => merge_toml(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let config = Config::default();
        config.validate().unwrap();
    }

    #[test]
    fn merge_keeps_defaults() {
        let mut base = toml::Value::try_from(Config::default()).unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            default_provider = "animegg"
            [providers.reanime]
            base_url = "https://example.test"
            "#,
        )
        .unwrap();
        merge_toml(&mut base, overlay);
        let config: Config = toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
        assert_eq!(config.default_provider, "animegg");
        assert_eq!(config.providers.reanime.base_url, "https://example.test");
        assert_eq!(config.providers.animegg.base_url, "https://www.animegg.org");
        assert_eq!(
            config.providers.anidb.base_url,
            "https://anilab2.amdapi.click"
        );
        assert_eq!(config.providers.kisskh.base_url, "https://kisskh.co");
    }
}
