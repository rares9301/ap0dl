use std::str::FromStr;

use ap0dl_core::ranges::EpisodeSelection;

use crate::models::Chapter;

/// Parsed chapter range for `--range`.
///
/// Numeric syntax is the anime episode syntax (`:`, `1`, `1-12`, `5-`,
/// `1-3,7`), so the two commands accept the same ranges. `latest`/`latest-N`
/// select the newest N chapters and may be combined with numbers. Ranges
/// match on the integer part, so `10` includes `10.5`, exactly like
/// episodes. Unnumbered chapters (oneshots) are only selected by `:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterSelection {
    numbers: Option<EpisodeSelection>,
    latest: Option<usize>,
}

impl Default for ChapterSelection {
    fn default() -> Self {
        Self::all()
    }
}

impl ChapterSelection {
    pub fn all() -> Self {
        Self {
            numbers: Some(EpisodeSelection::all()),
            latest: None,
        }
    }

    pub fn is_all(&self) -> bool {
        self.latest.is_none() && self.numbers.as_ref().is_some_and(EpisodeSelection::is_all)
    }

    /// Selects from `chapters` (any order) and returns them ascending.
    pub fn apply(&self, chapters: &[Chapter]) -> Vec<Chapter> {
        let mut sorted = chapters.to_vec();
        sort_chapters(&mut sorted);
        if self.is_all() {
            return sorted;
        }

        let numbered: Vec<&Chapter> = sorted.iter().filter(|c| c.number >= 0.0).collect();
        let latest_numbers: Vec<f32> = match self.latest {
            Some(count) => {
                let mut distinct: Vec<f32> = Vec::new();
                for chapter in numbered.iter().rev() {
                    if !distinct.contains(&chapter.number) {
                        distinct.push(chapter.number);
                    }
                    if distinct.len() == count {
                        break;
                    }
                }
                distinct
            }
            None => Vec::new(),
        };
        sorted
            .into_iter()
            .filter(|chapter| {
                latest_numbers.contains(&chapter.number)
                    || self
                        .numbers
                        .as_ref()
                        .is_some_and(|numbers| numbers.contains(chapter.number))
            })
            .collect()
    }
}

impl FromStr for ChapterSelection {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() || value == ":" || value.eq_ignore_ascii_case("all") {
            return Ok(Self::all());
        }
        let mut latest = None;
        let mut numeric = Vec::new();
        for token in value
            .split([',', ';', ' '])
            .filter(|t| !t.trim().is_empty())
        {
            let lower = token.trim().to_ascii_lowercase();
            if matches!(lower.as_str(), "latest" | "last" | "l") {
                latest = Some(latest.unwrap_or(0).max(1));
            } else if let Some(count) = lower
                .strip_prefix("latest-")
                .or_else(|| lower.strip_prefix("last-"))
            {
                let count = count
                    .parse::<usize>()
                    .map_err(|_| format!("invalid latest count in {token:?}"))?;
                latest = Some(count.max(1));
            } else {
                numeric.push(lower);
            }
        }
        let numbers = if numeric.is_empty() {
            None
        } else {
            Some(numeric.join(",").parse::<EpisodeSelection>()?)
        };
        Ok(Self { numbers, latest })
    }
}

/// Ascending by number; unnumbered chapters keep their source order at the
/// end, which is where Aniyomi lists extras.
pub fn sort_chapters(chapters: &mut [Chapter]) {
    chapters.sort_by(|a, b| match (a.number < 0.0, b.number < 0.0) {
        (false, false) => a.number.total_cmp(&b.number),
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (true, true) => std::cmp::Ordering::Equal,
    });
}

/// Collapses several uploads of the same chapter number (different
/// scanlation groups) into one, preferring `scanlator` when given. Reading
/// and downloading use this so `--range 5` fetches chapter 5 once.
pub fn dedupe_chapters(chapters: Vec<Chapter>, scanlator: Option<&str>) -> Vec<Chapter> {
    let preferred = |chapter: &Chapter| {
        scanlator.is_some_and(|wanted| {
            chapter.scanlator.as_deref().is_some_and(|group| {
                group
                    .to_ascii_lowercase()
                    .contains(&wanted.to_ascii_lowercase())
            })
        })
    };
    let mut out: Vec<Chapter> = Vec::with_capacity(chapters.len());
    for chapter in chapters {
        if chapter.number < 0.0 {
            out.push(chapter);
            continue;
        }
        match out
            .iter_mut()
            .find(|existing| existing.number == chapter.number)
        {
            Some(existing) => {
                if !preferred(existing) && preferred(&chapter) {
                    *existing = chapter;
                }
            }
            None => out.push(chapter),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(number: f32, group: &str) -> Chapter {
        Chapter {
            id: format!("{number}-{group}"),
            url: String::new(),
            name: String::new(),
            number,
            volume: None,
            scanlator: Some(group.to_string()),
            date_upload: None,
            language: None,
            pages: None,
        }
    }

    fn numbers(chapters: &[Chapter]) -> Vec<f32> {
        chapters.iter().map(|chapter| chapter.number).collect()
    }

    fn list() -> Vec<Chapter> {
        vec![
            chapter(3.0, "a"),
            chapter(-1.0, "a"),
            chapter(1.0, "a"),
            chapter(2.0, "a"),
            chapter(2.5, "a"),
            chapter(4.0, "a"),
        ]
    }

    #[test]
    fn all_keeps_everything_sorted_with_oneshots_last() {
        let selected: ChapterSelection = ":".parse().unwrap();
        assert_eq!(
            numbers(&selected.apply(&list())),
            [1.0, 2.0, 2.5, 3.0, 4.0, -1.0]
        );
    }

    #[test]
    fn numeric_ranges_match_integer_part() {
        let selected: ChapterSelection = "2-3".parse().unwrap();
        assert_eq!(numbers(&selected.apply(&list())), [2.0, 2.5, 3.0]);
        let selected: ChapterSelection = "1,4".parse().unwrap();
        assert_eq!(numbers(&selected.apply(&list())), [1.0, 4.0]);
    }

    #[test]
    fn latest_counts_distinct_numbers_and_combines_with_ranges() {
        let selected: ChapterSelection = "latest-2".parse().unwrap();
        assert_eq!(numbers(&selected.apply(&list())), [3.0, 4.0]);
        let selected: ChapterSelection = "1,latest".parse().unwrap();
        assert_eq!(numbers(&selected.apply(&list())), [1.0, 4.0]);
        assert!("latest-x".parse::<ChapterSelection>().is_err());
    }

    #[test]
    fn dedupe_prefers_requested_scanlator() {
        let chapters = vec![
            chapter(1.0, "First Group"),
            chapter(1.0, "Aoi Ichigo"),
            chapter(2.0, "First Group"),
            chapter(-1.0, "x"),
            chapter(-1.0, "y"),
        ];
        let deduped = dedupe_chapters(chapters.clone(), Some("aoi"));
        assert_eq!(numbers(&deduped), [1.0, 2.0, -1.0, -1.0]);
        assert_eq!(deduped[0].scanlator.as_deref(), Some("Aoi Ichigo"));
        let deduped = dedupe_chapters(chapters, None);
        assert_eq!(deduped[0].scanlator.as_deref(), Some("First Group"));
    }
}
