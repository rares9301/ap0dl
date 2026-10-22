#!/usr/bin/env bash
# Prove the required CLI subcommands and the six live provider registrations.
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
jq -e '([.[] | .id] | sort) == ["anidb", "animegg", "animeparadise", "animetoki", "kisskh", "reanime"]
  and all(.[]; .status == "stable")' "$tmp" >/dev/null

echo "PASS: CLI subcommands and six stable providers"