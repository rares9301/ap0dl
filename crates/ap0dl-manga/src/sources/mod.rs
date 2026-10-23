//! Concrete manga sites. Each adapter implements [`crate::MangaSource`];
//! [`all`] is the only place a source is registered.

pub mod mangadex;
pub mod mangapill;
pub mod weebcentral;

use std::sync::Arc;

use ap0dl_core::error::Result;

use crate::config::MangaConfig;
use crate::source::MangaSource;

/// Builds every compiled-in source in display order.
pub fn all(config: &MangaConfig) -> Result<Vec<Arc<dyn MangaSource>>> {
    Ok(vec![
        Arc::new(mangadex::MangaDex::new(&config.sources.mangadex)?),
        Arc::new(mangapill::MangaPill::new(&config.sources.mangapill)?),
        Arc::new(weebcentral::WeebCentral::new(&config.sources.weebcentral)?),
    ])
}
