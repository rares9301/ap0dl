#!/usr/bin/env bash
# Live acceptance for the 2026-10-22 improvement goal:
# AniDB episode coverage, shared TUI, 1080p new sources, health-aware
# selection, automatic fallback, and the native Cloudflare fallback.
set -euo pipefail
cd "$(dirname "$0")/.."

bin=./target/debug/ap0dl
tmp="$(mktemp -d)"
health=/tmp/ap0dl-provider-health
cleanup() {
  rm -rf "$tmp"
  rm -f "$health/reanime" "$health/animeparadise"
}
trap cleanup EXIT

cargo build --workspace >/dev/null

echo "== AniDB episode coverage =="
"$bin" episodes 'anidb:cyberpunk 2' --json --log warn >"$tmp/anidb-episodes.json"
jq -e '.episodes | length >= 10' "$tmp/anidb-episodes.json" >/dev/null
"$bin" grab 'anidb:cyberpunk 2' --range 2 --quality best --log warn >"$tmp/anidb-stream.json"
jq -e '.streams | any(.quality == 720 and .kind == "direct")' "$tmp/anidb-stream.json" >/dev/null

echo "== AnimeParadise =="
"$bin" episodes 'animeparadise:cyberpunk edgerunners' --json --log warn >"$tmp/ap-episodes.json"
jq -e '.anime.provider == "animeparadise" and (.episodes | length >= 10)' "$tmp/ap-episodes.json" >/dev/null
"$bin" grab 'animeparadise:cyberpunk edgerunners' --range 1 --quality 1080 --log warn >"$tmp/ap-stream.json"
jq -e '.streams | any(.quality == 1080 and .kind == "hls")' "$tmp/ap-stream.json" >/dev/null

echo "== AnimeToki =="
"$bin" episodes 'animetoki:cyberpunk edgerunners' --json --log warn >"$tmp/toki-episodes.json"
jq -e '.anime.provider == "animetoki" and (.episodes | length >= 10)' "$tmp/toki-episodes.json" >/dev/null
"$bin" grab 'animetoki:cyberpunk edgerunners' --range 1 --quality 1080 --log warn >"$tmp/toki-stream.json"
jq -e '.streams | any(.quality == 1080 and .kind == "direct")' "$tmp/toki-stream.json" >/dev/null

echo "== Shared TUI surface =="
"$bin" stream --help | grep -q -- '--no-interactive'
"$bin" download --help | grep -q -- '--no-interactive'
"$bin" download --help | grep -q -- '--range'
"$bin" download --help | grep -q -- '--print-url'

echo "== Health-aware ordering =="
mkdir -p "$health"
date +%s >"$health/reanime"
url="$("$bin" download 'cyberpunk 2' --print-url --range 2 --no-interactive --log warn)"
rm -f "$health/reanime"
case "$url" in
  *my.1anime.site*) ;;
  *)
    echo "expected a healthy-provider URL after demoting reanime, got: $url" >&2
    exit 1
    ;;
esac

echo "== Automatic same-title fallback =="
if AP0DL_ANIMEPARADISE_STREAM='http://127.0.0.1:9' \
  timeout 90 "$bin" stream 'animeparadise:cyberpunk edgerunners' \
  --index 1 --range 1 --print-url --no-interactive --log warn \
  >"$tmp/fallback.out" 2>"$tmp/fallback.err"; then
  test -s "$tmp/fallback.out"
  grep -q 'Using ' "$tmp/fallback.err"
else
  echo "automatic fallback failed" >&2
  cat "$tmp/fallback.err" >&2
  exit 1
fi

echo "== Native Cloudflare/TLS fallback is wired =="
"$bin" providers --probe 'https://anihq.cc/search/?q=cyberpunk' >"$tmp/cf.txt"
cat "$tmp/cf.txt"

echo "PASS: AniDB 10 episodes, AnimeParadise + AnimeToki live 1080p, shared TUI, health ordering, fallback, Cloudflare probe"