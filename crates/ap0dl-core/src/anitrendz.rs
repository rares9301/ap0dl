//! AniTrendz weekly chart.
//!
//! The site is a Next.js app; the chart is embedded in `__NEXT_DATA__` on
//! `/charts/top-anime`. We parse that payload instead of scraping rendered
//! HTML so ranks, movement and covers stay consistent with the live page.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::http::Http;

const CHART_URL: &str = "https://www.anitrendz.com/charts/top-anime";

#[derive(Debug, Clone, Serialize)]
pub struct TrendingEntry {
    pub rank: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub studio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub votes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weeks_on_chart: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendingChart {
    pub title: String,
    pub slug: String,
    pub week: String,
    pub season: String,
    pub date: String,
    pub votes: u64,
    pub entries: Vec<TrendingEntry>,
    pub fetched_from: String,
}

/// Fetch the current AniTrendz Top Anime chart. Cached for five minutes so
/// switching tabs does not hammer the site.
pub async fn fetch(http: &Http) -> Result<TrendingChart> {
    let headers = crate::http::headers(&[("Accept", "text/html,application/xhtml+xml")]);
    let html = http
        .get_text_cached(CHART_URL, &headers, std::time::Duration::from_secs(300))
        .await?;
    parse_page(&html)
}

pub fn parse_page(html: &str) -> Result<TrendingChart> {
    let marker = "id=\"__NEXT_DATA__\"";
    let marker_at = html
        .find(marker)
        .ok_or_else(|| Error::provider("anitrendz", "chart page has no __NEXT_DATA__ payload"))?;
    let script_open = html[..marker_at]
        .rfind("<script")
        .ok_or_else(|| Error::provider("anitrendz", "malformed __NEXT_DATA__ script"))?;
    let json_start = html[script_open..]
        .find('>')
        .map(|offset| script_open + offset + 1)
        .ok_or_else(|| Error::provider("anitrendz", "malformed __NEXT_DATA__ script"))?;
    let json_end = html[json_start..]
        .find("</script>")
        .map(|offset| json_start + offset)
        .ok_or_else(|| Error::provider("anitrendz", "unterminated __NEXT_DATA__ script"))?;
    let payload: Value = serde_json::from_str(&html[json_start..json_end])?;
    parse_payload(&payload)
}

fn parse_payload(payload: &Value) -> Result<TrendingChart> {
    let charts = payload
        .pointer("/props/pageProps/charts")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::provider("anitrendz", "chart payload has no charts array"))?;
    let chart = charts
        .iter()
        .find(|chart| chart.get("slug").and_then(Value::as_str) == Some("top-anime"))
        .or_else(|| charts.first())
        .ok_or_else(|| Error::provider("anitrendz", "chart payload is empty"))?;

    let mut entries = Vec::new();
    if let Some(choices) = chart.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let rank = int_field(choice, "position").unwrap_or(0) as u32;
            let Some(title) = string_field(choice, "name").filter(|value| !value.is_empty()) else {
                continue;
            };
            entries.push(TrendingEntry {
                rank,
                title,
                alt_title: string_field(choice, "nameAlt").filter(|value| !value.is_empty()),
                cover: choice
                    .pointer("/images/medium")
                    .and_then(Value::as_str)
                    .or_else(|| choice.pointer("/images/large").and_then(Value::as_str))
                    .or_else(|| choice.pointer("/images/small").and_then(Value::as_str))
                    .map(str::to_string),
                studio: string_field(choice, "subText").filter(|value| !value.is_empty()),
                tag: string_field(choice, "tag").filter(|value| !value.is_empty()),
                votes: int_field(choice, "total"),
                previous: int_field(choice, "previously").map(|value| value as u32),
                peak: int_field(choice, "peak").map(|value| value as u32),
                weeks_on_chart: int_field(choice, "weeksAtTopTen").map(|value| value as u32),
                uid: string_field(choice, "uid"),
            });
        }
    }
    entries.sort_by_key(|entry| entry.rank);

    Ok(TrendingChart {
        title: string_field(chart, "name").unwrap_or_else(|| "Top Anime".to_string()),
        slug: string_field(chart, "slug").unwrap_or_else(|| "top-anime".to_string()),
        week: string_field(chart, "week").unwrap_or_default(),
        season: string_field(chart, "season").unwrap_or_default(),
        date: string_field(chart, "date").unwrap_or_default(),
        votes: int_field(chart, "votes").unwrap_or(0),
        entries,
        fetched_from: CHART_URL.to_string(),
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn int_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_i64().map(|number| number.max(0) as u64))
            .or_else(|| value.as_f64().map(|number| number.max(0.0) as u64))
    })
}

/// Parse the same payload for callers that need rank lookup by title.
pub fn rank_by_title(chart: &TrendingChart) -> BTreeMap<String, u32> {
    chart
        .entries
        .iter()
        .map(|entry| (entry.title.to_ascii_lowercase(), entry.rank))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><body><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"charts":[{"name":"Top Anime","slug":"top-anime","week":"7","season":"fall","date":"2026-10-18","votes":790,"choices":[{"name":"Second Show","nameAlt":"Second","position":2,"total":244,"previously":3,"peak":1,"weeksAtTopTen":2,"subText":"Studio B","tag":"Fall 2026 Anime","uid":"b","images":{"medium":"https://example.test/b.jpg"}},{"name":"First Show","position":1,"total":298,"previously":1,"peak":1,"weeksAtTopTen":2,"subText":"Studio A","uid":"a","images":{"large":"https://example.test/a.jpg"}}]}]}}}</script></body></html>"#;

    #[test]
    fn parses_next_data_and_sorts_ranks() {
        let chart = parse_page(PAGE).expect("chart");
        assert_eq!(chart.slug, "top-anime");
        assert_eq!(chart.week, "7");
        assert_eq!(chart.entries.len(), 2);
        assert_eq!(chart.entries[0].title, "First Show");
        assert_eq!(
            chart.entries[0].cover.as_deref(),
            Some("https://example.test/a.jpg")
        );
        assert_eq!(chart.entries[1].rank, 2);
        assert_eq!(chart.entries[1].previous, Some(3));
    }

    #[test]
    fn rejects_pages_without_payload() {
        assert!(parse_page("<html>nope</html>").is_err());
    }
}
