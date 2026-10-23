//! `ap0dl-manga` — manga plugin for ap0dl.
//!
//! The crate follows the Aniyomi extension model and stays independent of
//! the anime engine, reusing only its shared pieces (HTTP client, config
//! file discovery, fuzzy ranking, range syntax, player detection):
//!
//! - [`source`] defines the async [`MangaSource`] trait (Aniyomi's
//!   `CatalogueSource`/`HttpSource`), and [`filters`] its declared search
//!   filters;
//! - [`sources`] contains the concrete site adapters and their registry;
//! - [`engine`] searches every source concurrently, ranks results locally
//!   and falls back to another source when pages fail;
//! - [`selection`] parses chapter ranges;
//! - [`download`] and [`cbz`] write chapters as CBZ (with `ComicInfo.xml`)
//!   or image folders;
//! - [`reader`] opens chapters in mpv or another image viewer, and
//!   [`viewer`] (Unix) runs the continuous two-page mpv reader with
//!   [`spread`] layouts;
//! - [`state`] saves the chapter and page last read for every series;
//! - [`library`] follows series, reports new chapters and remembers progress;
//! - [`cli`] (feature `cli`) is the `ap0dl manga` subcommand the binary
//!   mounts behind its `manga` cargo feature.

pub mod cbz;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub mod download;
pub mod engine;
pub mod filters;
pub mod library;
pub mod models;
pub mod reader;
pub mod selection;
pub mod source;
pub mod sources;
pub mod spread;
pub mod state;
#[cfg(unix)]
pub mod viewer;

pub use config::{ChapterFormat, MangaConfig};
pub use engine::{MangaEngine, ResolvedManga};
pub use filters::{Filter, FilterValues};
pub use library::{Library, LibraryEntry};
pub use models::{Chapter, Manga, MangaStatus, MangasPage, Page};
pub use selection::ChapterSelection;
pub use source::{MangaSource, SourceInfo, SourceStatus};
pub use spread::{PageLayout, ReadingDirection};
pub use state::{ReadingProgress, ReadingState};
