//! Weekly airing schedule backed by AniList's public GraphQL API.
//!
//! The UI sends the local Monday→Monday window. Rust does the paging and
//! normalisation so the schedule tab never depends on Kotlin-side scraping.

use chrono::{Datelike, Duration as ChronoDuration, Local, TimeZone};
use serde::Serialize;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::http::Http;

const ANILIST_URL: &str = "https://graphql.anilist.co";
const MAX_PAGES: usize = 4;

const QUERY: &str = r#"
query ($page: Int, $start: Int, $end: Int) {
  Page(page: $page, perPage: 50) {
    pageInfo { hasNextPage }
    airingSchedules(
      airingAt_greater: $start
      airingAt_lesser: $end
      sort: TIME
    ) {
      airingAt
      episode
      media {
        id
        title { romaji english native }
        coverImage { extraLarge large }
        siteUrl
        format
        status
        genres
      }
    }
  }
}
"#;

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleEntry {
    pub id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    pub episode: i64,
    pub airing_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWeek {
    pub start: i64,
    pub end: i64,
    pub entries: Vec<ScheduleEntry>,
}

/// Local Monday 00:00 → next Monday 00:00, in Unix seconds.
pub fn week_bounds() -> (i64, i64) {
    let now = Local::now();
    let date = now.date_naive();
    let monday = date - ChronoDuration::days(date.weekday().num_days_from_monday() as i64);
    let start = Local
        .from_local_datetime(&monday.and_hms_opt(0, 0, 0).expect("valid midnight"))
        .single()
        .map(|value| value.timestamp())
        .unwrap_or_else(|| now.timestamp() - 86_400 * 3);
    (start, start + 7 * 86_400)
}

pub async fn fetch(http: &Http, start: i64, end: i64) -> Result<ScheduleWeek> {
    let (default_start, default_end) = week_bounds();
    let (start, end) = if start > 0 && end > start {
        (start, end)
    } else {
        (default_start, default_end)
    };
    // Never let a malformed caller request a six-month query.
    let end = end.min(start + 9 * 86_400);

    let headers = crate::http::headers(&[
        ("Accept", "application/json"),
        ("Content-Type", "application/json"),
    ]);
    let mut entries = Vec::new();
    let mut pages = 0usize;

    for page in 1..=MAX_PAGES {
        let body = json!({
            "query": QUERY,
            "variables": { "page": page, "start": start, "end": end }
        });
        let payload: Value = http.post_json(ANILIST_URL, &headers, &body).await?;
        pages = page;
        let (mut page_entries, has_next) = parse_response(&payload)?;
        entries.append(&mut page_entries);
        if !has_next {
            break;
        }
    }
    if pages == 0 {
        return Err(Error::provider("anilist", "no schedule pages returned"));
    }

    entries.sort_by(|left, right| {
        left.airing_at
            .cmp(&right.airing_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    entries.dedup_by(|left, right| {
        left.id == right.id && left.episode == right.episode && left.airing_at == right.airing_at
    });

    Ok(ScheduleWeek {
        start,
        end,
        entries,
    })
}

fn parse_response(payload: &Value) -> Result<(Vec<ScheduleEntry>, bool)> {
    if let Some(errors) = payload.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            let message = errors
                .iter()
                .filter_map(|error| error.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(Error::provider(
                "anilist",
                if message.is_empty() {
                    "AniList returned an unknown error".to_string()
                } else {
                    message
                },
            ));
        }
    }
    let page = payload
        .pointer("/data/Page")
        .ok_or_else(|| Error::provider("anilist", "schedule response has no Page"))?;
    let has_next = page
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut entries = Vec::new();
    if let Some(schedules) = page.get("airingSchedules").and_then(Value::as_array) {
        for schedule in schedules {
            let Some(media) = schedule.get("media") else {
                continue;
            };
            let Some(id) = media.get("id").and_then(Value::as_i64) else {
                continue;
            };
            let Some(airing_at) = schedule.get("airingAt").and_then(Value::as_i64) else {
                continue;
            };
            let Some(title) = title_of(media) else {
                continue;
            };
            let episode = schedule.get("episode").and_then(Value::as_i64).unwrap_or(0);
            let cover = media
                .pointer("/coverImage/extraLarge")
                .and_then(Value::as_str)
                .or_else(|| media.pointer("/coverImage/large").and_then(Value::as_str))
                .map(str::to_string);
            let genres = media
                .get("genres")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            entries.push(ScheduleEntry {
                id,
                title,
                native_title: media
                    .pointer("/title/native")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                cover,
                episode,
                airing_at,
                format: media
                    .get("format")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                status: media
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                site_url: media
                    .get("siteUrl")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                genres,
            });
        }
    }
    Ok((entries, has_next))
}

fn title_of(media: &Value) -> Option<String> {
    ["english", "romaji", "native"].into_iter().find_map(|key| {
        media
            .pointer(&format!("/title/{key}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_airing_schedules() {
        let payload = json!({
            "data": {"Page": {
                "pageInfo": {"hasNextPage": true},
                "airingSchedules": [
                    {
                        "airingAt": 1790000000,
                        "episode": 4,
                        "media": {
                            "id": 42,
                            "title": {"romaji": "Romaji", "english": "English", "native": "Native"},
                            "coverImage": {"large": "https://example.test/a.jpg"},
                            "siteUrl": "https://anilist.co/anime/42",
                            "format": "TV",
                            "status": "RELEASING",
                            "genres": ["Action"]
                        }
                    },
                    {
                        "airingAt": 1790003600,
                        "episode": 2,
                        "media": {
                            "id": 43,
                            "title": {"romaji": "Second", "english": "", "native": null},
                            "coverImage": {"extraLarge": "https://example.test/b.jpg"}
                        }
                    }
                ]
            }}
        });
        let (entries, has_next) = parse_response(&payload).expect("schedule");
        assert!(has_next);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "English");
        assert_eq!(entries[0].episode, 4);
        assert_eq!(entries[1].title, "Second");
        assert_eq!(
            entries[1].cover.as_deref(),
            Some("https://example.test/b.jpg")
        );
    }

    #[test]
    fn surfaces_graphql_errors() {
        let payload = json!({"errors": [{"message": "rate limited"}]});
        assert!(parse_response(&payload).is_err());
    }
}
