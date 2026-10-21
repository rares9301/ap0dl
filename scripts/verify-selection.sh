#!/usr/bin/env bash
# Regression check for the reported selection bug: "cyberpunk 2" must select
# Cyberpunk: Edgerunners 2, not an unrelated short title such as PLUTO.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

./target/debug/ap0dl search 'cyberpunk 2' --json --limit 5 >"$tmp/search.json"
python3 - "$tmp/search.json" <<'PY'
import json
import pathlib
import sys

items = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert items, "no search results"
assert items[0]["title"] == "Cyberpunk: Edgerunners 2", items[0]
assert any(item["title"] == "Cyberpunk: Edgerunners 2" for item in items)
assert all(item["title"] != "PLUTO" for item in items[:3])
PY

./target/debug/ap0dl stream 'cyberpunk 2' --no-interactive --range 1 --print-url \
    >"$tmp/url.txt"
grep -q '^https://' "$tmp/url.txt"

if command -v script >/dev/null 2>&1; then
    printf '\r\r' | TERM=xterm-256color script -qec \
        "./target/debug/ap0dl stream 'cyberpunk 2' --print-url --log warn" /dev/null \
        >"$tmp/interactive.txt"
    grep -q 'https://' "$tmp/interactive.txt"
fi

echo "PASS: fuzzy search and interactive selection choose Cyberpunk: Edgerunners 2"