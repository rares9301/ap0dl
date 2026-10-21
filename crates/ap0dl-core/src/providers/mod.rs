pub mod anidb;
pub mod animegg;
pub mod kisskh;
pub mod reanime;

use std::sync::{Arc, OnceLock};

use regex::Regex;
use url::Url;

use crate::config::Config;
use crate::error::Result;
use crate::provider::Provider;

/// Builds every compiled-in provider in display order.
pub fn all(config: &Config) -> Result<Vec<Arc<dyn Provider>>> {
    Ok(vec![
        Arc::new(reanime::ReAnime::new(&config.providers.reanime)?),
        Arc::new(animegg::AnimeGG::new(&config.providers.animegg)?),
        Arc::new(anidb::AniDB::new(&config.providers.anidb)?),
        Arc::new(kisskh::KissKH::new(&config.providers.kisskh)?),
    ])
}

/// Parses `#EXT-X-STREAM-INF` entries from a master playlist. Providers use
/// this to expose every resolution instead of hiding everything behind one
/// adaptive URL, which makes 1080p selection explicit.
pub fn parse_hls_variants(text: &str, base: &str) -> Vec<(String, Option<u32>)> {
    static RESOLUTION: OnceLock<Regex> = OnceLock::new();
    let resolution = RESOLUTION.get_or_init(|| Regex::new(r"(?i)RESOLUTION=\d+x(\d+)").unwrap());

    let mut variants = Vec::new();
    let mut pending: Option<u32> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with("#EXT-X-STREAM-INF") {
            pending = resolution
                .captures(line)
                .and_then(|captures| captures.get(1)?.as_str().parse().ok());
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if pending.is_some() {
            variants.push((resolve_url(base, line), pending.take()));
        }
    }
    variants
}

/// Best-effort quality label from URLs that embed 1080/720/480/360.
pub fn infer_quality(url: &str) -> Option<u32> {
    [1080u32, 720, 480, 360, 240]
        .into_iter()
        .find(|quality| url.contains(&quality.to_string()))
}

fn resolve_url(base: &str, value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        return value.to_string();
    }
    if value.starts_with("//") {
        return format!("https:{value}");
    }
    Url::parse(base)
        .and_then(|base| base.join(value))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| value.to_string())
}
