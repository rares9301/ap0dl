#!/usr/bin/env bash
# Prove the Rust-only README and provider/legacy documentation are complete.
set -euo pipefail
cd "$(dirname "$0")/.."

test -s readme.md
for heading in "## Build" "## Working providers" "## Legacy providers" "## Configuration" "## Architecture" "## Verification" "## Rust-only migration"; do
    grep -q "$heading" readme.md
done
for term in reanime animegg anidb kisskh animeparadise animetoki animepahe gogoanime allanime aniwave 1080 Aniyomi; do
    grep -qi "$term" readme.md
done
grep -q "docs/legacy-providers.md" readme.md
grep -q "verify-improvements.sh" readme.md
! grep -qi "Python package remains\|Python code is intentionally still present" readme.md
test -s docs/legacy-providers.md
grep -q "HTTP 403" docs/legacy-providers.md
grep -q "HTTP 401" docs/legacy-providers.md
grep -qi "Aniyomi" docs/goal-checklist.md

echo "PASS: Rust-only README, provider matrix and legacy evidence documentation"