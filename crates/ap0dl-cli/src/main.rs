use std::path::PathBuf;

use ap0dl_core::Config;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Debug, Parser)]
#[command(
    name = "ap0dl",
    version,
    about = "Typed async anime scraper/downloader (Rust engine)",
    long_about = None
)]
pub struct Cli {
    /// Path to ap0dl.toml. Defaults to ./ap0dl.toml or ~/.config/ap0dl/config.toml.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Log filter (e.g. info, debug, ap0dl_core=trace).
    #[arg(long, global = true, default_value = "warn")]
    pub log: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Search a provider (or the default provider) for an anime.
    Search {
        query: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },

    /// List episodes for a query or direct provider URL.
    Episodes {
        query: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long, default_value_t = 1)]
        index: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        include_fillers: bool,
    },

    /// Print resolved stream JSON for external tools.
    Grab {
        query: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long, default_value_t = 1)]
        index: usize,
        #[arg(short = 'r', long, default_value = ":")]
        range: String,
        #[arg(short = 'q', long)]
        quality: Option<String>,
        #[arg(long)]
        include_fillers: bool,
    },

    /// Resolve a stream and launch a media player.
    ///
    /// With no --index/--range, opens fuzzy search and episode pickers.
    Stream {
        /// Anime title. Omit to be prompted interactively.
        query: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        /// Skip the anime picker and choose this one-based result.
        #[arg(long)]
        index: Option<usize>,
        /// Episode range (for example 1, 1-3, :). Omit for an episode picker.
        #[arg(short = 'r', long)]
        range: Option<String>,
        #[arg(short = 'q', long)]
        quality: Option<String>,
        #[arg(short = 'p', long)]
        player: Option<String>,
        #[arg(long = "player-arg", value_name = "ARG")]
        player_args: Vec<String>,
        #[arg(long)]
        include_fillers: bool,
        /// Print the resolved URL instead of playing it.
        #[arg(long)]
        print_url: bool,
        /// Disable the interactive picker and use the best automatic match.
        #[arg(long)]
        no_interactive: bool,
    },

    /// Download episodes to disk.
    Download {
        query: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long, default_value_t = 1)]
        index: usize,
        #[arg(short = 'r', long, default_value = ":")]
        range: String,
        #[arg(short = 'q', long)]
        quality: Option<String>,
        #[arg(short = 'd', long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        include_fillers: bool,
        #[arg(long)]
        no_merge: bool,
        #[arg(long)]
        json: bool,
    },

    /// List compiled-in providers and their support status.
    Providers {
        #[arg(long)]
        json: bool,
        /// Show legacy providers that were checked and deprecated, with HTTP evidence.
        #[arg(long)]
        legacy: bool,
    },

    /// Show, print or initialize configuration.
    Config {
        #[arg(long)]
        path: bool,
        #[arg(long)]
        sample: bool,
        #[arg(long)]
        init: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(&cli.log);

    if let Some(path) = &cli.config {
        // The config loader reads AP0DL_CONFIG; setting it here keeps the
        // command-line override out of every subcommand.
        // SAFETY: single-threaded before any provider task is spawned.
        unsafe {
            std::env::set_var("AP0DL_CONFIG", path);
        }
    }

    let config = Config::load().map_err(|error| anyhow::anyhow!(error.to_string()))?;

    match cli.command {
        Command::Search {
            query,
            provider,
            limit,
            json,
        } => commands::search::run(config, &query, provider.as_deref(), limit, json).await?,
        Command::Episodes {
            query,
            provider,
            index,
            json,
            include_fillers,
        } => {
            commands::episodes::run(
                config,
                &query,
                provider.as_deref(),
                index,
                json,
                include_fillers,
            )
            .await?
        }
        Command::Grab {
            query,
            provider,
            index,
            range,
            quality,
            include_fillers,
        } => {
            commands::grab::run(
                config,
                &query,
                provider.as_deref(),
                index,
                &range,
                quality.as_deref(),
                include_fillers,
            )
            .await?
        }
        Command::Stream {
            query,
            provider,
            index,
            range,
            quality,
            player,
            player_args,
            include_fillers,
            print_url,
            no_interactive,
        } => {
            commands::stream::run(
                config,
                query.as_deref(),
                provider.as_deref(),
                index,
                range.as_deref(),
                quality.as_deref(),
                player.as_deref(),
                player_args,
                include_fillers,
                print_url,
                no_interactive,
            )
            .await?
        }
        Command::Download {
            query,
            provider,
            index,
            range,
            quality,
            dir,
            include_fillers,
            no_merge,
            json,
        } => {
            commands::download::run(
                config,
                &query,
                provider.as_deref(),
                index,
                &range,
                quality.as_deref(),
                dir,
                include_fillers,
                no_merge,
                json,
            )
            .await?
        }
        Command::Providers { json, legacy } => {
            commands::providers::run(config, json, legacy).await?
        }
        Command::Config { path, sample, init } => {
            commands::config::run(config, path, sample, init).await?
        }
    }

    Ok(())
}

fn init_logging(filter: &str) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}
