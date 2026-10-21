#!/usr/bin/env bash
# Full acceptance harness for the ap0dl Rust rewrite goal.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace

./scripts/verify-quality.sh
./scripts/verify-claims.sh
./scripts/verify-legacy.sh
./scripts/verify-cli.sh
./scripts/verify-downloads.sh
./scripts/verify-docs.sh
./scripts/verify-checklist.sh
./scripts/verify-python-retention.sh

# Structural assertions for the typed async provider abstraction.
grep -q 'pub trait Provider' crates/ap0dl-core/src/provider.rs
grep -q 'async fn search' crates/ap0dl-core/src/provider.rs
for provider in reanime animegg anidb kisskh; do
    test -f "crates/ap0dl-core/src/providers/$provider.rs"
done

echo "PASS: Rust rewrite, 4 live providers, 1080p policy, legacy evidence, CLI, downloads, docs and checklist"