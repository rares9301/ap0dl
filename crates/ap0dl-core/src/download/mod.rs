pub mod hls;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::error::{Error, Result};
use crate::http::Http;
use crate::models::{Stream, StreamKind};
use crate::proxy::FlixProxy;
use crate::util;

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub ffmpeg: String,
    pub merge_video: bool,
    pub merge_subtitles: bool,
    pub xor_mask: Vec<u8>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".to_string(),
            merge_video: true,
            merge_subtitles: true,
            xor_mask: Vec::new(),
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
        };
        if hls_options.xor_mask.is_empty() {
            if let Some(info) = &stream.flix {
                hls_options.xor_mask = info.xor_mask.clone();
            }
        }
        let path = hls::download_hls(http, &url, &headers, destination, &hls_options).await?;
        let subtitles = if options.merge_subtitles {
            download_subtitles(http, stream, destination, options).await?
        } else {
            Vec::new()
        };
        if options.merge_subtitles && !subtitles.is_empty() {
            let _ = merge_subtitles(&options.ffmpeg, &path, &subtitles).await;
        }
        return Ok(Downloaded { path, subtitles });
    }

    let path = download_direct(http, &url, &mut headers, destination).await?;
    let subtitles = if options.merge_subtitles {
        download_subtitles(http, stream, destination, options).await?
    } else {
        Vec::new()
    };
    Ok(Downloaded { path, subtitles })
}

async fn download_direct(
    http: &Http,
    url: &str,
    headers: &mut HeaderMap,
    destination: &Path,
) -> Result<PathBuf> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let path = destination.with_extension("mp4");
    let mut written = tokio::fs::metadata(&path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let progress = progress_bar(None, "download");
    progress.set_position(written);
    let mut last_error: Option<Error> = None;

    // Large direct MP4s are frequently interrupted by CDN resets. Keep the
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

async fn merge_subtitles(ffmpeg: &str, video: &Path, subtitles: &[PathBuf]) -> Result<()> {
    if subtitles.is_empty() || which::which(ffmpeg).is_err() {
        return Ok(());
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
    if status.success() {
        let _ = tokio::fs::rename(&output, video.with_extension("mkv")).await;
    }
    Ok(())
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
            "{spinner:.green} {msg:20} [{bar:32.cyan/blue}] {bytes}/{total_bytes} ({eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=>-"),
    );
    bar.set_message(label.to_string());
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
