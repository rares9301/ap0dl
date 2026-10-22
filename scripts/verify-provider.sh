#!/usr/bin/env bash
# Live acceptance for one provider: search -> details/episodes -> stream.
set -euo pipefail
cd "$(dirname "$0")/.."

provider="${1:?usage: verify-provider.sh <reanime|animegg|anidb|kisskh|animeparadise|animetoki>}"
case "$provider" in
  reanime)
    query='reanime:Bocchi the Rock'
    expected_quality=1080
    expected_kind=hls
    ;;
  animegg)
    query='animegg:Frieren'
    expected_quality=1080
    expected_kind=direct
    ;;
  anidb)
    query='anidb:Bocchi the Rock'
    expected_quality=1080
    expected_kind=hls
    ;;
  kisskh)
    query='kisskh:Bocchi the Rock'
    expected_quality=720
    expected_kind=hls
    ;;
  animeparadise)
    query='animeparadise:cyberpunk edgerunners'
    expected_quality=1080
    expected_kind=hls
    ;;
  animetoki)
    query='animetoki:cyberpunk edgerunners'
    expected_quality=1080
    expected_kind=direct
    ;;
  *)
    echo "unknown provider: $provider" >&2
    exit 2
    ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

./target/debug/ap0dl search "$query" --json --limit 3 >"$tmp/search.json"
./target/debug/ap0dl episodes "$query" --json >"$tmp/episodes.json"
./target/debug/ap0dl grab "$query" --range 1 --quality best --log warn >"$tmp/stream.json"

jq -e --arg provider "$provider" \
  'type == "array" and length > 0 and all(.[]; .provider == $provider)' \
  "$tmp/search.json" >/dev/null
jq -e --arg provider "$provider" \
  '.anime.provider == $provider and (.anime.title | length > 0)
   and (.episodes | type == "array") and (.episodes | length > 0)' \
  "$tmp/episodes.json" >/dev/null
jq -e --argjson quality "$expected_quality" --arg kind "$expected_kind" \
  '(.streams | type == "array") and (.streams | length > 0)
   and any(.streams[]; .quality == $quality and .kind == $kind)' \
  "$tmp/stream.json" >/dev/null

jq -c -n --arg provider "$provider" \
  --argjson quality "$expected_quality" --arg kind "$expected_kind" \
  --slurpfile search "$tmp/search.json" \
  --slurpfile episodes "$tmp/episodes.json" \
  --slurpfile stream "$tmp/stream.json" \
  '{provider: $provider,
    search_results: ($search[0] | length),
    selected_title: $episodes[0].anime.title,
    episode_count: ($episodes[0].episodes | length),
    streams: [$stream[0].streams[] | {quality, kind, title}],
    required_stream: {quality: $quality, kind: $kind}}'

echo "PASS: $provider live search/details/episodes/stream"