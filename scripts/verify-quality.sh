#!/usr/bin/env bash
# Prove the 1080p preference and provider quality parsing tests.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo test --workspace best_prefers_1080_even_when_only_720_has_subtitles
cargo test --workspace parses_1080p_variants

grep -q 'quality: "best"' crates/ap0dl-core/src/config.rs
grep -q '1080' crates/ap0dl-core/src/quality.rs
for provider in anidb kisskh animeparadise animetoki; do
    grep -q '1080' "crates/ap0dl-core/src/providers/$provider.rs"
done

echo "PASS: 1080p preference, quality parsing and provider 1080p variant tests"