use anyhow::{Context, Result};
use dialoguer::FuzzySelect;

use super::download::download_one;
use super::interactive::{apply_range, choose_manga, is_interactive, load_chapters, pick_chapter};
use super::to_anyhow;
use crate::download::{DownloadOptions, download_chapter};
use crate::engine::{MangaEngine, ResolvedManga};
use crate::library::Library;
use crate::models::{Chapter, Page};
use crate::reader::{read_path, read_urls, reads_urls, resolve_reader};

pub struct ReadOptions {
    pub query: Option<String>,
    pub source: Option<String>,
    pub index: Option<usize>,
    pub range: Option<String>,
    pub reader: Option<String>,
    pub reader_args: Vec<String>,
    pub scanlator: Option<String>,
    pub print_url: bool,
    pub no_interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReaderAction {
    Next,
    Previous,
    Reread,
    Download,
    Follow,
    PickChapter,
    Quit,
}

struct Reader {
    binary: String,
    args: Vec<String>,
    print_url: bool,
}

pub async fn run(engine: &MangaEngine, options: ReadOptions) -> Result<()> {
    let interactive = is_interactive(options.no_interactive);
    let mut resolved = choose_manga(
        engine,
        options.query.as_deref(),
        options.source.as_deref(),
        options.index,
        interactive,
    )
    .await?;
    let scanlator = options.scanlator.as_deref();
    let mut chapters = load_chapters(engine, &resolved, scanlator).await?;
    let library_path = engine.manga.library_file();
    let mut library = Library::load(&library_path).map_err(to_anyhow)?;

    let mut args = engine.manga.reader_args.clone();
    args.extend(options.reader_args);
    let reader = Reader {
        binary: resolve_reader(options.reader.as_deref().unwrap_or(&engine.manga.reader)),
        args,
        print_url: options.print_url,
    };

    if let Some(range) = options.range.as_deref() {
        for chapter in apply_range(&chapters, range)? {
            if let Some(used) = open_chapter(engine, &resolved, &chapter, &reader).await? {
                resolved = used;
            }
            remember(&mut library, &resolved, &chapter, &reader)?;
        }
        return Ok(());
    }

    let resume = library
        .get(&resolved.manga.source, &resolved.manga.id)
        .map(|entry| entry.resume_index(&chapters))
        .unwrap_or(0);
    let mut position = if interactive {
        pick_chapter(&chapters, "Select chapter (type to fuzzy-filter)", resume)
            .context("chapter selection cancelled")?
    } else {
        resume
    };

    loop {
        let chapter = chapters[position].clone();
        if let Some(used) = open_chapter(engine, &resolved, &chapter, &reader).await? {
            // Keep navigating on the source that actually served the pages.
            resolved = used;
            chapters = load_chapters(engine, &resolved, scanlator).await?;
            position = chapters
                .iter()
                .position(|candidate| candidate.number == chapter.number)
                .unwrap_or(0);
        }
        remember(&mut library, &resolved, &chapter, &reader)?;
        if !interactive || reader.print_url {
            return Ok(());
        }

        loop {
            let following = library
                .get(&resolved.manga.source, &resolved.manga.id)
                .is_some();
            match choose_action(&chapters, position, following) {
                ReaderAction::Next => {
                    position += 1;
                    break;
                }
                ReaderAction::Previous => {
                    position -= 1;
                    break;
                }
                ReaderAction::Reread => break,
                ReaderAction::Download => {
                    let chapter = &chapters[position];
                    download_one(engine, &resolved, chapter, &download_options(engine)).await;
                }
                ReaderAction::Follow => {
                    library.add(&resolved.manga, &chapters);
                    library.mark_read(
                        &resolved.manga.source,
                        &resolved.manga.id,
                        chapters[position].number,
                    );
                    library.save().map_err(to_anyhow)?;
                    eprintln!("Added {} to the library.", resolved.manga.title);
                }
                ReaderAction::PickChapter => {
                    if let Some(choice) = pick_chapter(&chapters, "Select chapter", position) {
                        position = choice;
                        break;
                    }
                }
                ReaderAction::Quit => return Ok(()),
            }
        }
    }
}

fn download_options(engine: &MangaEngine) -> DownloadOptions {
    DownloadOptions {
        format: engine.manga.format,
        concurrency: engine.manga.concurrency,
        progress: true,
    }
}

fn remember(
    library: &mut Library,
    resolved: &ResolvedManga,
    chapter: &Chapter,
    reader: &Reader,
) -> Result<()> {
    if !reader.print_url
        && library.mark_read(&resolved.manga.source, &resolved.manga.id, chapter.number)
    {
        library.save().map_err(to_anyhow)?;
    }
    Ok(())
}

/// Resolves pages (falling back to another source if needed) and shows
/// them. Returns the fallback series when a different source served them.
async fn open_chapter(
    engine: &MangaEngine,
    resolved: &ResolvedManga,
    chapter: &Chapter,
    reader: &Reader,
) -> Result<Option<ResolvedManga>> {
    let (used, used_chapter, pages) = pages_with_fallback(engine, resolved, chapter).await?;
    if reader.print_url {
        for page in &pages {
            println!("{}", page.image_url);
        }
    } else {
        let title = format!("{} - {}", used.manga.title, used_chapter.display_title());
        eprintln!(
            "Reading {title} ({} pages, {})",
            pages.len(),
            used.source.name()
        );
        show(engine, &used, &used_chapter, &pages, &title, reader).await?;
    }
    Ok((used.source.id() != resolved.source.id()).then_some(used))
}

async fn show(
    engine: &MangaEngine,
    resolved: &ResolvedManga,
    chapter: &Chapter,
    pages: &[Page],
    title: &str,
    reader: &Reader,
) -> Result<()> {
    if reads_urls(&reader.binary) {
        return read_urls(&reader.binary, title, pages, &reader.args).map_err(to_anyhow);
    }
    // Viewers that cannot fetch URLs (feh, imv, zathura, ...) get a local
    // copy in a per-process temp folder that is removed afterwards.
    let root = std::env::temp_dir().join(format!("ap0dl-manga-{}", std::process::id()));
    let options = DownloadOptions {
        format: crate::config::ChapterFormat::Folder,
        concurrency: engine.manga.concurrency,
        progress: true,
    };
    let downloaded = download_chapter(
        &engine.http,
        &resolved.manga,
        chapter,
        pages,
        &root,
        &options,
    )
    .await
    .map_err(to_anyhow)?;
    let result =
        read_path(&reader.binary, title, &downloaded.path, &reader.args).map_err(to_anyhow);
    let _ = std::fs::remove_dir_all(&root);
    result
}

/// Pages for `chapter`, or for the same title and chapter number on another
/// source when this one fails.
pub(super) async fn pages_with_fallback(
    engine: &MangaEngine,
    resolved: &ResolvedManga,
    chapter: &Chapter,
) -> Result<(ResolvedManga, Chapter, Vec<Page>)> {
    match engine.pages(resolved, chapter).await {
        Ok(pages) => Ok((resolved.clone(), chapter.clone(), pages)),
        Err(error) => {
            eprintln!(
                "{} pages failed on {}: {error}; trying another source...",
                chapter.label(),
                resolved.source.name()
            );
            match engine
                .fallback_pages(&resolved.manga, resolved.source.id(), chapter.number)
                .await
            {
                Some((fallback, fallback_chapter, pages)) => {
                    eprintln!("Using {} instead.", fallback.source.name());
                    Ok((fallback, fallback_chapter, pages))
                }
                None => Err(to_anyhow(error)),
            }
        }
    }
}

fn choose_action(chapters: &[Chapter], position: usize, following: bool) -> ReaderAction {
    let chapter = &chapters[position];
    let mut items: Vec<(ReaderAction, String)> = Vec::new();
    if let Some(next) = chapters.get(position + 1) {
        items.push((
            ReaderAction::Next,
            format!("Next: {}", next.display_title()),
        ));
    }
    if position > 0 {
        items.push((
            ReaderAction::Previous,
            format!("Previous: {}", chapters[position - 1].display_title()),
        ));
    }
    items.push((ReaderAction::Reread, "Read this chapter again".to_string()));
    items.push((ReaderAction::Download, "Download this chapter".to_string()));
    if !following {
        items.push((ReaderAction::Follow, "Add to library".to_string()));
    }
    items.push((
        ReaderAction::PickChapter,
        "Pick another chapter".to_string(),
    ));
    items.push((ReaderAction::Quit, "Quit".to_string()));

    let labels: Vec<String> = items.iter().map(|(_, label)| label.clone()).collect();
    match FuzzySelect::new()
        .with_prompt(format!("Finished {} — what next?", chapter.display_title()))
        .items(&labels)
        .default(0)
        .interact()
    {
        Ok(choice) => items[choice].0,
        Err(_) => ReaderAction::Quit,
    }
}
