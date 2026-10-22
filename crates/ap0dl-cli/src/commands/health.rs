use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ap0dl_core::{Provider, SearchResult};

const PROVIDER_FAILURE_TTL: u64 = 600;

pub type Hit = (Arc<dyn Provider>, SearchResult);

/// Keep recently failing providers in the results, but below healthy ones.
pub fn sort_hits_by_health(hits: &mut [Hit]) {
    hits.sort_by_key(|(provider, _)| provider_is_degraded(provider.id()));
}

pub fn provider_is_degraded(provider: &str) -> bool {
    let Ok(value) = std::fs::read_to_string(health_marker(provider)) else {
        return false;
    };
    let Ok(failed_at) = value.trim().parse::<u64>() else {
        return false;
    };
    now_secs().saturating_sub(failed_at) < PROVIDER_FAILURE_TTL
}

pub fn mark_provider_degraded(provider: &str) {
    let marker = health_marker(provider);
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker, now_secs().to_string());
}

pub fn mark_provider_healthy(provider: &str) {
    let _ = std::fs::remove_file(health_marker(provider));
}

fn health_marker(provider: &str) -> PathBuf {
    std::env::temp_dir()
        .join("ap0dl-provider-health")
        .join(provider)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
