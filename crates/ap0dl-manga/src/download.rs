use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::Bytes;
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;

use ap0dl_core::download::header_map;
use ap0dl_core::error::{Error, Result};
use ap0dl_core::http::Http;
use ap0dl_core::util::sanitize_filename;

use crate::cbz::{CbzWriter, comic_info};
use crate::config::ChapterFormat;
use crate::models::{Chapter, Manga, Page};

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub format: ChapterFormat,
    pub concurrency: usize,
    /// Show an `indicatif` page bar on stderr.
    pub progress: bool,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            format: ChapterFormat::Cbz,
            concurrency: 4,
            progress: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadedChapter {
    pub path: PathBuf,
    pub pages: usize,
    pub bytes: u64,
    /// True when the chapter was already on disk and nothing was fetched.
    pub skipped: bool,
}

/// `<root>/<Series>/Chapter 0012` (+ `.cbz`). Zero-padded numbers keep
/// readers and file managers in reading order.
pub fn chapter_path(
    root: &Path,
    manga: &Manga,
    chapter: &Chapter,
    format: ChapterFormat,
) -> PathBuf {
    let stem = if chapter.number >= 0.0 {
        let whole = chapter.number.trunc() as u32;
        let fraction = chapter.number.fract();
        if fraction == 0.0 {
            format!("Chapter {whole:04}")
        } else {
            let decimals = format!("{fraction:.3}");
            format!(
                "Chapter {whole:04}.{}",
                decimals.trim_start_matches("0.").trim_end_matches('0')
            )
        }
    } else {
        sanitize_filename(&chapter.name)
    };
    let directory = root.join(sanitize_filename(&manga.title));
    match format {
        ChapterFormat::Cbz => directory.join(format!("{stem}.cbz")),
        ChapterFormat::Folder => directory.join(stem),
    }
}

/// Downloads every page of a chapter into a CBZ (with `ComicInfo.xml`) or a
/// folder of numbered images. Pages are fetched concurrently but written in
/// order; the output is assembled under a `.part` name and renamed when
/// complete, so an interrupted run never leaves a truncated chapter that a
/// later run would skip.
pub async fn download_chapter(
    http: &Http,
    manga: &Manga,
    chapter: &Chapter,
    pages: &[Page],
    root: &Path,
    options: &DownloadOptions,
) -> Result<DownloadedChapter> {
    let path = chapter_path(root, manga, chapter, options.format);
    if path.exists() {
        return Ok(DownloadedChapter {
            path,
            pages: pages.len(),
            bytes: 0,
            skipped: true,
        });
    }
    if pages.is_empty() {
        return Err(Error::download(format!("{} has no pages", chapter.label())));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = partial_path(&path);
    if partial.is_dir() {
        std::fs::remove_dir_all(&partial)?;
    } else if partial.exists() {
        std::fs::remove_file(&partial)?;
    }

    let bar = progress_bar(pages.len() as u64, &chapter.label(), options.progress);
    let width = pages.len().to_string().len().max(3);
    let mut fetches = futures::stream::iter(pages.iter().cloned())
        .map(|page| async move {
            let bytes = fetch_image(http, &page).await?;
            Ok::<_, Error>((page, bytes))
        })
        .buffered(options.concurrency.max(1));

    let mut total = 0u64;
    let result: Result<()> = async {
        match options.format {
            ChapterFormat::Cbz => {
                let file = BufWriter::new(File::create(&partial)?);
                let mut archive = CbzWriter::new(file);
                while let Some(fetched) = fetches.next().await {
                    let (page, bytes) = fetched?;
                    let name = page_name(page.index, width, &bytes);
                    archive.add(&name, &bytes)?;
                    total += bytes.len() as u64;
                    bar.inc(1);
                }
                archive.add(
                    "ComicInfo.xml",
                    comic_info(manga, chapter, pages.len()).as_bytes(),
                )?;
                archive.finish()?;
            }
            ChapterFormat::Folder => {
                std::fs::create_dir_all(&partial)?;
                while let Some(fetched) = fetches.next().await {
                    let (page, bytes) = fetched?;
                    std::fs::write(partial.join(page_name(page.index, width, &bytes)), &bytes)?;
                    total += bytes.len() as u64;
                    bar.inc(1);
                }
            }
        }
        Ok(())
    }
    .await;

    if let Err(error) = result {
        bar.abandon_with_message(format!("{} failed", chapter.label()));
        if partial.is_dir() {
            let _ = std::fs::remove_dir_all(&partial);
        } else {
            let _ = std::fs::remove_file(&partial);
        }
        return Err(error);
    }
    std::fs::rename(&partial, &path)?;
    bar.finish_and_clear();
    Ok(DownloadedChapter {
        path,
        pages: pages.len(),
        bytes: total,
        skipped: false,
    })
}

fn partial_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    path.with_file_name(name)
}

/// Fetches one image. Image CDNs are not paced like site APIs, so this goes
/// straight to the shared reqwest client with its own retries, and falls
/// back to [`Http::get_bytes`] (native TLS impersonation) on a challenge.
///
/// When the primary URL fails, each of [`Page::fallback_urls`] is tried in
/// turn (MangaDex lists its origin server behind the @Home cache nodes).
pub async fn fetch_image(http: &Http, page: &Page) -> Result<Bytes> {
    let mut headers = http.headers();
    for (name, value) in header_map(&page.headers) {
        if let Some(name) = name {
            headers.insert(name, value);
        }
    }
    let mut result = fetch_url(http, page, &page.image_url, &headers).await;
    for url in &page.fallback_urls {
        if result.is_ok() {
            break;
        }
        tracing::debug!(%url, "retrying page from fallback URL");
        result = fetch_url(http, page, url, &headers).await;
    }
    result
}

async fn fetch_url(
    http: &Http,
    page: &Page,
    url: &str,
    headers: &reqwest::header::HeaderMap,
) -> Result<Bytes> {
    let mut last_error = None;
    for attempt in 0..4u64 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(400 * attempt)).await;
        }
        let response = match http.client().get(url).headers(headers.clone()).send().await {
            Ok(response) => response,
            Err(source) => {
                last_error = Some(Error::Http {
                    url: url.to_string(),
                    source,
                });
                continue;
            }
        };
        let status = response.status();
        if status == StatusCode::FORBIDDEN || status == StatusCode::SERVICE_UNAVAILABLE {
            return http.get_bytes(url, headers).await;
        }
        if !status.is_success() {
            last_error = Some(Error::HttpStatus {
                status: status.as_u16(),
                url: url.to_string(),
            });
            // Client errors other than rate limiting are final for this URL;
            // the caller moves on to the page's fallback URLs.
            if status.is_client_error() && status != StatusCode::TOO_MANY_REQUESTS {
                break;
            }
            continue;
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(source) => {
                last_error = Some(Error::Http {
                    url: url.to_string(),
                    source,
                });
                continue;
            }
        };
        if image_extension(&bytes).is_none() && content_type.contains("text/html") {
            return Err(Error::download(format!(
                "page {} returned HTML instead of an image ({})",
                page.index + 1,
                url
            )));
        }
        return Ok(bytes);
    }
    Err(last_error.unwrap_or_else(|| Error::download(format!("could not fetch {url}"))))
}

/// Detects the image type from magic bytes; URLs and content types are not
/// reliable across CDNs.
pub fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("webp"),
        [
            _,
            _,
            _,
            _,
            b'f',
            b't',
            b'y',
            b'p',
            b'a',
            b'v',
            b'i',
            b'f',
            ..,
        ] => Some("avif"),
        [b'B', b'M', ..] => Some("bmp"),
        _ => None,
    }
}

fn page_name(index: usize, width: usize, bytes: &[u8]) -> String {
    format!(
        "{:0width$}.{}",
        index + 1,
        image_extension(bytes).unwrap_or("jpg")
    )
}

fn progress_bar(total: u64, label: &str, visible: bool) -> ProgressBar {
    if !visible {
        return ProgressBar::hidden();
    }
    let bar = ProgressBar::new(total);
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {msg:16} [{bar:36.cyan/blue}] {pos}/{len} pages ({eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=>-"),
    );
    bar.set_message(label.to_string());
    bar.enable_steady_tick(Duration::from_millis(200));
    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(number: f32, name: &str) -> Chapter {
        Chapter {
            id: "1".to_string(),
            url: String::new(),
            name: name.to_string(),
            number,
            volume: None,
            scanlator: None,
            date_upload: None,
            language: None,
            pages: None,
        }
    }

    #[test]
    fn chapter_paths_are_zero_padded_and_sanitized() {
        let manga = Manga::new(
            "test",
            "1",
            "Bocchi the Rock!: Side/Story",
            "https://x.test",
        );
        let root = Path::new("/tmp/manga");
        assert_eq!(
            chapter_path(root, &manga, &chapter(12.0, ""), ChapterFormat::Cbz),
            Path::new("/tmp/manga/Bocchi the Rock! - Side - Story/Chapter 0012.cbz")
        );
        assert_eq!(
            chapter_path(root, &manga, &chapter(10.5, ""), ChapterFormat::Folder),
            Path::new("/tmp/manga/Bocchi the Rock! - Side - Story/Chapter 0010.5")
        );
        assert_eq!(
            chapter_path(root, &manga, &chapter(-1.0, "Oneshot"), ChapterFormat::Cbz),
            Path::new("/tmp/manga/Bocchi the Rock! - Side - Story/Oneshot.cbz")
        );
    }

    #[test]
    fn detects_image_types_from_magic_bytes() {
        assert_eq!(image_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_extension(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_extension(b"\0\0\0\x1cftypavif"), Some("avif"));
        assert_eq!(image_extension(b"<!DOCTYPE html>"), None);
        assert_eq!(page_name(0, 3, b"\x89PNG"), "001.png");
    }

    #[test]
    fn partial_path_appends_suffix() {
        assert_eq!(
            partial_path(Path::new("/a/Chapter 0001.cbz")),
            Path::new("/a/Chapter 0001.cbz.part")
        );
    }
}
