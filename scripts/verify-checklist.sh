#!/usr/bin/env bash
# Prove the ordered checklist records started/completed state for every item.
set -euo pipefail
cd "$(dirname "$0")/.."

cmp -s docs/goal-checklist.md /root/goals/ap0dl-rust-rewrite.md
grep -q "Status: complete" docs/goal-checklist.md
grep -q "Final verification" docs/goal-checklist.md
grep -q "Live smoke evidence" docs/goal-checklist.md
! grep -Eq '\| \[ \] \||\| \[~\] \|' docs/goal-checklist.md

rows="$(grep -Eo '^\| [0-9]+ \|' docs/goal-checklist.md | grep -Eo '[0-9]+' | tr '\n' ' ')"
test "$rows" = "1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 "

for provider in reanime animegg anidb kisskh animeparadise animetoki; do
    evidence="docs/evidence/provider-$provider.txt"
    test -s "$evidence"
    jq -e --arg provider "$provider" \
        '.provider == $provider and .episode_count > 0
         and (.required_stream.quality == 720 or .required_stream.quality == 1080)
         and (.required_stream.kind == "hls" or .required_stream.kind == "direct")' \
        "$evidence" >/dev/null
done

for name in "Re:Anime" "AnimeGG" "AniDB" "KissKH" "AnimeParadise" "AnimeToki"; do
    grep -q "$name" docs/evidence/live-smoke-2026-10-22.md
done

echo "PASS: 21 checklist items have started/completed state and final evidence"
