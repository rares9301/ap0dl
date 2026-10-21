#!/usr/bin/env bash
# Prove the README and legacy documentation cover the requested migration and provider matrix.
set -euo pipefail
cd "$(dirname "$0")/.."

test -s readme.md
for heading in "## Build" "## Working providers" "## Legacy providers" "## Configuration" "## Architecture" "## Verification" "## Migration from Python"; do
    grep -q "$heading" readme.md
done
for term in reanime animegg anidb kisskh animepahe gogoanime allanime aniwave 1080 Aniyomi; do
    grep -qi "$term" readme.md
done
grep -q "docs/legacy-providers.md" readme.md
test -s docs/legacy-providers.md
grep -q "HTTP 403" docs/legacy-providers.md
grep -q "HTTP 401" docs/legacy-providers.md
grep -qi "Aniyomi" docs/goal-checklist.md

echo "PASS: README migration/provider matrix and legacy evidence documentation"