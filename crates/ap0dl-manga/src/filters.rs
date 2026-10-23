use std::collections::BTreeMap;

use serde::Serialize;

use ap0dl_core::error::{Error, Result};

/// One selectable value of a [`Filter`]. `value` is what the source sends to
/// the site; `label` is what the user sees.
#[derive(Debug, Clone, Serialize)]
pub struct FilterOption {
    pub value: &'static str,
    pub label: &'static str,
}

pub const fn option(value: &'static str, label: &'static str) -> FilterOption {
    FilterOption { value, label }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FilterKind {
    /// Exactly one value (Aniyomi `Filter.Select` / `Filter.Sort`).
    Select { options: Vec<FilterOption> },
    /// Any number of values (Aniyomi `Filter.Group` of check boxes).
    Multi { options: Vec<FilterOption> },
    /// Free text (Aniyomi `Filter.Text`).
    Text,
}

/// A search filter a source declares (Aniyomi's `getFilterList()` entries).
/// Filters are data, so the CLI can list and validate them without knowing
/// anything about the site.
#[derive(Debug, Clone, Serialize)]
pub struct Filter {
    pub key: &'static str,
    pub name: &'static str,
    #[serde(flatten)]
    pub kind: FilterKind,
}

impl Filter {
    pub fn select(key: &'static str, name: &'static str, options: Vec<FilterOption>) -> Self {
        Self {
            key,
            name,
            kind: FilterKind::Select { options },
        }
    }

    pub fn multi(key: &'static str, name: &'static str, options: Vec<FilterOption>) -> Self {
        Self {
            key,
            name,
            kind: FilterKind::Multi { options },
        }
    }

    pub fn text(key: &'static str, name: &'static str) -> Self {
        Self {
            key,
            name,
            kind: FilterKind::Text,
        }
    }

    fn options(&self) -> &[FilterOption] {
        match &self.kind {
            FilterKind::Select { options } | FilterKind::Multi { options } => options,
            FilterKind::Text => &[],
        }
    }
}

/// User-chosen filter values, keyed by [`Filter::key`]. Built from
/// `--filter key=value[,value]` arguments and validated against a source's
/// declared filters before the source sees them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FilterValues(BTreeMap<String, Vec<String>>);

impl FilterValues {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Parses `key=value` pairs. Repeated keys and comma-separated values
    /// accumulate, so `genre=Action,Comedy` equals two `genre=` arguments.
    pub fn parse(pairs: &[String]) -> Result<Self> {
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for pair in pairs {
            let (key, value) = pair.split_once('=').ok_or_else(|| {
                Error::config(format!("invalid filter {pair:?}; expected key=value"))
            })?;
            let key = key.trim().to_ascii_lowercase();
            if key.is_empty() {
                return Err(Error::config(format!("invalid filter {pair:?}; empty key")));
            }
            let entry = values.entry(key).or_default();
            entry.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            );
        }
        Ok(Self(values))
    }

    /// Checks keys and option values against `filters`, normalising option
    /// values to the exact spelling the source declared (matching is
    /// case-insensitive on both value and label).
    pub fn validate(&self, source: &str, filters: &[Filter]) -> Result<Self> {
        let mut normalized = BTreeMap::new();
        for (key, values) in &self.0 {
            let filter = filters
                .iter()
                .find(|filter| filter.key == key)
                .ok_or_else(|| {
                    let known = filters
                        .iter()
                        .map(|filter| filter.key)
                        .collect::<Vec<_>>()
                        .join(", ");
                    Error::config(format!(
                        "source {source} has no filter {key:?} (available: {known})"
                    ))
                })?;
            let mut accepted = Vec::new();
            for value in values {
                if matches!(filter.kind, FilterKind::Text) {
                    accepted.push(value.clone());
                    continue;
                }
                let option = filter
                    .options()
                    .iter()
                    .find(|option| {
                        option.value.eq_ignore_ascii_case(value)
                            || option.label.eq_ignore_ascii_case(value)
                    })
                    .ok_or_else(|| {
                        Error::config(format!(
                            "filter {key} on {source} has no option {value:?}; run `ap0dl manga sources --filters {source}`"
                        ))
                    })?;
                accepted.push(option.value.to_string());
            }
            if matches!(filter.kind, FilterKind::Select { .. }) && accepted.len() > 1 {
                return Err(Error::config(format!(
                    "filter {key} on {source} takes one value, got {}",
                    accepted.len()
                )));
            }
            normalized.insert(key.clone(), accepted);
        }
        Ok(Self(normalized))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .get(key)
            .and_then(|values| values.first())
            .map(String::as_str)
    }

    pub fn all(&self, key: &str) -> &[String] {
        self.0.get(key).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filters() -> Vec<Filter> {
        vec![
            Filter::select(
                "sort",
                "Sort",
                vec![
                    option("relevance", "Relevance"),
                    option("popular", "Popular"),
                ],
            ),
            Filter::multi(
                "genre",
                "Genres",
                vec![option("action", "Action"), option("comedy", "Comedy")],
            ),
            Filter::text("author", "Author"),
        ]
    }

    #[test]
    fn parses_and_normalizes_values() {
        let values = FilterValues::parse(&[
            "sort=Popular".to_string(),
            "genre=Action,comedy".to_string(),
            "author=Hamaji Aki".to_string(),
        ])
        .unwrap()
        .validate("test", &filters())
        .unwrap();
        assert_eq!(values.get("sort"), Some("popular"));
        assert_eq!(values.all("genre"), ["action", "comedy"]);
        assert_eq!(values.get("author"), Some("Hamaji Aki"));
    }

    #[test]
    fn rejects_unknown_keys_values_and_multi_select() {
        let unknown_key = FilterValues::parse(&["year=2020".to_string()]).unwrap();
        assert!(unknown_key.validate("test", &filters()).is_err());
        let unknown_value = FilterValues::parse(&["genre=horror".to_string()]).unwrap();
        assert!(unknown_value.validate("test", &filters()).is_err());
        let two_sorts = FilterValues::parse(&["sort=popular,relevance".to_string()]).unwrap();
        assert!(two_sorts.validate("test", &filters()).is_err());
        assert!(FilterValues::parse(&["novalue".to_string()]).is_err());
    }
}
