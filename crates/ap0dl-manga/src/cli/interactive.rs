use std::io::IsTerminal;

use anyhow::{Context, Result};
use dialoguer::{FuzzySelect, Input, MultiSelect};

use super::to_anyhow;
use crate::engine::{MangaEngine, ResolvedManga};
use crate::filters::FilterValues;
use crate::models::Chapter;
use crate::selection::{ChapterSelection, dedupe_chapters};

pub fn is_interactive(no_interactive: bool) -> bool {
    !no_interactive && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Shared manga picker for `read`, `download` and `library add`. Mirrors the
/// anime picker: URLs resolve directly, `--index` picks a ranked hit, a TTY
/// gets a fuzzy list, scripts get the best match.
pub async fn choose_manga(
    engine: &MangaEngine,
    query: Option<&str>,
    source: Option<&str>,
    index: Option<usize>,
    interactive: bool,
) -> Result<ResolvedManga> {
    let query = match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(query) => query.to_string(),
        None if interactive => Input::<String>::new()
            .with_prompt("Search manga")
            .interact_text()
            .context("search prompt cancelled")?,
        None => anyhow::bail!(
            "a manga query is required when stdin is not a terminal; pass a title, for example: ap0dl manga read \"bocchi the rock\""
        ),
    };
    if query.trim().is_empty() {
        anyhow::bail!("empty manga query");
    }

    if query.starts_with("http://") || query.starts_with("https://") {
        return engine
            .resolve(&query, 1)
            .await
            .map_err(to_anyhow)
            .with_context(|| format!("could not resolve {query:?}"));
    }

    let hits = engine
        .search(source, &query, 1, 40, &FilterValues::default())
        .await
        .map_err(to_anyhow)
        .with_context(|| format!("could not search for {query:?}"))?;
    if hits.is_empty() {
        anyhow::bail!("no results for {query:?}");
    }

    let (source, manga) = if let Some(index) = index {
        hits[index.saturating_sub(1) % hits.len()].clone()
    } else if interactive {
        let labels: Vec<String> = hits
            .iter()
            .map(|(source, manga)| {
                let year = manga
                    .year
                    .map(|year| format!(" ({year})"))
                    .unwrap_or_default();
                format!("[{}] {}{}", source.name(), manga.title, year)
            })
            .collect();
        let choice = FuzzySelect::new()
            .with_prompt("Select manga (type to fuzzy-filter, Enter to pick)")
            .items(&labels)
            .default(0)
            .interact()
            .context("manga selection cancelled")?;
        hits[choice].clone()
    } else {
        hits.into_iter()
            .next()
            .expect("hits was checked as non-empty")
    };

    engine
        .resolve_hit(source, manga)
        .await
        .map_err(to_anyhow)
        .with_context(|| format!("could not resolve {query:?}"))
}

/// Chapters ascending with duplicate uploads collapsed (preferring
/// `scanlator`), which is the list every picker and range works on.
pub async fn load_chapters(
    engine: &MangaEngine,
    resolved: &ResolvedManga,
    scanlator: Option<&str>,
) -> Result<Vec<Chapter>> {
    let chapters = engine.chapters(resolved).await.map_err(to_anyhow)?;
    let chapters = dedupe_chapters(chapters, scanlator);
    if chapters.is_empty() {
        anyhow::bail!(
            "no readable chapters found for {} on {} (licensed series often have none on MangaDex; try another source)",
            resolved.manga.title,
            resolved.source.name()
        );
    }
    Ok(chapters)
}

pub fn apply_range(chapters: &[Chapter], range: &str) -> Result<Vec<Chapter>> {
    let selection: ChapterSelection = range
        .parse()
        .map_err(|error: String| anyhow::anyhow!("invalid chapter range: {error}"))?;
    let selected = selection.apply(chapters);
    if selected.is_empty() {
        anyhow::bail!("no chapters matched {range:?}");
    }
    Ok(selected)
}

/// Fuzzy single-chapter picker. `None` when cancelled.
pub fn pick_chapter(chapters: &[Chapter], prompt: &str, default: usize) -> Option<usize> {
    if chapters.is_empty() {
        return None;
    }
    let labels: Vec<String> = chapters.iter().map(Chapter::display_title).collect();
    FuzzySelect::new()
        .with_prompt(prompt)
        .items(&labels)
        .default(default.min(labels.len() - 1))
        .interact()
        .ok()
}

/// Checkbox chapter list whose first row selects everything.
pub fn pick_chapters_to_download(chapters: &[Chapter]) -> Result<Vec<Chapter>> {
    let mut labels = Vec::with_capacity(chapters.len() + 1);
    labels.push(format!("DOWNLOAD ALL CHAPTERS ({})", chapters.len()));
    labels.extend(chapters.iter().map(Chapter::display_title));
    let picks = MultiSelect::new()
        .with_prompt("Select chapters (space toggles, Enter starts download)")
        .items(&labels)
        .interact()
        .context("chapter selection cancelled")?;
    if picks.is_empty() {
        anyhow::bail!("no chapters selected");
    }
    if picks.contains(&0) {
        return Ok(chapters.to_vec());
    }
    Ok(picks
        .into_iter()
        .filter(|index| *index > 0)
        .map(|index| chapters[index - 1].clone())
        .collect())
}
