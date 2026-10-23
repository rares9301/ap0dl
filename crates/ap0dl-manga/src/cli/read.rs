use anyhow::{Context, Result};
use dialoguer::{FuzzySelect, Input};

use super::download::download_one;
use super::interactive::{apply_range, choose_manga, is_interactive, load_chapters, pick_chapter};
use super::to_anyhow;
use crate::download::{DownloadOptions, download_chapter};
use crate::engine::{MangaEngine, ResolvedManga};
use crate::library::Library;
use crate::models::{Chapter, Page};
use crate::reader::{read_path, read_urls, reads_urls, resolve_reader};
use crate::spread::{PageLayout, ReadingDirection};
use crate::state::{ReadingProgress, ReadingState, nearest_chapter};

pub struct ReadOptions {
    pub query: Option<String>,
    pub source: Option<String>,
    pub index: Option<usize>,
    pub range: Option<String>,
    pub reader: Option<String>,
    pub reader_args: Vec<String>,
    pub scanlator: Option<String>,
    pub layout: Option<PageLayout>,
    pub direction: Option<ReadingDirection>,
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

/// Library and reading state, saved as the reader moves.
struct Progress {
    library: Library,
    state: ReadingState,
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
    let mut progress = Progress {
        library: Library::load(&engine.manga.library_file()).map_err(to_anyhow)?,
        state: ReadingState::load(&engine.manga.state_file()).map_err(to_anyhow)?,
    };

    let mut args = engine.manga.reader_args.clone();
    args.extend(options.reader_args.iter().cloned());
    let reader = Reader {
        binary: resolve_reader(options.reader.as_deref().unwrap_or(&engine.manga.reader)),
        args,
        print_url: options.print_url,
    };
    #[cfg(unix)]
    let viewer = viewer::Viewer::new(engine, &reader, &options, &progress, &resolved);

    if let Some(range) = options.range.as_deref() {
        let selected = apply_range(&chapters, range)?;
        #[cfg(unix)]
        if let Some(mut viewer) = viewer {
            viewer
                .show(engine, &resolved, &selected, (0, 0), &mut progress)
                .await?;
            return Ok(());
        }
        for chapter in selected {
            if let Some(used) = open_chapter(engine, &resolved, &chapter, &reader).await? {
                resolved = used;
            }
            remember(&mut progress, &resolved, &chapter, &reader)?;
        }
        return Ok(());
    }

    let library_resume = progress
        .library
        .get(&resolved.manga.source, &resolved.manga.id)
        .map(|entry| entry.resume_index(&chapters));
    let saved = progress.state.find(&resolved.manga).cloned();
    let (mut position, page) = if interactive {
        ask_chapter(&chapters, saved.as_ref(), library_resume)?
    } else {
        saved
            .as_ref()
            .and_then(|saved| Some((saved.chapter_index(&chapters)?, saved.page)))
            .unwrap_or((library_resume.unwrap_or(0), 0))
    };

    #[cfg(unix)]
    if let Some(viewer) = viewer {
        return viewer
            .run(
                engine,
                &resolved,
                &chapters,
                (position, page),
                progress,
                interactive,
            )
            .await;
    }
    let _ = page;

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
        remember(&mut progress, &resolved, &chapter, &reader)?;
        if !interactive || reader.print_url {
            return Ok(());
        }

        loop {
            let following = progress
                .library
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
                    follow(&mut progress.library, &resolved, &chapters, position)?;
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

fn follow(
    library: &mut Library,
    resolved: &ResolvedManga,
    chapters: &[Chapter],
    position: usize,
) -> Result<()> {
    library.add(&resolved.manga, chapters);
    library.mark_read(
        &resolved.manga.source,
        &resolved.manga.id,
        chapters[position].number,
    );
    library.save().map_err(to_anyhow)?;
    eprintln!("Added {} to the library.", resolved.manga.title);
    Ok(())
}

fn remember(
    progress: &mut Progress,
    resolved: &ResolvedManga,
    chapter: &Chapter,
    reader: &Reader,
) -> Result<()> {
    if reader.print_url {
        return Ok(());
    }
    progress
        .state
        .record(ReadingProgress::new(&resolved.manga, chapter, 0, 0));
    progress.state.save().map_err(to_anyhow)?;
    if progress
        .library
        .mark_read(&resolved.manga.source, &resolved.manga.id, chapter.number)
    {
        progress.library.save().map_err(to_anyhow)?;
    }
    Ok(())
}

/// "What chapter do you want to read?" Enter keeps the default: the saved
/// chapter (resuming at its saved page), else the library's next unread
/// chapter, else the first. Accepts a number (the closest chapter is used
/// when it does not exist), `first`, `latest`, part of a title, or `list`
/// for the fuzzy picker. Returns (chapter index, page).
fn ask_chapter(
    chapters: &[Chapter],
    saved: Option<&ReadingProgress>,
    library_resume: Option<usize>,
) -> Result<(usize, usize)> {
    let saved_index = saved.and_then(|saved| saved.chapter_index(chapters));
    let default = saved_index.or(library_resume).unwrap_or(0);
    eprintln!(
        "{} chapters: {} to {}.",
        chapters.len(),
        chapters[0].label(),
        chapters[chapters.len() - 1].label()
    );
    if let (Some(saved), Some(_)) = (saved, saved_index) {
        eprintln!(
            "Saved: {}. Press Enter to continue from there.",
            saved.describe()
        );
    }
    let page_for = |index: usize| match (saved, saved_index) {
        (Some(saved), Some(saved_index)) if saved_index == index => saved.page,
        _ => 0,
    };

    loop {
        let answer: String = Input::new()
            .with_prompt("What chapter do you want to read? (number, latest, list)")
            .default(chapter_answer(&chapters[default]))
            .interact_text()
            .context("chapter prompt cancelled")?;
        match find_chapter(chapters, &answer) {
            Ok(Some(index)) => {
                if chapters[index].number >= 0.0
                    && answer_number(&answer).is_some_and(|number| number != chapters[index].number)
                {
                    eprintln!(
                        "There is no Chapter {}; opening {} (closest).",
                        answer.trim(),
                        chapters[index].label()
                    );
                }
                return Ok((index, page_for(index)));
            }
            Ok(None) => {
                if let Some(index) = pick_chapter(chapters, "Select chapter", default) {
                    return Ok((index, page_for(index)));
                }
            }
            Err(message) => eprintln!("{message}"),
        }
    }
}

/// What the prompt shows for a chapter: its number, or its name.
fn chapter_answer(chapter: &Chapter) -> String {
    if chapter.number < 0.0 {
        chapter.label()
    } else if chapter.number.fract() == 0.0 {
        format!("{}", chapter.number as i64)
    } else {
        format!("{}", chapter.number)
    }
}

fn answer_number(answer: &str) -> Option<f32> {
    let lower = answer.trim().to_ascii_lowercase();
    let number = ["chapter", "ch.", "ch", "#"]
        .iter()
        .find_map(|prefix| lower.strip_prefix(prefix))
        .unwrap_or(&lower)
        .trim();
    number
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite())
}

/// `Ok(None)` asks for the picker.
fn find_chapter(chapters: &[Chapter], answer: &str) -> std::result::Result<Option<usize>, String> {
    let lower = answer.trim().to_ascii_lowercase();
    match lower.as_str() {
        "list" | "l" | "?" | "pick" => return Ok(None),
        "first" | "start" => return Ok(Some(0)),
        "latest" | "last" | "newest" => return Ok(Some(chapters.len() - 1)),
        _ => {}
    }
    if let Some(number) = answer_number(&lower) {
        if let Some(index) = chapters.iter().position(|chapter| chapter.number == number) {
            return Ok(Some(index));
        }
        if let Some(index) = nearest_chapter(chapters, number) {
            return Ok(Some(index));
        }
    }
    chapters
        .iter()
        .position(|chapter| {
            chapter
                .display_title()
                .to_ascii_lowercase()
                .contains(&lower)
        })
        .map(Some)
        .ok_or_else(|| {
            format!(
                "No chapter matches {:?}; type a number, first, latest or list.",
                answer.trim()
            )
        })
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

/// The continuous mpv viewer (Unix: it talks to mpv over a Unix socket).
#[cfg(unix)]
mod viewer {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use anyhow::Result;
    use async_trait::async_trait;
    use dialoguer::FuzzySelect;

    use super::{
        Progress, ReadOptions, Reader, ask_chapter, download_one, download_options, follow,
        pages_with_fallback,
    };
    use crate::cli::to_anyhow;
    use crate::config::ChapterFormat;
    use crate::download::{DownloadOptions, download_chapter};
    use crate::engine::{MangaEngine, ResolvedManga};
    use crate::models::Chapter;
    use crate::reader::reads_urls;
    use crate::spread::PageLayout;
    use crate::state::ReadingProgress;
    use crate::viewer::{ChapterFeed, Position, ViewerOptions, ViewerSettings, view};

    pub(super) struct Viewer {
        options: ViewerOptions,
        settings: ViewerSettings,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AfterView {
        Continue,
        Another,
        Download,
        Follow,
        Quit,
    }

    /// Removes the page cache when the read command ends.
    struct Cleanup(PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Viewer {
        /// `None` when the reader is not mpv (other viewers, `--print-url`).
        /// Windows `mpv.exe` cannot be driven from WSL, so Linux `mpv` is used
        /// when installed.
        pub(super) fn new(
            engine: &MangaEngine,
            reader: &Reader,
            options: &ReadOptions,
            progress: &Progress,
            resolved: &ResolvedManga,
        ) -> Option<Self> {
            if reader.print_url || !reads_urls(&reader.binary) {
                return None;
            }
            let name = Path::new(&reader.binary)
                .file_name()?
                .to_string_lossy()
                .to_ascii_lowercase();
            let mpv = match name.as_str() {
                "mpv" => reader.binary.clone(),
                "mpv.exe" if which::which("mpv").is_ok() => "mpv".to_string(),
                _ => return None,
            };
            let saved = progress.state.find(&resolved.manga);
            let layout = options
                .layout
                .or(saved.and_then(|saved| saved.layout))
                .unwrap_or(engine.manga.page_layout);
            let ffmpeg = which::which(&engine.config.ffmpeg).ok();
            if ffmpeg.is_none() && layout == PageLayout::Double {
                eprintln!(
                    "{} not found; showing single pages (double page needs ffmpeg).",
                    engine.config.ffmpeg
                );
            }
            Some(Self {
                options: ViewerOptions {
                    mpv,
                    title: resolved.manga.title.clone(),
                    mpv_args: reader.args.clone(),
                    direction: options.direction.unwrap_or(engine.manga.reading_direction),
                    ffmpeg,
                    work_dir: std::env::temp_dir()
                        .join(format!("ap0dl-manga-view-{}", std::process::id())),
                },
                settings: ViewerSettings {
                    layout,
                    offset: saved.is_some_and(|saved| saved.offset),
                },
            })
        }

        /// Opens the viewer, then (interactively) offers continue / another
        /// chapter / download / follow / quit until the user quits.
        pub(super) async fn run(
            mut self,
            engine: &MangaEngine,
            resolved: &ResolvedManga,
            chapters: &[Chapter],
            mut start: (usize, usize),
            mut progress: Progress,
            interactive: bool,
        ) -> Result<()> {
            loop {
                let now = self
                    .show(engine, resolved, chapters, start, &mut progress)
                    .await?;
                if !interactive {
                    return Ok(());
                }
                loop {
                    let following = progress
                        .library
                        .get(&resolved.manga.source, &resolved.manga.id)
                        .is_some();
                    match after_view(&chapters[now.chapter], now, following) {
                        AfterView::Continue => {
                            start = (now.chapter, now.page);
                            break;
                        }
                        AfterView::Another => {
                            let saved = progress.state.find(&resolved.manga).cloned();
                            start = ask_chapter(chapters, saved.as_ref(), None)?;
                            break;
                        }
                        AfterView::Download => {
                            download_one(
                                engine,
                                resolved,
                                &chapters[now.chapter],
                                &download_options(engine),
                            )
                            .await;
                        }
                        AfterView::Follow => {
                            follow(&mut progress.library, resolved, chapters, now.chapter)?;
                        }
                        AfterView::Quit => return Ok(()),
                    }
                }
            }
        }

        /// One viewer session over `chapters`; saves the reading state on
        /// every page turn and marks followed chapters read at their last page.
        pub(super) async fn show(
            &mut self,
            engine: &MangaEngine,
            resolved: &ResolvedManga,
            chapters: &[Chapter],
            (chapter, page): (usize, usize),
            progress: &mut Progress,
        ) -> Result<Position> {
            let _cleanup = Cleanup(self.options.work_dir.clone());
            let feed = Feed {
                engine,
                resolved,
                chapters,
                progress: AtomicBool::new(true),
            };
            eprintln!(
                "Reading {} from {} ({}, {}). Press ? in the viewer for keys.",
                resolved.manga.title,
                chapters[chapter].label(),
                self.settings.layout.label(),
                match self.options.direction {
                    crate::spread::ReadingDirection::Rtl => "right to left",
                    crate::spread::ReadingDirection::Ltr => "left to right",
                }
            );
            let mut on_change = |position: Position, settings: ViewerSettings| {
                let mut saved = ReadingProgress::new(
                    &resolved.manga,
                    &chapters[position.chapter],
                    position.page,
                    position.pages,
                );
                saved.layout = Some(settings.layout);
                saved.offset = settings.offset;
                progress.state.record(saved);
                if let Err(error) = progress.state.save() {
                    tracing::warn!("could not save reading state: {error}");
                }
                if position.last
                    && progress.library.mark_read(
                        &resolved.manga.source,
                        &resolved.manga.id,
                        chapters[position.chapter].number,
                    )
                    && let Err(error) = progress.library.save()
                {
                    tracing::warn!("could not save the manga library: {error}");
                }
            };
            let outcome = view(
                &feed,
                &self.options,
                self.settings,
                chapter,
                page,
                &mut on_change,
            )
            .await
            .map_err(to_anyhow)?;
            self.settings = outcome.settings;
            let position = outcome.position;
            eprintln!(
                "Stopped at {}, page {}/{}.",
                chapters[position.chapter].label(),
                position.page + 1,
                position.pages
            );
            Ok(position)
        }
    }

    fn after_view(chapter: &Chapter, now: Position, following: bool) -> AfterView {
        let mut items = vec![
            (
                AfterView::Continue,
                format!(
                    "Continue reading ({}, page {}/{})",
                    chapter.label(),
                    now.page + 1,
                    now.pages
                ),
            ),
            (AfterView::Another, "Read another chapter".to_string()),
            (AfterView::Download, format!("Download {}", chapter.label())),
        ];
        if !following {
            items.push((AfterView::Follow, "Add to library".to_string()));
        }
        items.push((AfterView::Quit, "Quit".to_string()));
        let labels: Vec<&str> = items.iter().map(|(_, label)| label.as_str()).collect();
        match FuzzySelect::new()
            .with_prompt("Viewer closed — what next?")
            .items(&labels)
            .default(labels.len() - 1)
            .interact()
        {
            Ok(choice) => items[choice].0,
            Err(_) => AfterView::Quit,
        }
    }

    /// Downloads chapters into the viewer's cache as image folders, with
    /// the same cross-source fallback as the per-chapter reader.
    struct Feed<'a> {
        engine: &'a MangaEngine,
        resolved: &'a ResolvedManga,
        chapters: &'a [Chapter],
        /// Progress bar for the first chapter only; later ones load while
        /// the viewer is open.
        progress: AtomicBool,
    }

    #[async_trait]
    impl ChapterFeed for Feed<'_> {
        fn chapter_count(&self) -> usize {
            self.chapters.len()
        }

        fn label(&self, index: usize) -> String {
            self.chapters[index].label()
        }

        async fn load(&self, index: usize, directory: &Path) -> ap0dl_core::Result<Vec<PathBuf>> {
            let chapter = &self.chapters[index];
            let (used, used_chapter, pages) =
                pages_with_fallback(self.engine, self.resolved, chapter)
                    .await
                    .map_err(|error| ap0dl_core::Error::download(error.to_string()))?;
            let options = DownloadOptions {
                format: ChapterFormat::Folder,
                concurrency: self.engine.manga.concurrency,
                progress: self.progress.swap(false, Ordering::Relaxed),
            };
            let downloaded = download_chapter(
                &self.engine.http,
                &used.manga,
                &used_chapter,
                &pages,
                directory,
                &options,
            )
            .await?;
            let mut files: Vec<PathBuf> = std::fs::read_dir(&downloaded.path)?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_file())
                .collect();
            files.sort();
            Ok(files)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(number: f32, name: &str) -> Chapter {
        Chapter {
            id: format!("c{number}"),
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
    fn chapter_answers_resolve_numbers_words_and_titles() {
        let chapters = [
            chapter(1.0, "Chapter 1"),
            chapter(2.0, "Chapter 2 - Kessoku Band"),
            chapter(2.5, "Chapter 2.5"),
            chapter(10.0, "Chapter 10"),
        ];
        assert_eq!(find_chapter(&chapters, "2"), Ok(Some(1)));
        assert_eq!(find_chapter(&chapters, "ch 2.5"), Ok(Some(2)));
        assert_eq!(find_chapter(&chapters, "Chapter 10"), Ok(Some(3)));
        assert_eq!(find_chapter(&chapters, "7"), Ok(Some(3)));
        assert_eq!(find_chapter(&chapters, "latest"), Ok(Some(3)));
        assert_eq!(find_chapter(&chapters, "first"), Ok(Some(0)));
        assert_eq!(find_chapter(&chapters, "list"), Ok(None));
        assert_eq!(find_chapter(&chapters, "kessoku"), Ok(Some(1)));
        assert!(find_chapter(&chapters, "nothing like this").is_err());
        assert_eq!(chapter_answer(&chapters[2]), "2.5");
        assert_eq!(chapter_answer(&chapters[3]), "10");
    }
}
