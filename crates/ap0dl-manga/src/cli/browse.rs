use anyhow::Result;

use super::interactive::{apply_range, choose_manga, load_chapters};
use super::to_anyhow;
use crate::engine::MangaEngine;
use crate::filters::{FilterKind, FilterValues};
use crate::models::Manga;

pub fn sources(engine: &MangaEngine, json: bool, filters: Option<&str>) -> Result<()> {
    if let Some(id) = filters {
        let source = engine.require(id).map_err(to_anyhow)?;
        let filters = source.filters();
        if json {
            println!("{}", serde_json::to_string_pretty(&filters)?);
            return Ok(());
        }
        println!("{} search filters (use --filter key=value):", source.name());
        for filter in filters {
            match filter.kind {
                FilterKind::Text => println!("  {:<12} {} (text)", filter.key, filter.name),
                FilterKind::Select { options } | FilterKind::Multi { options } => {
                    println!("  {:<12} {}", filter.key, filter.name);
                    let values: Vec<&str> = options.iter().map(|option| option.value).collect();
                    println!("               {}", values.join(" | "));
                }
            }
        }
        return Ok(());
    }

    let infos = engine.source_infos();
    if json {
        println!("{}", serde_json::to_string_pretty(&infos)?);
        return Ok(());
    }
    println!(
        "{:<12} {:<14} {:<5} {:<8} BASE",
        "ID", "NAME", "LANG", "STATUS"
    );
    for info in infos {
        println!(
            "{:<12} {:<14} {:<5} {:<8} {}",
            info.id,
            info.name,
            info.lang,
            format!("{:?}", info.status).to_lowercase(),
            info.base_url
        );
        if !info.notes.is_empty() {
            println!("             {}", info.notes);
        }
    }
    Ok(())
}

pub async fn search(
    engine: &MangaEngine,
    query: &str,
    source: Option<&str>,
    filters: &[String],
    page: u32,
    limit: usize,
    json: bool,
) -> Result<()> {
    let filters = FilterValues::parse(filters).map_err(to_anyhow)?;
    let hits = engine
        .search(source, query, page, limit, &filters)
        .await
        .map_err(to_anyhow)?;
    if json {
        let mangas: Vec<&Manga> = hits.iter().map(|(_, manga)| manga).collect();
        println!("{}", serde_json::to_string_pretty(&mangas)?);
        return Ok(());
    }
    for (index, (source, manga)) in hits.iter().enumerate() {
        print_manga(index, source.name(), manga);
    }
    Ok(())
}

pub async fn browse(
    engine: &MangaEngine,
    source: Option<&str>,
    page: u32,
    json: bool,
    latest: bool,
) -> Result<()> {
    let id = source.unwrap_or(&engine.manga.default_source).to_string();
    let source = engine.require(&id).map_err(to_anyhow)?;
    let result = if latest {
        engine.latest(source.id(), page).await
    } else {
        engine.popular(source.id(), page).await
    }
    .map_err(to_anyhow)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if result.mangas.is_empty() {
        eprintln!("No titles on page {page} of {}.", source.name());
        return Ok(());
    }
    for (index, manga) in result.mangas.iter().enumerate() {
        print_manga(index, source.name(), manga);
    }
    if result.has_next_page {
        eprintln!("More results: --page {}", page + 1);
    }
    Ok(())
}

pub async fn chapters(
    engine: &MangaEngine,
    query: &str,
    source: Option<&str>,
    index: usize,
    json: bool,
) -> Result<()> {
    let resolved = choose_manga(engine, Some(query), source, Some(index), false).await?;
    // The listing shows every upload, including duplicate scanlations.
    let chapters = engine.chapters(&resolved).await.map_err(to_anyhow)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "manga": resolved.manga,
                "source": resolved.source.id(),
                "chapters": chapters,
            }))?
        );
        return Ok(());
    }

    let manga = &resolved.manga;
    println!("{} [{}]", manga.title, resolved.source.name());
    println!("{}", manga.url);
    let mut facts = vec![manga.status.label().to_string()];
    if let Some(author) = &manga.author {
        facts.push(author.clone());
    }
    if let Some(year) = manga.year {
        facts.push(year.to_string());
    }
    if !manga.genres.is_empty() {
        facts.push(manga.genres.join(", "));
    }
    println!("{}", facts.join(" · "));
    if let Some(description) = &manga.description {
        println!("{}", description.trim());
    }
    if chapters.is_empty() {
        eprintln!("No readable chapters on {}.", resolved.source.name());
    }
    for chapter in chapters {
        let number = if chapter.number < 0.0 {
            "-".to_string()
        } else {
            chapter.number.to_string()
        };
        println!("  {:>7}  {}", number, chapter.display_title());
    }
    Ok(())
}

pub async fn pages(
    engine: &MangaEngine,
    query: &str,
    source: Option<&str>,
    index: usize,
    range: &str,
    scanlator: Option<&str>,
) -> Result<()> {
    let resolved = choose_manga(engine, Some(query), source, Some(index), false).await?;
    let chapters = load_chapters(engine, &resolved, scanlator).await?;
    for chapter in apply_range(&chapters, range)? {
        let pages = engine.pages(&resolved, &chapter).await.map_err(to_anyhow)?;
        let payload = serde_json::json!({
            "manga": resolved.manga.title,
            "source": resolved.source.id(),
            "chapter": chapter,
            "pages": pages,
        });
        println!("{}", serde_json::to_string(&payload)?);
    }
    Ok(())
}

fn print_manga(index: usize, source: &str, manga: &Manga) {
    println!("{:>2}. [{}] {}", index + 1, source, manga.title);
    println!("    {}", manga.url);
    let mut facts = Vec::new();
    if let Some(year) = manga.year {
        facts.push(year.to_string());
    }
    if manga.status != Default::default() {
        facts.push(manga.status.label().to_string());
    }
    if let Some(format) = &manga.format {
        facts.push(format.clone());
    }
    if !facts.is_empty() {
        println!("    {}", facts.join(" · "));
    }
}
