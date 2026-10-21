#!/usr/bin/env bash
# Prove the Python tree remains present and unmodified while the Rust replacement builds/tests.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --workspace
cargo test --workspace
test -d ap0dl
test -f pyproject.toml
git diff --quiet -- ap0dl pyproject.toml
test -z "$(git status --porcelain --untracked-files=no -- ap0dl pyproject.toml)"

echo "PASS: Python tree retained; Rust replacement builds and tests"