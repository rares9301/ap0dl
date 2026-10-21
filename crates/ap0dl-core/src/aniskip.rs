use regex::Regex;
use serde_json::Value;

use crate::error::Result;
use crate::http::Http;
use crate::models::Chapter;

/// Optional AniSkip integration. Returns chapter ranges for OP/ED skipping.
pub async fn skip_times(
    http: &Http,
    mal_id: i64,
    episode: f32,
    episode_length: f64,
) -> Result<Vec<Chapter>> {
    if mal_id <= 0 {
        return Ok(Vec::new());
    }
    let episode = if episode.fract() == 0.0 {
        format!("{}", episode as i64)
    } else {
        format!("{episode}")
    };
    let url = format!(
        "https://api.aniskip.com/v2/skip-times/{mal_id}/{episode}?types[]=op&types[]=ed&episodeLength={episode_length}"
    );
    let value: Value = http
        .get_json(
            &url,
            &crate::http::headers(&[("Accept", "application/json")]),
        )
        .await?;
    let mut chapters = Vec::new();
    let Some(results) = value.get("results").and_then(Value::as_array) else {
        return Ok(chapters);
    };
    for result in results {
        let Some(interval) = result.get("interval") else {
            continue;
        };
        let start = interval.get("startTime").and_then(Value::as_f64);
        let end = interval.get("endTime").and_then(Value::as_f64);
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        if end <= start {
            continue;
        }
        let title = result
            .get("skipType")
            .and_then(Value::as_str)
            .unwrap_or("skip")
            .to_ascii_uppercase();
        chapters.push(Chapter { start, end, title });
    }
    Ok(chapters)
}

pub fn _regex_guard(value: &str) -> Option<String> {
    Regex::new(value).ok().map(|_| value.to_string())
}
