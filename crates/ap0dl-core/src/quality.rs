use regex::Regex;
use std::sync::OnceLock;

use crate::models::{Audio, Stream};

/// IntelliQ-style quality selector.
///
/// Syntax (slash-separated fallbacks, plus-separated filters):
///
/// - `best`, `worst`
/// - `1080`, `720p`, `>=720`, `<=1080`, `=1080`
/// - `best[subtitles]`, `720[titled]`
/// - `[server=HD-1]`, `[title=r"^DUB"]`
/// - `sub`, `dub`, `raw`, `subtitles`, `titled`
///
/// The first slash-separated portion that produces candidates wins. If nothing
/// matches, the untouched stream list is returned so the caller can prompt or
/// fail gracefully.
pub fn select<'a>(streams: &'a [Stream], spec: &str) -> Vec<&'a Stream> {
    if streams.is_empty() {
        return Vec::new();
    }
    let spec = spec.trim();
    if spec.is_empty() {
        return vec![&streams[0]];
    }

    for portion in split_top_level(spec, '/') {
        let mut candidates: Vec<&Stream> = streams.iter().collect();
        for token in split_top_level(&portion, '+') {
            candidates = apply_token(&candidates, token.trim());
            if candidates.is_empty() {
                break;
            }
        }
        if !candidates.is_empty() {
            return candidates;
        }
    }

    streams.iter().collect()
}

fn apply_token<'a>(streams: &[&'a Stream], token: &str) -> Vec<&'a Stream> {
    if token.is_empty() {
        return streams.to_vec();
    }
    if let Some(open) = token.find('[') {
        let base = token[..open].trim();
        let bracket = token[open..]
            .trim()
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or_default()
            .trim();
        // Apply filters inside `[...]` before the base selector, so
        // `best[subtitles]` means "best of the subtitled streams" rather than
        // "highest stream, then hope it has subtitles". Attribute filters are
        // handled directly to avoid recursing into regex brackets such as
        // `[title=r"^\[Dub\]"]`.
        let filtered = if bracket.is_empty() {
            streams.to_vec()
        } else if let Some(attr) = apply_attribute(streams, bracket) {
            attr
        } else {
            apply_token(streams, bracket)
        };
        if base.is_empty() {
            return filtered;
        }
        let selected = apply_token(&filtered, base);
        return if selected.is_empty() {
            filtered
        } else {
            selected
        };
    }

    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "best" => return max_quality(streams),
        "worst" => return min_quality(streams),
        "subtitles" | "subs" => {
            return streams
                .iter()
                .copied()
                .filter(|s| !s.subtitles.is_empty())
                .collect();
        }
        "titled" => {
            return streams
                .iter()
                .copied()
                .filter(|s| s.title.as_deref().is_some_and(|t| !t.is_empty()))
                .collect();
        }
        "sub" => {
            return streams
                .iter()
                .copied()
                .filter(|s| matches!(s.audio, Audio::Sub | Audio::Both))
                .collect();
        }
        "dub" => {
            return streams
                .iter()
                .copied()
                .filter(|s| matches!(s.audio, Audio::Dub | Audio::Both))
                .collect();
        }
        "raw" => {
            return streams
                .iter()
                .copied()
                .filter(|s| s.audio == Audio::Raw)
                .collect();
        }
        _ => {}
    }

    if let Some(filtered) = apply_attribute(streams, token) {
        return filtered;
    }

    if let Some((comparator, quality)) = parse_quality_expression(token) {
        let mut matches = filter_quality(streams, comparator, quality);
        if matches.is_empty() && comparator.is_none() {
            // A bare number means "at most this quality" after an exact miss.
            matches = filter_quality(streams, Some("<="), quality);
            if matches.is_empty() {
                matches = filter_quality(streams, Some(">="), quality);
            }
        }
        return matches;
    }

    // Last resort: title/server substring, which makes specs like
    // `HD-1` or `English` useful without special syntax.
    let needle = token.to_ascii_lowercase();
    let matches: Vec<&Stream> = streams
        .iter()
        .copied()
        .filter(|stream| {
            stream
                .title
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains(&needle)
                || stream
                    .server
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&needle)
        })
        .collect();
    if matches.is_empty() {
        streams.to_vec()
    } else {
        matches
    }
}

fn apply_attribute<'a>(streams: &[&'a Stream], token: &str) -> Option<Vec<&'a Stream>> {
    let (key, value) = token.split_once('=')?;
    let key = key.trim().to_ascii_lowercase();
    let value = value.trim();
    if value.is_empty()
        || key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }

    let regex = if let Some(pattern) = value
        .strip_prefix("r\"")
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix("r'")
                .and_then(|value| value.strip_suffix('\''))
        }) {
        Regex::new(pattern).ok()
    } else {
        None
    };

    let matches: Vec<&Stream> = streams
        .iter()
        .copied()
        .filter(|stream| {
            let haystack = match key.as_str() {
                "title" => stream.title.as_deref(),
                "server" => stream.server.as_deref(),
                "url" | "stream_url" => Some(stream.url.as_str()),
                "audio" => Some(match stream.audio {
                    Audio::Sub => "sub",
                    Audio::Dub => "dub",
                    Audio::Both => "both",
                    Audio::Raw => "raw",
                }),
                "quality" => {
                    return stream
                        .quality
                        .is_some_and(|q| q.to_string() == value.trim_end_matches('p'));
                }
                _ => None,
            };
            let Some(haystack) = haystack else {
                return false;
            };
            match &regex {
                Some(regex) => regex.is_match(haystack),
                None => haystack.eq_ignore_ascii_case(value),
            }
        })
        .collect();

    Some(matches)
}

fn parse_quality_expression(token: &str) -> Option<(Option<&'static str>, u32)> {
    static EXPR: OnceLock<Regex> = OnceLock::new();
    let regex = EXPR
        .get_or_init(|| Regex::new(r"(?i)^(?:(>=|<=|>|<|==|=))?\s*(?:\d+x)?(\d{2,4})p?$").unwrap());
    let captures = regex.captures(token.trim())?;
    let comparator = captures.get(1).map(|m| match m.as_str() {
        ">=" => ">=",
        "<=" => "<=",
        ">" => ">",
        "<" => "<",
        _ => "=",
    });
    let quality = captures.get(2)?.as_str().parse().ok()?;
    Some((comparator, quality))
}

fn filter_quality<'a>(
    streams: &[&'a Stream],
    comparator: Option<&str>,
    quality: u32,
) -> Vec<&'a Stream> {
    let mut matches: Vec<&Stream> = streams
        .iter()
        .copied()
        .filter(|stream| {
            let Some(candidate) = stream.quality else {
                return false;
            };
            match comparator {
                Some(">=") => candidate >= quality,
                Some("<=") => candidate <= quality,
                Some(">") => candidate > quality,
                Some("<") => candidate < quality,
                Some("=") | None => candidate == quality,
                _ => candidate == quality,
            }
        })
        .collect();

    if matches.is_empty() {
        return matches;
    }
    if (comparator.is_none() || comparator == Some("="))
        && matches.iter().any(|s| s.quality == Some(quality))
    {
        matches.retain(|s| s.quality == Some(quality));
    }
    matches
}

fn max_quality<'a>(streams: &[&'a Stream]) -> Vec<&'a Stream> {
    let Some(max) = streams.iter().filter_map(|s| s.quality).max() else {
        return streams.to_vec();
    };
    streams
        .iter()
        .copied()
        .filter(|s| s.quality == Some(max))
        .collect()
}

fn min_quality<'a>(streams: &[&'a Stream]) -> Vec<&'a Stream> {
    let Some(min) = streams.iter().filter_map(|s| s.quality).min() else {
        return streams.to_vec();
    };
    streams
        .iter()
        .copied()
        .filter(|s| s.quality == Some(min))
        .collect()
}

/// Split on `separator` while ignoring separators inside brackets, braces,
/// parentheses and quoted strings.
fn split_top_level(value: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut stack: Vec<char> = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in value.chars() {
        if let Some(q) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '[' | '{' | '(' | '（' | '【' => {
                stack.push(ch);
                current.push(ch);
            }
            ']' | '}' | ')' | '）' | '】' => {
                stack.pop();
                current.push(ch);
            }
            _ if ch == separator && stack.is_empty() => {
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Audio, StreamKind};

    fn stream(quality: Option<u32>, audio: Audio, title: &str, subs: bool) -> Stream {
        Stream {
            url: format!("https://example.test/{quality:?}.m3u8"),
            container: None,
            quality,
            title: Some(title.to_string()),
            audio,
            server: None,
            subtitles: if subs {
                vec![crate::models::Subtitle {
                    url: "https://example.test/sub.vtt".into(),
                    lang: "English".into(),
                    format: Some("vtt".into()),
                    default: true,
                }]
            } else {
                Vec::new()
            },
            chapters: Vec::new(),
            headers: Default::default(),
            kind: StreamKind::Hls,
            flix: None,
        }
    }

    #[test]
    fn best_and_worst() {
        let streams = vec![
            stream(Some(480), Audio::Sub, "480", false),
            stream(Some(1080), Audio::Sub, "1080", false),
            stream(Some(720), Audio::Sub, "720", false),
        ];
        assert_eq!(select(&streams, "best")[0].quality, Some(1080));
        assert_eq!(select(&streams, "worst")[0].quality, Some(480));
    }

    #[test]
    fn exact_and_comparators() {
        let streams = vec![
            stream(Some(480), Audio::Sub, "480", false),
            stream(Some(720), Audio::Sub, "720", false),
            stream(Some(1080), Audio::Sub, "1080", false),
        ];
        assert_eq!(select(&streams, "720")[0].quality, Some(720));
        assert_eq!(select(&streams, ">=720").len(), 2);
        assert_eq!(select(&streams, "<=720").len(), 2);
        assert_eq!(select(&streams, "1080p")[0].quality, Some(1080));
    }

    #[test]
    fn falls_back_to_next_portion() {
        let streams = vec![
            stream(Some(1080), Audio::Sub, "nosubs", false),
            stream(Some(720), Audio::Dub, "dub", true),
        ];
        let selected = select(&streams, "best[subtitles]/best");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].quality, Some(720));
    }

    #[test]
    fn best_prefers_1080_even_when_only_720_has_subtitles() {
        let streams = vec![
            stream(Some(1080), Audio::Sub, "1080", false),
            stream(Some(720), Audio::Sub, "720", true),
        ];
        assert_eq!(select(&streams, "best")[0].quality, Some(1080));
    }

    #[test]
    fn attribute_and_audio_filters() {
        let streams = vec![
            stream(Some(1080), Audio::Sub, "[Sub] HD-1", true),
            stream(Some(1080), Audio::Dub, "[Dub] HD-1", true),
        ];
        let selected = select(&streams, "best[dub]");
        assert_eq!(selected[0].audio, Audio::Dub);

        // No fixture has a server, so the selector falls back to the full list.
        let selected = select(&streams, "[server=HD-1]");
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn title_regex() {
        let streams = vec![
            stream(Some(1080), Audio::Sub, "[Sub] HD-1", true),
            stream(Some(1080), Audio::Dub, "[Dub] HD-1", true),
        ];
        let selected = select(&streams, "[title=r\"^\\[Dub\\]\"]");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].audio, Audio::Dub);
    }
}
