//! Local reading state: the chapter and page last shown for every series,
//! saved on each page turn so `read` can resume exactly where you stopped.
//!
//! Unlike the [`Library`](crate::library::Library), this tracks every
//! series ever opened, followed or not.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use ap0dl_core::error::{Error, Result};

use crate::models::{Chapter, Manga};
use crate::spread::PageLayout;

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingProgress {
    pub source: String,
    pub id: String,
    pub title: String,
    pub url: String,
    pub chapter_id: String,
    pub chapter_number: f32,
    pub chapter_name: String,
    /// Zero-based page of the chapter shown last.
    pub page: usize,
    /// Page count of that chapter, when known.
    #[serde(default)]
    pub pages: usize,
    /// Layout chosen in the viewer for this series, if changed from the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<PageLayout>,
    /// Show the first page of each chapter alone in double layout.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub offset: bool,
    pub updated_at: String,
}

impl ReadingProgress {
    pub fn new(manga: &Manga, chapter: &Chapter, page: usize, pages: usize) -> Self {
        Self {
            source: manga.source.clone(),
            id: manga.id.clone(),
            title: manga.title.clone(),
            url: manga.url.clone(),
            chapter_id: chapter.id.clone(),
            chapter_number: chapter.number,
            chapter_name: chapter.label(),
            page,
            pages,
            layout: None,
            offset: false,
            updated_at: Utc::now().to_rfc3339(),
        }
    }

    /// Index of the saved chapter in `chapters`: by id, then by number (the
    /// same series on another source has other ids), then the nearest number.
    pub fn chapter_index(&self, chapters: &[Chapter]) -> Option<usize> {
        if let Some(index) = chapters
            .iter()
            .position(|chapter| chapter.id == self.chapter_id)
        {
            return Some(index);
        }
        if self.chapter_number < 0.0 {
            return chapters
                .iter()
                .position(|chapter| chapter.label() == self.chapter_name);
        }
        nearest_chapter(chapters, self.chapter_number)
    }

    /// "Chapter 12, page 5/20".
    pub fn describe(&self) -> String {
        if self.pages > 0 {
            format!(
                "{}, page {}/{}",
                self.chapter_name,
                self.page + 1,
                self.pages
            )
        } else {
            format!("{}, page {}", self.chapter_name, self.page + 1)
        }
    }
}

/// Index of the chapter whose number is closest to `number` (exact matches
/// win; ties go to the lower chapter). Unnumbered chapters are ignored.
pub fn nearest_chapter(chapters: &[Chapter], number: f32) -> Option<usize> {
    chapters
        .iter()
        .enumerate()
        .filter(|(_, chapter)| chapter.number >= 0.0)
        .min_by(|(_, left), (_, right)| {
            let left = (left.number - number).abs();
            let right = (right.number - number).abs();
            left.total_cmp(&right)
        })
        .map(|(index, _)| index)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateFile {
    version: u32,
    #[serde(default)]
    series: Vec<ReadingProgress>,
}

/// JSON-backed reading state, written atomically like the library.
#[derive(Debug, Clone)]
pub struct ReadingState {
    path: PathBuf,
    series: Vec<ReadingProgress>,
}

impl ReadingState {
    pub fn load(path: &Path) -> Result<Self> {
        let series = match std::fs::read_to_string(path) {
            Ok(text) => {
                let file: StateFile = serde_json::from_str(&text).map_err(|error| {
                    Error::config(format!(
                        "invalid manga reading state {}: {error}",
                        path.display()
                    ))
                })?;
                file.series
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(Error::Io(error)),
        };
        Ok(Self {
            path: path.to_path_buf(),
            series,
        })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = StateFile {
            version: VERSION,
            series: self.series.clone(),
        };
        let text = serde_json::to_string_pretty(&file)?;
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, &self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn series(&self) -> &[ReadingProgress] {
        &self.series
    }

    /// Saved progress for `manga`: the same source entry, or else the most
    /// recent entry with the same title on any source.
    pub fn find(&self, manga: &Manga) -> Option<&ReadingProgress> {
        self.series
            .iter()
            .find(|entry| entry.source == manga.source && entry.id == manga.id)
            .or_else(|| {
                let title = normalize(&manga.title);
                self.series
                    .iter()
                    .filter(|entry| normalize(&entry.title) == title)
                    .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
            })
    }

    /// Replaces the entry for the same source and id, keeping the viewer
    /// preferences unless `progress` sets its own.
    pub fn record(&mut self, mut progress: ReadingProgress) {
        progress.updated_at = Utc::now().to_rfc3339();
        match self
            .series
            .iter_mut()
            .find(|entry| entry.source == progress.source && entry.id == progress.id)
        {
            Some(entry) => {
                if progress.layout.is_none() {
                    progress.layout = entry.layout;
                }
                *entry = progress;
            }
            None => self.series.push(progress),
        }
    }
}

fn normalize(title: &str) -> String {
    title
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(id: &str, number: f32) -> Chapter {
        Chapter {
            id: id.to_string(),
            url: format!("https://example.test/{id}"),
            name: format!("Chapter {number}"),
            number,
            volume: None,
            scanlator: None,
            date_upload: None,
            language: None,
            pages: None,
        }
    }

    fn manga(source: &str, id: &str, title: &str) -> Manga {
        Manga::new(source, id, title, format!("https://{source}.test/{id}"))
    }

    #[test]
    fn saves_and_reloads_progress() {
        let directory =
            std::env::temp_dir().join(format!("ap0dl-state-test-{}", std::process::id()));
        let path = directory.join("state.json");
        let mut state = ReadingState::load(&path).unwrap();
        assert!(state.series().is_empty());

        let bocchi = manga("mangadex", "b1", "Bocchi the Rock!");
        let mut progress = ReadingProgress::new(&bocchi, &chapter("c12", 12.0), 4, 20);
        progress.layout = Some(PageLayout::Single);
        state.record(progress);
        state.record(ReadingProgress::new(&bocchi, &chapter("c13", 13.0), 0, 18));
        state.save().unwrap();

        let state = ReadingState::load(&path).unwrap();
        let saved = state.find(&bocchi).unwrap();
        assert_eq!(state.series().len(), 1);
        assert_eq!(saved.chapter_id, "c13");
        assert_eq!(saved.layout, Some(PageLayout::Single));
        assert_eq!(saved.describe(), "Chapter 13, page 1/18");

        // The same title on another source resumes by chapter number.
        let other = manga("mangapill", "5573", "Bocchi the Rock");
        let saved = state.find(&other).unwrap();
        let chapters = [chapter("x", 12.0), chapter("y", 13.0), chapter("z", 14.0)];
        assert_eq!(saved.chapter_index(&chapters), Some(1));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nearest_chapter_prefers_exact_then_closest() {
        let chapters = [
            chapter("a", 1.0),
            chapter("b", 2.0),
            chapter("c", 2.5),
            chapter("d", 10.0),
        ];
        assert_eq!(nearest_chapter(&chapters, 2.5), Some(2));
        assert_eq!(nearest_chapter(&chapters, 7.0), Some(3));
        assert_eq!(nearest_chapter(&chapters, 0.0), Some(0));
        assert_eq!(nearest_chapter(&[chapter("x", -1.0)], 3.0), None);
    }
}
