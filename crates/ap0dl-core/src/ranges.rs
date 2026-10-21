use std::str::FromStr;

use crate::models::Episode;

/// Parsed episode range, compatible with the old `-r/--range` syntax.
///
/// Examples: `:`, `1`, `1-12`, `12-1`, `1-3,7,9-`, `latest`, `latest-2`,
/// `latest-all`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeSelection {
    ranges: Vec<(u32, u32)>,
    latest: Option<usize>,
    latest_all: bool,
}

impl Default for EpisodeSelection {
    fn default() -> Self {
        Self::all()
    }
}

impl EpisodeSelection {
    pub fn all() -> Self {
        Self {
            ranges: vec![(0, u32::MAX)],
            latest: None,
            latest_all: false,
        }
    }

    pub fn is_all(&self) -> bool {
        self.ranges.len() == 1 && self.ranges[0] == (0, u32::MAX) && self.latest.is_none()
    }

    pub fn contains(&self, number: f32) -> bool {
        if !number.is_finite() || number < 0.0 {
            return false;
        }
        let n = number.floor() as u32;
        self.ranges
            .iter()
            .any(|(start, end)| n >= *start && n <= *end)
    }

    /// Applies the range and the `latest` rotation to an episode list.
    pub fn apply(&self, episodes: &[Episode]) -> Vec<Episode> {
        let mut selected: Vec<Episode> = episodes
            .iter()
            .filter(|episode| self.contains(episode.number))
            .cloned()
            .collect();
        selected.sort_by(|a, b| a.number.total_cmp(&b.number));

        if let Some(count) = self.latest {
            if !selected.is_empty() {
                let count = count.min(selected.len());
                let split = selected.len() - count;
                let mut rotated = selected[split..].to_vec();
                rotated.extend_from_slice(&selected[..split]);
                selected = rotated;
            }
        } else if self.latest_all && !selected.is_empty() {
            selected.rotate_right(1);
        }

        selected
    }
}

impl FromStr for EpisodeSelection {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() || value == ":" || value == "all" {
            return Ok(Self::all());
        }

        let mut ranges = Vec::new();
        let mut latest = None;
        let mut latest_all = false;

        for raw in value
            .split([',', ';', ' '])
            .filter(|part| !part.trim().is_empty())
        {
            let token = raw.trim().to_ascii_lowercase();
            match token.as_str() {
                "latest" | "last" | "l" => {
                    latest = Some(latest.unwrap_or(0).max(1));
                    continue;
                }
                "latest-all" | "latest*" | "last*" | "l*" => {
                    latest = Some(latest.unwrap_or(0).max(1));
                    latest_all = true;
                    continue;
                }
                _ => {}
            }

            if let Some(rest) = token
                .strip_prefix("latest-")
                .or_else(|| token.strip_prefix("last-"))
                .or_else(|| token.strip_prefix("l-"))
            {
                let count = rest
                    .parse::<usize>()
                    .map_err(|_| format!("invalid latest count in {raw:?}"))?;
                latest = Some(count.max(1));
                continue;
            }

            if let Some((start, end)) = token.split_once('-') {
                let start = parse_bound(start, raw)?;
                let end = if end.trim().is_empty() {
                    u32::MAX
                } else {
                    parse_bound(end, raw)?
                };
                let (start, end) = if start <= end {
                    (start, end)
                } else {
                    (end, start)
                };
                ranges.push((start, end));
            } else {
                let number = token
                    .parse::<u32>()
                    .map_err(|_| format!("invalid episode range token {raw:?}"))?;
                ranges.push((number, number));
            }
        }

        if ranges.is_empty() {
            ranges.push((0, u32::MAX));
        }

        Ok(Self {
            ranges,
            latest,
            latest_all,
        })
    }
}

fn parse_bound(value: &str, raw: &str) -> Result<u32, String> {
    if value.trim().is_empty() {
        return Ok(0);
    }
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("invalid episode range token {raw:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Audio, Episode};

    fn episode(number: f32) -> Episode {
        Episode {
            number,
            title: None,
            id: number.to_string(),
            url: String::new(),
            filler: false,
            recap: false,
            aired: None,
            audio: Audio::Sub,
        }
    }

    #[test]
    fn parses_all() {
        let selection: EpisodeSelection = ":".parse().unwrap();
        assert!(selection.is_all());
        assert!(selection.contains(0.0));
        assert!(selection.contains(1234.0));
    }

    #[test]
    fn parses_single_and_ranges() {
        let selection: EpisodeSelection = "1,3-5,12-9".parse().unwrap();
        assert!(selection.contains(1.0));
        assert!(!selection.contains(2.0));
        assert!(selection.contains(3.0));
        assert!(selection.contains(5.0));
        assert!(selection.contains(9.0));
        assert!(selection.contains(12.0));
        assert!(!selection.contains(13.0));
    }

    #[test]
    fn open_ended_ranges() {
        let selection: EpisodeSelection = "3-, -2".parse().unwrap();
        assert!(selection.contains(1.0));
        assert!(selection.contains(2.0));
        assert!(selection.contains(3.0));
        assert!(selection.contains(99.0));
    }

    #[test]
    fn latest_rotates() {
        let episodes: Vec<_> = (1..=5).map(|n| episode(n as f32)).collect();
        let selection: EpisodeSelection = "latest-2".parse().unwrap();
        let selected = selection.apply(&episodes);
        assert_eq!(
            selected.iter().map(|e| e.number as u32).collect::<Vec<_>>(),
            vec![4, 5, 1, 2, 3]
        );
    }

    #[test]
    fn latest_all_rotates_one() {
        let episodes: Vec<_> = (1..=3).map(|n| episode(n as f32)).collect();
        let selection: EpisodeSelection = "latest-all".parse().unwrap();
        let selected = selection.apply(&episodes);
        assert_eq!(
            selected.iter().map(|e| e.number as u32).collect::<Vec<_>>(),
            vec![3, 1, 2]
        );
    }
}
