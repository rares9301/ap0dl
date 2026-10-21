//! `ap0dl-core` — typed, async engine for ap0dl.
//!
//! The crate is deliberately split into small, testable pieces:
//!
//! - [`config`] loads and merges user configuration;
//! - [`http`] owns the shared HTTP client, retries, rate limiting and cache;
//! - [`models`] contains the wire/domain types used by every provider;
//! - [`provider`] defines the async provider trait and registry;
//! - [`providers`] contains concrete site adapters;
//! - [`quality`] and [`ranges`] implement the two user-facing selectors;
//! - [`download`] implements direct and HLS downloads;
//! - [`proxy`] serves decrypted flixcloud playlists and segments locally.

pub mod aniskip;
pub mod config;
pub mod download;
pub mod engine;
pub mod error;
pub mod extractors;
pub mod http;
pub mod legacy;
pub mod models;
pub mod player;
pub mod provider;
pub mod providers;
pub mod proxy;
pub mod quality;
pub mod ranges;
pub mod util;

pub use config::Config;
pub use engine::{Engine, Resolved};
pub use error::{Error, Result};
pub use http::Http;
pub use models::{
    Anime, Audio, Chapter, Episode, FlixInfo, SearchResult, Stream, StreamKind, Subtitle,
};
pub use provider::{Provider, ProviderInfo, ProviderStatus};
