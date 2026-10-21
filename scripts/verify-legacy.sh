#!/usr/bin/env bash
# Prove the deprecated legacy providers have live-observed HTTP evidence.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
./target/debug/ap0dl providers --legacy --json >"$tmp"

python3 - "$tmp" <<'PY'
import json
import pathlib
import sys

items = json.loads(pathlib.Path(sys.argv[1]).read_text())
expected = {"animepahe", "gogoanime", "allanime", "aniwave"}
ids = {item["id"] for item in items}
assert ids == expected, ids
for item in items:
    assert item["status"] == "deprecated", item
    assert item["observed"], item
    evidence = item["evidence"]
    assert evidence, item
    assert any(token in evidence for token in ("403", "401", "Cloudflare", "parked", "abovedomains")), item

doc = pathlib.Path("docs/legacy-providers.md").read_text()
for token in ("AnimePahe", "Gogoanime", "AllAnime", "AniWave", "HTTP 403", "HTTP 401"):
    assert token in doc, token
PY

echo "PASS: four legacy providers are deprecated with observed HTTP evidence"