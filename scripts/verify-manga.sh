#!/usr/bin/env bash
# Live acceptance for the manga plugin (`--features ap0dl-cli/manga`):
# per source search -> details/chapters -> pages -> one CBZ download, then the
# shared TUI checklist, library tracking and same-title source fallback.
#
#   ./scripts/verify-manga.sh                 # every source + plugin checks
#   ./scripts/verify-manga.sh mangapill       # one source only
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --workspace --features ap0dl-cli/manga >/dev/null
bin=./target/debug/ap0dl

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
export AP0DL_MANGA_LIBRARY="$tmp/library.json"
export AP0DL_MANGA_STATE="$tmp/state.json"

verify_source() {
  local source="$1" query="$2" min_chapters="$3"
  "$bin" manga search "$source:$query" --json --limit 3 >"$tmp/$source-search.json"
  "$bin" manga chapters "$source:$query" --json >"$tmp/$source-chapters.json"
  "$bin" manga pages "$source:$query" --range 1 >"$tmp/$source-pages.json"
  "$bin" manga download "$source:$query" --range 1 --no-interactive --json \
    -d "$tmp/$source-dl" >"$tmp/$source-download.json"

  jq -e --arg source "$source" \
    'type == "array" and length > 0 and all(.[]; .source == $source)' \
    "$tmp/$source-search.json" >/dev/null
  jq -e --arg source "$source" --argjson min "$min_chapters" \
    '.source == $source and .manga.initialized and (.manga.title | length > 0)
     and (.chapters | length >= $min)' \
    "$tmp/$source-chapters.json" >/dev/null
  jq -e '(.pages | length > 0) and all(.pages[]; .image_url | startswith("https://"))' \
    "$tmp/$source-pages.json" >/dev/null
  jq -e 'length == 1 and .[0].pages > 0 and .[0].bytes > 100000
     and (.[0].path | endswith(".cbz"))' "$tmp/$source-download.json" >/dev/null

  local cbz
  cbz="$(jq -r '.[0].path' "$tmp/$source-download.json")"
  if command -v unzip >/dev/null 2>&1; then
    unzip -tq "$cbz" >/dev/null
    unzip -l "$cbz" | grep -q 'ComicInfo.xml'
  fi

  jq -c -n --arg source "$source" \
    --slurpfile chapters "$tmp/$source-chapters.json" \
    --slurpfile pages "$tmp/$source-pages.json" \
    --slurpfile download "$tmp/$source-download.json" \
    '{source: $source,
      title: $chapters[0].manga.title,
      chapter_count: ($chapters[0].chapters | length),
      first_chapter_pages: ($pages[0].pages | length),
      cbz_bytes: $download[0][0].bytes}'
  echo "PASS: $source live search/details/chapters/pages/CBZ"
}

run_source() {
  case "$1" in
    mangadex) verify_source mangadex 'bocchi the rock' 100 ;;
    mangapill) verify_source mangapill 'bocchi the rock' 90 ;;
    weebcentral) verify_source weebcentral 'bocchi the rock' 90 ;;
    *)
      echo "unknown manga source: $1" >&2
      exit 2
      ;;
  esac
}

if [ "$#" -gt 0 ]; then
  for source in "$@"; do run_source "$source"; done
  exit 0
fi

"$bin" manga sources --json >"$tmp/sources.json"
jq -e '([.[] | .id] | sort) == ["mangadex", "mangapill", "weebcentral"]
  and all(.[]; .status == "stable")' "$tmp/sources.json" >/dev/null

for source in mangadex mangapill weebcentral; do
  run_source "$source"
done

echo "== Merged search ranks the series on every source first =="
"$bin" manga search 'bocchi the rock' --json --limit 3 >"$tmp/merged.json"
jq -e 'length == 3 and all(.[]; .title == "Bocchi the Rock!")
  and ([.[].source] | sort) == ["mangadex", "mangapill", "weebcentral"]' \
  "$tmp/merged.json" >/dev/null

echo "== Download checklist TUI =="
printf ' \r' | TERM=xterm-256color timeout 180 script -qec \
  "$bin manga download 'weebcentral:bocchi the rock' --index 1 --print-url --log warn" /dev/null \
  >"$tmp/download-tui.txt"
grep -q 'DOWNLOAD ALL CHAPTERS' "$tmp/download-tui.txt"
test "$(grep -c '^https://' "$tmp/download-tui.txt")" -ge 500

echo "== Chapter prompt, saved reading state and viewer menu TUI =="
mkdir -p "$tmp/bin"
printf '#!/bin/sh\nexit 0\n' >"$tmp/bin/mpv"
chmod +x "$tmp/bin/mpv"
read_tui() {
  printf "$1" | PATH="$tmp/bin:$PATH" TERM=xterm-256color timeout 180 script -qec \
    "$bin manga read 'mangapill:bocchi the rock' --index 1 --reader mpv --log warn" /dev/null \
    >"$tmp/read-tui.txt"
}
# Typed chapter, then Quit (the menu's default) after the viewer closes.
read_tui '3\r\r'
grep -q 'What chapter do you want to read' "$tmp/read-tui.txt"
grep -q 'Reading Bocchi the Rock! from Chapter 3' "$tmp/read-tui.txt"
grep -q 'Viewer closed' "$tmp/read-tui.txt"
jq -e '.series | length == 1 and .[0].chapter_name == "Chapter 3"' "$AP0DL_MANGA_STATE" >/dev/null
# Enter with nothing typed resumes the saved chapter.
read_tui '\r\r'
grep -q 'Saved: Chapter 3' "$tmp/read-tui.txt"
grep -q 'Reading Bocchi the Rock! from Chapter 3' "$tmp/read-tui.txt"

echo "== Continuous two-page mpv viewer (offline) =="
if command -v mpv >/dev/null 2>&1 && command -v ffmpeg >/dev/null 2>&1; then
  cargo test -q -p ap0dl-manga mpv_session -- --ignored >"$tmp/viewer-test.txt" 2>&1
  grep -q 'test result: ok. 1 passed' "$tmp/viewer-test.txt"
else
  echo "SKIP: mpv and ffmpeg are needed for the viewer session test"
fi

echo "== Library tracking =="
"$bin" manga library add 'mangadex:bocchi the rock' --index 1 --no-interactive
"$bin" manga read 'mangadex:bocchi the rock' --index 1 --range 3 --reader true --no-interactive
jq -e '.entries | length == 1 and .[0].last_read == 3 and .[0].total_chapters > 100' \
  "$AP0DL_MANGA_LIBRARY" >/dev/null
"$bin" manga library update --json | jq -e 'length == 1' >/dev/null

echo "== Automatic same-title fallback =="
AP0DL_MANGAPILL_BASE='http://127.0.0.1:9' timeout 120 "$bin" manga read \
  'https://mangapill.com/manga/5573/bocchi-the-rock' --range 5 --print-url --no-interactive \
  >"$tmp/fallback.out" 2>"$tmp/fallback.err"
grep -q '^https://' "$tmp/fallback.out"
grep -q 'Using ' "$tmp/fallback.err"

echo "PASS: three live manga sources, merged ranking, CBZ downloads, TUI checklist, chapter prompt + saved state, mpv viewer, library, fallback"
