#!/usr/bin/env bash
# Prove the required CLI subcommands and the four live provider registrations.
set -euo pipefail
cd "$(dirname "$0")/.."

help="$(./target/debug/ap0dl --help)"
for command in search episodes grab stream download providers; do
    grep -q "  $command " <<<"$help"
done
./target/debug/ap0dl config --help >/dev/null

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
./target/debug/ap0dl providers --json >"$tmp"
python3 - "$tmp" <<'PY'
import json
import pathlib
import sys

items = json.loads(pathlib.Path(sys.argv[1]).read_text())
ids = {item["id"] for item in items}
assert ids == {"reanime", "animegg", "anidb", "kisskh"}, ids
assert all(item["status"] == "stable" for item in items), items
PY

echo "PASS: CLI subcommands and four stable providers"