#!/usr/bin/env bash
# Prove every provider the CLI claims is stable has just passed a live smoke test.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
./target/debug/ap0dl providers --json >"$tmp"
python3 - "$tmp" <<'PY'
import json
import pathlib
import sys

items = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert {item["id"] for item in items} == {"reanime", "animegg", "anidb", "kisskh"}
assert all(item["status"] == "stable" for item in items)
PY

for provider in reanime animegg anidb kisskh; do
    ./scripts/verify-provider.sh "$provider"
done

echo "PASS: every stable provider passed a live search/details/episodes/stream smoke"