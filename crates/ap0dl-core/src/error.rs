use std::fmt;

/// Errors produced by the engine. Providers add their own context through
/// [`Error::Provider`] so the CLI can always name the failing source.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP request to {url} failed: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("HTTP {status} from {url}")]
    HttpStatus { status: u16, url: String },

    #[error("invalid URL {0:?}: {1}")]
    Url(String, url::ParseError),

    #[error("could not decode JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("could not parse HTML from {0}: {1}")]
    Html(String, String),

    #[error("could not parse playlist from {0}: {1}")]
    Playlist(String, String),

    #[error("provider {provider} failed: {message}")]
    Provider {
        provider: &'static str,
        message: String,
    },

    #[error("no results for {0:?}")]
    NoResults(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("extraction failed: {0}")]
    Extractor(String),

    #[error("download failed: {0}")]
    Download(String),

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("player failed: {0}")]
    Player(String),

    #[error("{0}")]
    Unavailable(String),
}

impl Error {
    pub fn provider(provider: &'static str, message: impl fmt::Display) -> Self {
        Self::Provider {
            provider,
            message: message.to_string(),
        }
    }

    pub fn config(message: impl fmt::Display) -> Self {
        Self::Config(message.to_string())
    }

    pub fn unsupported(message: impl fmt::Display) -> Self {
        Self::Unsupported(message.to_string())
    }

    pub fn extraction(message: impl fmt::Display) -> Self {
        Self::Extractor(message.to_string())
    }

    pub fn download(message: impl fmt::Display) -> Self {
        Self::Download(message.to_string())
    }

    /// The source is temporarily unavailable for a reason the user can act on
    /// (for example a per-file download quota). The message is shown verbatim.
    pub fn unavailable(message: impl fmt::Display) -> Self {
        Self::Unavailable(message.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
