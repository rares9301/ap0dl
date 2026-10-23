use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use ap0dl_core::error::{Error, Result};

use crate::models::{Chapter, Manga, MangaStatus};

const VERSION: u32 = 1;

/// One followed series. Like Aniyomi's library entry, it remembers which
/// chapters were already seen so `update` can report only new ones, plus the
/// last chapter read for "continue reading".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub source: String,
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub status: MangaStatus,
    pub added_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<String>,
    /// Chapter ids seen at the last add/update.
    #[serde(default)]
    pub known_chapters: BTreeSet<String>,
    #[serde(default)]
    pub total_chapters: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_read: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_read_at: Option<String>,
}

impl LibraryEntry {
    pub fn new(manga: &Manga, chapters: &[Chapter]) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            source: manga.source.clone(),
            id: manga.id.clone(),
            title: manga.title.clone(),
            url: manga.url.clone(),
            thumbnail_url: manga.thumbnail_url.clone(),
            status: manga.status,
            added_at: now.clone(),
            last_checked: Some(now),
            known_chapters: chapters.iter().map(|chapter| chapter.id.clone()).collect(),
            total_chapters: chapters.len(),
            last_read: None,
            last_read_at: None,
        }
    }

    pub fn is(&self, source: &str, id: &str) -> bool {
        self.source == source && self.id == id
    }

    /// Records a fresh chapter list and returns the chapters not seen before.
    pub fn apply_update(&mut self, chapters: &[Chapter]) -> Vec<Chapter> {
        let new: Vec<Chapter> = chapters
            .iter()
            .filter(|chapter| !self.known_chapters.contains(&chapter.id))
            .cloned()
            .collect();
        self.known_chapters = chapters.iter().map(|chapter| chapter.id.clone()).collect();
        self.total_chapters = chapters.len();
        self.last_checked = Some(Utc::now().to_rfc3339());
        new
    }

    /// Index of the chapter after the last one read, for resuming.
    pub fn resume_index(&self, chapters: &[Chapter]) -> usize {
        match self.last_read {
            Some(last) => chapters
                .iter()
                .position(|chapter| chapter.number > last)
                .unwrap_or(chapters.len().saturating_sub(1)),
            None => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LibraryFile {
    version: u32,
    #[serde(default)]
    entries: Vec<LibraryEntry>,
}

/// JSON-backed library. Writes go to a temporary file that is renamed over
/// the old one, so a crash never leaves a half-written library.
#[derive(Debug, Clone)]
pub struct Library {
    path: PathBuf,
    entries: Vec<LibraryEntry>,
}

impl Library {
    pub fn load(path: &Path) -> Result<Self> {
        let entries = match std::fs::read_to_string(path) {
            Ok(text) => {
                let file: LibraryFile = serde_json::from_str(&text).map_err(|error| {
                    Error::config(format!("invalid manga library {}: {error}", path.display()))
                })?;
                file.entries
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(Error::Io(error)),
        };
        Ok(Self {
            path: path.to_path_buf(),
            entries,
        })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = LibraryFile {
            version: VERSION,
            entries: self.entries.clone(),
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

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }

    pub fn entries_mut(&mut self) -> &mut [LibraryEntry] {
        &mut self.entries
    }

    pub fn get(&self, source: &str, id: &str) -> Option<&LibraryEntry> {
        self.entries.iter().find(|entry| entry.is(source, id))
    }

    pub fn get_mut(&mut self, source: &str, id: &str) -> Option<&mut LibraryEntry> {
        self.entries.iter_mut().find(|entry| entry.is(source, id))
    }

    /// Adds or refreshes an entry. Returns false when it already existed.
    pub fn add(&mut self, manga: &Manga, chapters: &[Chapter]) -> bool {
        if let Some(entry) = self.get_mut(&manga.source, &manga.id) {
            entry.title = manga.title.clone();
            entry.url = manga.url.clone();
            entry.status = manga.status;
            entry.apply_update(chapters);
            return false;
        }
        self.entries.push(LibraryEntry::new(manga, chapters));
        self.entries
            .sort_by_key(|entry| entry.title.to_ascii_lowercase());
        true
    }

    /// Removes entries whose `source:id`, id or title matches `needle`
    /// (case-insensitive). Returns the removed entries.
    pub fn remove(&mut self, needle: &str) -> Vec<LibraryEntry> {
        let needle = needle.trim().to_ascii_lowercase();
        let (removed, kept): (Vec<_>, Vec<_>) = self.entries.drain(..).partition(|entry| {
            format!("{}:{}", entry.source, entry.id).to_ascii_lowercase() == needle
                || entry.id.to_ascii_lowercase() == needle
                || entry.title.to_ascii_lowercase() == needle
                || entry.url.to_ascii_lowercase() == needle
        });
        self.entries = kept;
        removed
    }

    /// Remembers the furthest chapter read. Returns false when the series is
    /// not in the library (reading outside the library is not tracked).
    pub fn mark_read(&mut self, source: &str, id: &str, number: f32) -> bool {
        let Some(entry) = self.get_mut(source, id) else {
            return false;
        };
        if number >= 0.0 && entry.last_read.is_none_or(|last| number > last) {
            entry.last_read = Some(number);
        }
        entry.last_read_at = Some(Utc::now().to_rfc3339());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(id: &str, number: f32) -> Chapter {
        Chapter {
            id: id.to_string(),
            url: String::new(),
            name: String::new(),
            number,
            volume: None,
            scanlator: None,
            date_upload: None,
            language: None,
            pages: None,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ap0dl-manga-test-{}-{name}", std::process::id()))
            .join("library.json")
    }

    #[test]
    fn add_update_read_and_remove_round_trip() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let manga = Manga::new(
            "mangadex",
            "abc",
            "Bocchi the Rock!",
            "https://mangadex.org/title/abc",
        );

        let mut library = Library::load(&path).unwrap();
        assert!(library.add(&manga, &[chapter("c1", 1.0), chapter("c2", 2.0)]));
        assert!(!library.add(&manga, &[chapter("c1", 1.0), chapter("c2", 2.0)]));
        assert!(library.mark_read("mangadex", "abc", 1.0));
        assert!(!library.mark_read("mangapill", "abc", 1.0));
        library.save().unwrap();

        let mut library = Library::load(&path).unwrap();
        let entry = library.get_mut("mangadex", "abc").unwrap();
        assert_eq!(entry.last_read, Some(1.0));
        let new = entry.apply_update(&[chapter("c1", 1.0), chapter("c2", 2.0), chapter("c3", 3.0)]);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].id, "c3");
        assert_eq!(entry.total_chapters, 3);
        let chapters = [chapter("c1", 1.0), chapter("c2", 2.0), chapter("c3", 3.0)];
        assert_eq!(entry.resume_index(&chapters), 1);

        assert_eq!(library.remove("MANGADEX:abc").len(), 1);
        assert!(library.entries().is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn mark_read_keeps_the_furthest_chapter() {
        let manga = Manga::new("mangadex", "abc", "x", "https://x.test");
        let mut library = Library {
            path: PathBuf::from("unused.json"),
            entries: Vec::new(),
        };
        library.add(&manga, &[]);
        library.mark_read("mangadex", "abc", 5.0);
        library.mark_read("mangadex", "abc", 3.0);
        assert_eq!(library.get("mangadex", "abc").unwrap().last_read, Some(5.0));
    }
}
