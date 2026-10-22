#!/usr/bin/env bash
# Full acceptance harness for the Rust-only ap0dl engine.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace

./scripts/verify-quality.sh
./scripts/verify-selection.sh
./scripts/verify-legacy.sh
./scripts/verify-cli.sh
./scripts/verify-docs.sh
./scripts/verify-checklist.sh
./scripts/verify-claims.sh
./scripts/verify-improvements.sh

# Structural assertions for the typed async provider abstraction.
grep -q 'pub trait Provider' crates/ap0dl-core/src/provider.rs
grep -q 'async fn search' crates/ap0dl-core/src/provider.rs
for provider in reanime animegg anidb kisskh animeparadise animetoki; do
    test -f "crates/ap0dl-core/src/providers/$provider.rs"
done
test -z "$(find . -path ./.git -prune -o -path ./target -prune -o -name '*.py' -print -quit)"

echo "PASS: Rust-only engine, six live provider flows, 1080p policy, shared TUI, fallback, downloads, docs and checklist"