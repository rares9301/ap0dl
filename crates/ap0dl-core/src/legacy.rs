//! Legacy providers retained for users migrating from the Python engine.
//!
//! These adapters are intentionally not registered with [`crate::Engine`].
//! A provider only moves into the working registry after a live search ->
//! details -> episodes -> stream smoke test succeeds. The observations below
//! were recorded on 2026-10-21 from this sandbox.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LegacyProviderInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub status: &'static str,
    pub observed: &'static str,
    pub evidence: &'static str,
}

const OBSERVED: &str = "2026-10-21";

pub const LEGACY_PROVIDERS: &[LegacyProviderInfo] = &[
    LegacyProviderInfo {
        id: "animepahe",
        name: "AnimePahe",
        status: "deprecated",
        observed: OBSERVED,
        evidence: "GET https://animepahe.pw/ -> HTTP 403, 5,102 bytes, title \"Just a moment...\" (Cloudflare). GET https://animepahe.su/ -> HTTP 200, 6,990-byte challenge shell with no usable API JSON.",
    },
    LegacyProviderInfo {
        id: "gogoanime",
        name: "Gogoanime / Anitaku",
        status: "deprecated",
        observed: OBSERVED,
        evidence: "GET https://gogoanime.bid/ -> redirect to http://ww547.gogoanime.bid/?tkn=... then HTTP 401, 9 bytes. GET https://anitaku.pe/search.html?keyword=Bocchi -> HTTP 200 but a 27,666-byte parked \"abovedomains\" for-sale page, not results.",
    },
    LegacyProviderInfo {
        id: "allanime",
        name: "AllAnime",
        status: "deprecated",
        observed: OBSERVED,
        evidence: "GET https://api.allanime.day/api with a shows search query -> HTTP 403, 7,231-byte Cloudflare \"Just a moment...\" page. The previous source query had already returned AA_CRYPTO_MISSING.",
    },
    LegacyProviderInfo {
        id: "aniwave",
        name: "AniWave / 9anime",
        status: "deprecated",
        observed: OBSERVED,
        evidence: "GET https://aniwave.to/ -> HTTP 200 after redirect to https://ww38.aniwave.to/, 44,059 bytes, title \"aniwave.to\", no watch/search links (parked domain).",
    },
];

pub fn all() -> &'static [LegacyProviderInfo] {
    LEGACY_PROVIDERS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_legacy_provider_has_evidence() {
        assert_eq!(all().len(), 4);
        for provider in all() {
            assert_eq!(provider.status, "deprecated");
            assert!(!provider.observed.is_empty());
            assert!(!provider.evidence.is_empty());
        }
    }
}
