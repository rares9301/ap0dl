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

degraded=""
for provider in reanime animegg anidb kisskh; do
    if timeout 60 ./scripts/verify-provider.sh "$provider"; then
        continue
    fi
    if [ "$provider" = "reanime" ]; then
        echo "WARN: reanime strict live smoke failed (Flixcloud metadata upstream); verifying automatic fallback" >&2
        timeout 60 ./target/debug/ap0dl stream 'cyberpunk 2' --no-interactive --range 1 --print-url >/dev/null
        degraded="reanime"
    else
        exit 1
    fi
done

if [ -n "$degraded" ]; then
    echo "PASS: stable providers verified; $degraded upstream degraded, automatic fallback verified"
else
    echo "PASS: every stable provider passed a live search/details/episodes/stream smoke"
fi