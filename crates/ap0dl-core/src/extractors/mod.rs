//! Site-specific extractors that do not fit the generic provider flow.
//!
//! Flixcloud lives in [`crate::proxy`], while AnimeGG's inline JSON is handled
//! by its provider. This module is intentionally small; keeping extraction
//! helpers here makes it obvious where new hosts should be added.
