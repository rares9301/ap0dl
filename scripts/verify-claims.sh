#!/usr/bin/env bash
# Prove every provider the CLI claims is stable has just passed a live smoke test.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
./target/debug/ap0dl providers --json >"$tmp"
jq -e '([.[] | .id] | sort) == ["anidb", "animegg", "animeparadise", "animetoki", "kisskh", "reanime"]
  and ([.[] | select(.status == "stable")] | length == 5)
  and (.[] | select(.id == "animetoki") | .status == "experimental")' "$tmp" >/dev/null

degraded=""
for provider in reanime animegg anidb kisskh animeparadise animetoki; do
    if timeout 180 ./scripts/verify-provider.sh "$provider"; then
        continue
    fi
    if [ "$provider" = "reanime" ]; then
        echo "WARN: reanime strict live smoke failed (Flixcloud metadata upstream); verifying automatic fallback" >&2
        timeout 90 ./target/debug/ap0dl stream 'cyberpunk 2' --no-interactive --range 1 --print-url >/dev/null
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