#!/usr/bin/env bash
# Prove the deprecated legacy providers have live-observed HTTP evidence.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
./target/debug/ap0dl providers --legacy --json >"$tmp"

jq -e '([.[] | .id] | sort) == ["allanime", "animepahe", "aniwave", "gogoanime"]
  and all(.[]; .status == "deprecated")
  and all(.[]; (.observed | length) > 0 and (.evidence | length) > 0)
  and all(.[]; (.evidence | test("403|401|Cloudflare|parked|abovedomains")))' \
  "$tmp" >/dev/null

for token in AnimePahe Gogoanime AllAnime AniWave "HTTP 403" "HTTP 401"; do
    grep -q "$token" docs/legacy-providers.md
done

echo "PASS: four legacy providers are deprecated with observed HTTP evidence"