use anyhow::Result;

use super::LibraryCommand;
use super::download::download_many;
use super::interactive::{choose_manga, is_interactive};
use super::to_anyhow;
use crate::download::DownloadOptions;
use crate::engine::{MangaEngine, ResolvedManga};
use crate::library::Library;
use crate::selection::{dedupe_chapters, sort_chapters};

pub async fn run(engine: &MangaEngine, command: LibraryCommand) -> Result<()> {
    let mut library = Library::load(&engine.manga.library_file()).map_err(to_anyhow)?;
    match command {
        LibraryCommand::List { json } => list(&library, json),
        LibraryCommand::Add {
            query,
            source,
            index,
            no_interactive,
        } => {
            let resolved = choose_manga(
                engine,
                Some(&query),
                source.as_deref(),
                index,
                is_interactive(no_interactive),
            )
            .await?;
            let chapters = engine.chapters(&resolved).await.map_err(to_anyhow)?;
            let added = library.add(&resolved.manga, &chapters);
            library.save().map_err(to_anyhow)?;
            eprintln!(
                "{} {} [{}] with {} chapter(s).",
                if added { "Added" } else { "Refreshed" },
                resolved.manga.title,
                resolved.source.name(),
                chapters.len()
            );
            Ok(())
        }
        LibraryCommand::Remove { query } => {
            let removed = library.remove(&query);
            if removed.is_empty() {
                anyhow::bail!("{query:?} is not in the library");
            }
            library.save().map_err(to_anyhow)?;
            for entry in removed {
                eprintln!("Removed {} [{}].", entry.title, entry.source);
            }
            Ok(())
        }
        LibraryCommand::Update { json, download } => {
            update(engine, &mut library, json, download).await
        }
    }
}

fn list(library: &Library, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(library.entries())?);
        return Ok(());
    }
    if library.entries().is_empty() {
        eprintln!("The library is empty. Add a series with `ap0dl manga library add \"title\"`.");
        return Ok(());
    }
    for entry in library.entries() {
        let progress = entry
            .last_read
            .map(|last| format!("read up to {last}"))
            .unwrap_or_else(|| "unread".to_string());
        println!(
            "[{}] {} — {} chapter(s), {progress}, {}",
            entry.source,
            entry.title,
            entry.total_chapters,
            entry.status.label()
        );
        println!("    {}", entry.url);
    }
    Ok(())
}

/// Aniyomi's library update: refetch every followed series, report chapters
/// that were not there last time, optionally download them.
async fn update(
    engine: &MangaEngine,
    library: &mut Library,
    json: bool,
    download: bool,
) -> Result<()> {
    let mut report = Vec::new();
    let options = DownloadOptions {
        format: engine.manga.format,
        concurrency: engine.manga.concurrency,
        progress: !json,
    };
    let root = engine.manga.download_root(&engine.config);
    let targets: Vec<(String, String, String)> = library
        .entries()
        .iter()
        .map(|entry| (entry.source.clone(), entry.id.clone(), entry.url.clone()))
        .collect();

    for (source_id, id, url) in targets {
        let Some(source) = engine.get(&source_id) else {
            eprintln!("Skipping {url}: source {source_id} is not compiled in.");
            continue;
        };
        let manga = match source.details(&engine.http, &url).await {
            Ok(manga) => manga,
            Err(error) => {
                eprintln!("Update failed for {url}: {error}");
                continue;
            }
        };
        let resolved = ResolvedManga { source, manga };
        let mut chapters = match engine.chapters(&resolved).await {
            Ok(chapters) => chapters,
            Err(error) => {
                eprintln!("Update failed for {}: {error}", resolved.manga.title);
                continue;
            }
        };
        sort_chapters(&mut chapters);
        let Some(entry) = library.get_mut(&source_id, &id) else {
            continue;
        };
        entry.status = resolved.manga.status;
        let new = entry.apply_update(&chapters);
        if !json {
            if new.is_empty() {
                eprintln!("{}: no new chapters.", resolved.manga.title);
            } else {
                println!("{}: {} new chapter(s)", resolved.manga.title, new.len());
                for chapter in &new {
                    println!("    {}", chapter.display_title());
                }
            }
        }
        if download && !new.is_empty() {
            let new = dedupe_chapters(new.clone(), None);
            download_many(engine, &resolved, &new, &root, &options, false, false).await?;
        }
        report.push(serde_json::json!({
            "source": source_id,
            "id": id,
            "title": resolved.manga.title,
            "new_chapters": new,
        }));
    }
    library.save().map_err(to_anyhow)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(())
}
