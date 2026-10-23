pub mod hls;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::error::{Error, Result};
use crate::http::Http;
use crate::models::{Stream, StreamKind};
use crate::proxy::FlixProxy;
use crate::util;

/// One progress tick from a direct or HLS download.
#[derive(Debug, Clone, Copy, Default)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub segments_done: Option<u64>,
    pub segments_total: Option<u64>,
}

/// Cheap, thread-safe progress hook used by the Android bridge. The desktop
/// CLI keeps using its terminal progress bars.
#[derive(Clone)]
pub struct ProgressCallback(Arc<dyn Fn(DownloadProgress) + Send + Sync>);

impl ProgressCallback {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(DownloadProgress) + Send + Sync + 'static,
    {
        Self(Arc::new(callback))
    }

    pub fn emit(&self, progress: DownloadProgress) {
        (self.0)(progress);
    }
}

impl std::fmt::Debug for ProgressCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProgressCallback(..)")
    }
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub ffmpeg: String,
    pub merge_video: bool,
    pub merge_subtitles: bool,
    pub xor_mask: Vec<u8>,
    pub progress: Option<ProgressCallback>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".to_string(),
            merge_video: true,
            merge_subtitles: true,
            xor_mask: Vec::new(),
            progress: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Downloaded {
    pub path: PathBuf,
    pub subtitles: Vec<PathBuf>,
}

/// Downloads a resolved stream. For flixcloud HLS a local proxy is started
/// automatically; the proxy is kept alive until the download has finished.
pub async fn download_stream(
    http: &Http,
    stream: &Stream,
    destination: &Path,
    options: &DownloadOptions,
) -> Result<Downloaded> {
    let mut headers = header_map(&stream.headers);
    let _proxy = if let Some(info) = &stream.flix {
        Some(FlixProxy::start(info.clone()).await?)
    } else {
        None
    };
    let url = _proxy
        .as_ref()
        .map(|proxy| proxy.url())
        .unwrap_or_else(|| stream.url.clone());

    if stream.kind == StreamKind::Hls || url.contains(".m3u8") || url.contains("parse-flixcloud") {
        let mut hls_options = hls::HlsOptions {
            ffmpeg: options.ffmpeg.clone(),
            merge: options.merge_video,
            xor_mask: options.xor_mask.clone(),
            progress: options.progress.clone(),
        };
        if hls_options.xor_mask.is_empty() {
            if let Some(info) = &stream.flix {
                hls_options.xor_mask = info.xor_mask.clone();
            }
        }
        let mut path = hls::download_hls(http, &url, &headers, destination, &hls_options).await?;
        let subtitles = if options.merge_subtitles {
            download_subtitles(http, stream, destination, options).await?
        } else {
            Vec::new()
        };
        if options.merge_subtitles && !subtitles.is_empty() {
            if let Ok(Some(merged)) = merge_subtitles(&options.ffmpeg, &path, &subtitles).await {
                path = merged;
            }
        }
        return Ok(Downloaded { path, subtitles });
    }

    let mut path = download_direct(
        http,
        &url,
        &mut headers,
        destination,
        stream.container.as_deref(),
        options.progress.as_ref(),
    )
    .await?;
    let subtitles = if options.merge_subtitles {
        download_subtitles(http, stream, destination, options).await?
    } else {
        Vec::new()
    };
    if options.merge_subtitles && !subtitles.is_empty() {
        if let Ok(Some(merged)) = merge_subtitles(&options.ffmpeg, &path, &subtitles).await {
            path = merged;
        }
    }
    Ok(Downloaded { path, subtitles })
}

async fn download_direct(
    http: &Http,
    url: &str,
    headers: &mut HeaderMap,
    destination: &Path,
    container_hint: Option<&str>,
    progress_callback: Option<&ProgressCallback>,
) -> Result<PathBuf> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let extension = direct_extension(url, container_hint);
    let path = destination.with_extension(&extension);
    let mut written = tokio::fs::metadata(&path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let label = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let progress = progress_bar(None, label);
    progress.set_position(written);
    if let Some(callback) = progress_callback {
        callback.emit(DownloadProgress {
            downloaded_bytes: written,
            total_bytes: None,
            ..DownloadProgress::default()
        });
    }
    let mut last_error: Option<Error> = None;

    // Large direct files are frequently interrupted by CDN resets. Keep the
    // partial file and resume with a Range request instead of restarting.
    for attempt in 0..5 {
        let mut request_headers = headers.clone();
        if written > 0 {
            request_headers.insert(
                reqwest::header::RANGE,
                HeaderValue::from_str(&format!("bytes={written}-"))
                    .map_err(|error| Error::download(format!("invalid range header: {error}")))?,
            );
        }

        let response = match http.get(url, &request_headers).await {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(error);
                if attempt < 4 {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt + 1))).await;
                    continue;
                }
                break;
            }
        };

        let append = written > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if !append {
            written = 0;
        }
        let total = response.content_length().map(|length| length + written);
        if let Some(total) = total {
            progress.set_length(total);
            if let Some(callback) = progress_callback {
                callback.emit(DownloadProgress {
                    downloaded_bytes: written,
                    total_bytes: Some(total),
                    ..DownloadProgress::default()
                });
            }
        }

        let mut file = if append {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await?
        } else {
            tokio::fs::File::create(&path).await?
        };
        let mut stream = response.bytes_stream();
        let mut interrupted = false;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(chunk) => {
                    tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
                    written += chunk.len() as u64;
                    progress.set_position(written);
                    if let Some(callback) = progress_callback {
                        callback.emit(DownloadProgress {
                            downloaded_bytes: written,
                            total_bytes: total,
                            ..DownloadProgress::default()
                        });
                    }
                }
                Err(source) => {
                    last_error = Some(Error::Http {
                        url: url.to_string(),
                        source,
                    });
                    interrupted = true;
                    break;
                }
            }
        }

        if !interrupted {
            progress.finish_and_clear();
            return Ok(path);
        }
        if attempt < 4 {
            tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt + 1))).await;
        }
    }

    progress.finish_and_clear();
    Err(last_error.unwrap_or_else(|| Error::download("direct download was interrupted")))
}

/// Picks the output extension from the provider hint, then the URL, and
/// finally falls back to MP4. This keeps AnimeToki MKV downloads as `.mkv`.
fn direct_extension(url: &str, hint: Option<&str>) -> String {
    if let Some(hint) = hint {
        let hint = hint.trim().trim_start_matches('.').to_ascii_lowercase();
        if matches!(
            hint.as_str(),
            "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v"
        ) {
            return hint;
        }
    }
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let extension = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v"
    ) {
        extension
    } else {
        "mp4".to_string()
    }
}

async fn download_subtitles(
    http: &Http,
    stream: &Stream,
    destination: &Path,
    _options: &DownloadOptions,
) -> Result<Vec<PathBuf>> {
    if stream.subtitles.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut paths = Vec::new();
    for (index, subtitle) in stream.subtitles.iter().enumerate() {
        let extension = subtitle
            .format
            .as_deref()
            .filter(|format| !format.is_empty())
            .unwrap_or_else(|| {
                subtitle
                    .url
                    .split(['?', '#'])
                    .next()
                    .and_then(|value| value.rsplit('.').next())
                    .unwrap_or("srt")
            });
        let path = destination.with_extension(format!("sub{index}.{extension}"));
        let bytes = http
            .get_bytes(&subtitle.url, &header_map(&stream.headers))
            .await?;
        tokio::fs::write(&path, bytes).await?;
        paths.push(path);
    }
    Ok(paths)
}

async fn merge_subtitles(
    ffmpeg: &str,
    video: &Path,
    subtitles: &[PathBuf],
) -> Result<Option<PathBuf>> {
    if subtitles.is_empty() || which::which(ffmpeg).is_err() {
        return Ok(None);
    }
    let output = video.with_extension("submerged.mkv");
    let mut command = tokio::process::Command::new(ffmpeg);
    command.arg("-y").arg("-i").arg(video);
    for subtitle in subtitles {
        command.arg("-i").arg(subtitle);
    }
    command.arg("-map").arg("0").arg("-c").arg("copy");
    for index in 0..subtitles.len() {
        command.arg("-map").arg(format!("{}:0", index + 1));
    }
    command.arg("-c:s").arg("srt").arg(&output);
    let status = command
        .status()
        .await
        .map_err(|error| Error::download(format!("could not run ffmpeg: {error}")))?;
    if !status.success() {
        let _ = tokio::fs::remove_file(&output).await;
        return Ok(None);
    }
    let merged = video.with_extension("mkv");
    tokio::fs::rename(&output, &merged).await?;
    if video.extension().and_then(|extension| extension.to_str()) != Some("mkv") {
        let _ = tokio::fs::remove_file(video).await;
    }
    Ok(Some(merged))
}

pub fn header_map(headers: &BTreeMap<String, String>) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            map.insert(name, value);
        }
    }
    map
}

fn progress_bar(total: Option<u64>, label: &str) -> ProgressBar {
    let bar = match total {
        Some(total) => ProgressBar::new(total),
        None => ProgressBar::new_spinner(),
    };
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {msg:22} [{bar:32.cyan/blue}] {bytes}/{total_bytes} {bytes_per_sec} ({eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=>-"),
    );
    bar.set_message(label.to_string());
    bar.enable_steady_tick(std::time::Duration::from_millis(200));
    bar
}

pub async fn download_episode(
    http: &Http,
    stream: &Stream,
    title: &str,
    episode: f32,
    directory: &Path,
    options: &DownloadOptions,
) -> Result<Downloaded> {
    let destination = util::file_stem_for(directory, title, episode);
    download_stream(http, stream, &destination, options).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_container_extension() {
        assert_eq!(
            direct_extension("https://example.test/file.mkv?token=1", None),
            "mkv"
        );
        assert_eq!(
            direct_extension("https://example.test/?a=download&id=1", Some("mkv")),
            "mkv"
        );
        assert_eq!(
            direct_extension("https://example.test/?a=download&id=1", None),
            "mp4"
        );
    }
}
