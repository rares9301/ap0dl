//! `ap0dl manga ...` — the plugin's command-line surface.
//!
//! The host binary mounts [`MangaArgs`] as one subcommand and forwards it to
//! [`run`]; everything else (pickers, reader loop, downloads, library) lives
//! here, so the plugin can be enabled or removed with a cargo feature.

mod browse;
mod download;
mod interactive;
mod library;
mod read;

use std::path::PathBuf;

use anyhow::Result;
use ap0dl_core::Config;
use clap::{Args, Subcommand};

use crate::config::{ChapterFormat, MangaConfig};
use crate::engine::MangaEngine;
use crate::spread::{PageLayout, ReadingDirection};

#[derive(Debug, Args)]
pub struct MangaArgs {
    #[command(subcommand)]
    pub command: MangaCommand,
}

#[derive(Debug, Subcommand)]
pub enum MangaCommand {
    /// List compiled-in manga sources, or one source's search filters.
    Sources {
        #[arg(long)]
        json: bool,
        /// Show the search filters a source accepts.
        #[arg(long, value_name = "SOURCE")]
        filters: Option<String>,
    },

    /// Search every source (or one) for a manga.
    Search {
        query: String,
        #[arg(long)]
        source: Option<String>,
        /// Source filter as key=value; repeatable. Requires --source.
        #[arg(long = "filter", value_name = "KEY=VALUE")]
        filters: Vec<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },

    /// Browse a source's popular titles.
    Popular {
        /// Defaults to manga.default_source.
        #[arg(long)]
        source: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long)]
        json: bool,
    },

    /// Browse a source's latest updates.
    Latest {
        #[arg(long)]
        source: Option<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long)]
        json: bool,
    },

    /// Show details and the chapter list for a query or source URL.
    Chapters {
        query: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long, default_value_t = 1)]
        index: usize,
        #[arg(long)]
        json: bool,
    },

    /// Print resolved page-image JSON for external tools.
    Pages {
        query: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long, default_value_t = 1)]
        index: usize,
        /// Chapter range (for example 1, 1-3, latest).
        #[arg(short = 'r', long, default_value = "1")]
        range: String,
        /// Prefer this scanlation group when a chapter has several uploads.
        #[arg(long)]
        scanlator: Option<String>,
    },

    /// Read chapters in mpv (or the configured image viewer).
    ///
    /// With no --index/--range, opens the fuzzy manga picker and asks which
    /// chapter to read; Enter resumes the saved chapter and page. mpv shows
    /// a continuous two-page viewer that runs on into the next chapters.
    Read {
        /// Manga title. Omit to be prompted interactively.
        query: Option<String>,
        #[arg(long)]
        source: Option<String>,
        /// Skip the manga picker and choose this one-based result.
        #[arg(long)]
        index: Option<usize>,
        /// Chapter range (for example 1, 1-3, latest). Omit for a chapter picker.
        #[arg(short = 'r', long)]
        range: Option<String>,
        #[arg(long)]
        reader: Option<String>,
        #[arg(long = "reader-arg", value_name = "ARG")]
        reader_args: Vec<String>,
        #[arg(long)]
        scanlator: Option<String>,
        /// One page at a time in the mpv viewer.
        #[arg(long, conflicts_with = "double")]
        single: bool,
        /// Two-page spreads in the mpv viewer (needs ffmpeg).
        #[arg(long)]
        double: bool,
        /// Left-to-right reading (comics): Right arrow turns forward.
        #[arg(long, conflicts_with = "rtl")]
        ltr: bool,
        /// Right-to-left reading (manga): Left arrow turns forward.
        #[arg(long)]
        rtl: bool,
        /// Print page image URLs instead of opening the reader.
        #[arg(long)]
        print_url: bool,
        /// Disable the interactive pickers.
        #[arg(long)]
        no_interactive: bool,
    },

    /// Download chapters as CBZ (with ComicInfo.xml) or image folders.
    ///
    /// With no --index/--range, opens the fuzzy manga picker and a checkbox
    /// chapter list with a download-all row.
    Download {
        query: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        index: Option<usize>,
        #[arg(short = 'r', long)]
        range: Option<String>,
        #[arg(short = 'd', long)]
        dir: Option<PathBuf>,
        /// cbz or folder. Defaults to manga.format.
        #[arg(long)]
        format: Option<ChapterFormat>,
        #[arg(long)]
        scanlator: Option<String>,
        #[arg(long)]
        json: bool,
        /// Print page image URLs instead of downloading.
        #[arg(long)]
        print_url: bool,
        #[arg(long)]
        no_interactive: bool,
    },

    /// Follow series and check them for new chapters.
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },

    /// Print the manga config sample or the effective settings.
    Config {
        #[arg(long)]
        sample: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum LibraryCommand {
    /// List followed series and reading progress.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Follow a series (records its current chapters as seen).
    Add {
        query: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        index: Option<usize>,
        #[arg(long)]
        no_interactive: bool,
    },
    /// Stop following a series (by title, id, source:id or URL).
    Remove { query: String },
    /// Fetch every followed series and report new chapters.
    Update {
        #[arg(long)]
        json: bool,
        /// Also download the new chapters.
        #[arg(long)]
        download: bool,
    },
}

/// Entry point the host binary calls for `ap0dl manga ...`.
pub async fn run(config: Config, args: MangaArgs) -> Result<()> {
    let manga = MangaConfig::load().map_err(to_anyhow)?;
    if let MangaCommand::Config { sample } = &args.command {
        if *sample {
            print!("{}", MangaConfig::sample());
        } else {
            println!("{}", toml::to_string_pretty(&manga)?);
        }
        return Ok(());
    }
    let engine = MangaEngine::new(config, manga).map_err(to_anyhow)?;

    match args.command {
        MangaCommand::Sources { json, filters } => {
            browse::sources(&engine, json, filters.as_deref())
        }
        MangaCommand::Search {
            query,
            source,
            filters,
            page,
            limit,
            json,
        } => {
            browse::search(
                &engine,
                &query,
                source.as_deref(),
                &filters,
                page,
                limit,
                json,
            )
            .await
        }
        MangaCommand::Popular { source, page, json } => {
            browse::browse(&engine, source.as_deref(), page, json, false).await
        }
        MangaCommand::Latest { source, page, json } => {
            browse::browse(&engine, source.as_deref(), page, json, true).await
        }
        MangaCommand::Chapters {
            query,
            source,
            index,
            json,
        } => browse::chapters(&engine, &query, source.as_deref(), index, json).await,
        MangaCommand::Pages {
            query,
            source,
            index,
            range,
            scanlator,
        } => {
            browse::pages(
                &engine,
                &query,
                source.as_deref(),
                index,
                &range,
                scanlator.as_deref(),
            )
            .await
        }
        MangaCommand::Read {
            query,
            source,
            index,
            range,
            reader,
            reader_args,
            scanlator,
            single,
            double,
            ltr,
            rtl,
            print_url,
            no_interactive,
        } => {
            read::run(
                &engine,
                read::ReadOptions {
                    query,
                    source,
                    index,
                    range,
                    reader,
                    reader_args,
                    scanlator,
                    layout: (single || double).then_some(if single {
                        PageLayout::Single
                    } else {
                        PageLayout::Double
                    }),
                    direction: (ltr || rtl).then_some(if ltr {
                        ReadingDirection::Ltr
                    } else {
                        ReadingDirection::Rtl
                    }),
                    print_url,
                    no_interactive,
                },
            )
            .await
        }
        MangaCommand::Download {
            query,
            source,
            index,
            range,
            dir,
            format,
            scanlator,
            json,
            print_url,
            no_interactive,
        } => {
            download::run(
                &engine,
                download::DownloadArgs {
                    query,
                    source,
                    index,
                    range,
                    dir,
                    format,
                    scanlator,
                    json,
                    print_url,
                    no_interactive,
                },
            )
            .await
        }
        MangaCommand::Library { command } => library::run(&engine, command).await,
        MangaCommand::Config { .. } => unreachable!("handled before the engine is built"),
    }
}

pub(crate) fn to_anyhow(error: ap0dl_core::Error) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}
