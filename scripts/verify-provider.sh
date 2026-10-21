#!/usr/bin/env bash
# Live acceptance for one provider: search -> details/episodes -> stream.
set -euo pipefail
cd "$(dirname "$0")/.."

provider="${1:?usage: verify-provider.sh <reanime|animegg|anidb|kisskh>}"
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

python3 - "$provider" "$expected_quality" "$expected_kind" "$tmp" <<'PY'
import json
import pathlib
import sys

provider, expected_quality, expected_kind, tmp = sys.argv[1], int(sys.argv[2]), sys.argv[3], pathlib.Path(sys.argv[4])
search = json.loads((tmp / "search.json").read_text())
details = json.loads((tmp / "episodes.json").read_text())
stream = json.loads((tmp / "stream.json").read_text())

assert isinstance(search, list) and search, "search returned no results"
assert all(item["provider"] == provider for item in search), "search returned another provider"
anime = details["anime"]
episodes = details["episodes"]
assert anime["provider"] == provider, "details returned another provider"
assert anime["title"], "details has no title"
assert isinstance(episodes, list) and episodes, "episode list is empty"
streams = stream["streams"]
assert isinstance(streams, list) and streams, "stream resolution returned no streams"
matching = [
    s for s in streams
    if s.get("quality") == expected_quality and s.get("kind") == expected_kind
]
assert matching, f"expected a {expected_quality}p {expected_kind} stream, got {[(s.get('quality'), s.get('kind')) for s in streams]}"

print(json.dumps({
    "provider": provider,
    "search_results": len(search),
    "selected_title": anime["title"],
    "episode_count": len(episodes),
    "streams": [{"quality": s.get("quality"), "kind": s.get("kind"), "title": s.get("title")} for s in streams],
    "required_stream": {"quality": expected_quality, "kind": expected_kind},
}, ensure_ascii=False))
PY