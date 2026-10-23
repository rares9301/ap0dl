use std::path::PathBuf;

use anyhow::Result;

use super::interactive::{
    apply_range, choose_manga, is_interactive, load_chapters, pick_chapters_to_download,
};
use super::read::pages_with_fallback;
use crate::config::ChapterFormat;
use crate::download::{DownloadOptions, DownloadedChapter, download_chapter};
use crate::engine::{MangaEngine, ResolvedManga};
use crate::models::Chapter;

pub struct DownloadArgs {
    pub query: Option<String>,
    pub source: Option<String>,
    pub index: Option<usize>,
    pub range: Option<String>,
    pub dir: Option<PathBuf>,
    pub format: Option<ChapterFormat>,
    pub scanlator: Option<String>,
    pub json: bool,
    pub print_url: bool,
    pub no_interactive: bool,
}

pub async fn run(engine: &MangaEngine, args: DownloadArgs) -> Result<()> {
    let interactive = is_interactive(args.no_interactive);
    let resolved = choose_manga(
        engine,
        args.query.as_deref(),
        args.source.as_deref(),
        args.index,
        interactive,
    )
    .await?;
    let chapters = load_chapters(engine, &resolved, args.scanlator.as_deref()).await?;
    let selected = match args.range.as_deref() {
        Some(range) => apply_range(&chapters, range)?,
        None if interactive => pick_chapters_to_download(&chapters)?,
        None => chapters,
    };

    let root = args
        .dir
        .clone()
        .unwrap_or_else(|| engine.manga.download_root(&engine.config));
    let options = DownloadOptions {
        format: args.format.unwrap_or(engine.manga.format),
        concurrency: engine.manga.concurrency,
        progress: !args.json && !args.print_url,
    };
    let (succeeded, failed) = download_many(
        engine,
        &resolved,
        &selected,
        &root,
        &options,
        args.json,
        args.print_url,
    )
    .await?;
    if succeeded == 0 && failed > 0 {
        anyhow::bail!("every selected chapter failed to download");
    }
    Ok(())
}

/// Downloads chapters one after another (pages within a chapter run in
/// parallel). Returns (succeeded, failed); failures are reported and the
/// remaining chapters still download, like the anime downloader.
pub(super) async fn download_many(
    engine: &MangaEngine,
    resolved: &ResolvedManga,
    chapters: &[Chapter],
    root: &std::path::Path,
    options: &DownloadOptions,
    json: bool,
    print_url: bool,
) -> Result<(usize, usize)> {
    if !json && !print_url {
        eprintln!(
            "Downloading {} chapter(s) of {} from {} into {}",
            chapters.len(),
            resolved.manga.title,
            resolved.source.name(),
            root.display()
        );
    }
    let mut succeeded = 0;
    let mut failed = 0;
    let mut results = Vec::new();
    for chapter in chapters {
        let (used, used_chapter, pages) = match pages_with_fallback(engine, resolved, chapter).await
        {
            Ok(found) => found,
            Err(error) => {
                eprintln!("Download failed for {}: {error}", chapter.label());
                failed += 1;
                continue;
            }
        };
        if print_url {
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "manga": used.manga.title,
                        "source": used.source.id(),
                        "chapter": used_chapter,
                        "pages": pages,
                    }))?
                );
            } else {
                for page in &pages {
                    println!("{}", page.image_url);
                }
            }
            succeeded += 1;
            continue;
        }
        match download_chapter(
            &engine.http,
            &used.manga,
            &used_chapter,
            &pages,
            root,
            options,
        )
        .await
        {
            Ok(done) => {
                succeeded += 1;
                if json {
                    results.push(result_json(&used, &used_chapter, &done));
                } else {
                    report(&used_chapter, &done);
                }
            }
            Err(error) => {
                failed += 1;
                eprintln!("Download failed for {}: {error}", chapter.label());
            }
        }
    }
    if json && !print_url {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }
    Ok((succeeded, failed))
}

/// Single-chapter download used by the reader's "Download this chapter".
pub(super) async fn download_one(
    engine: &MangaEngine,
    resolved: &ResolvedManga,
    chapter: &Chapter,
    options: &DownloadOptions,
) {
    let root = engine.manga.download_root(&engine.config);
    if let Err(error) = download_many(
        engine,
        resolved,
        std::slice::from_ref(chapter),
        &root,
        options,
        false,
        false,
    )
    .await
    {
        eprintln!("Download failed for {}: {error}", chapter.label());
    }
}

fn report(chapter: &Chapter, done: &DownloadedChapter) {
    if done.skipped {
        eprintln!("{} already downloaded.", chapter.label());
    } else {
        eprintln!(
            "{}: {} pages, {}",
            chapter.label(),
            done.pages,
            ap0dl_core::util::human_bytes(done.bytes)
        );
    }
    println!("{}", done.path.display());
}

fn result_json(
    resolved: &ResolvedManga,
    chapter: &Chapter,
    done: &DownloadedChapter,
) -> serde_json::Value {
    serde_json::json!({
        "manga": resolved.manga.title,
        "source": resolved.source.id(),
        "chapter": chapter.number,
        "name": chapter.name,
        "path": done.path,
        "pages": done.pages,
        "bytes": done.bytes,
        "skipped": done.skipped,
    })
}
