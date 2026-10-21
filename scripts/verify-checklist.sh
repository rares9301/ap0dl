#!/usr/bin/env bash
# Prove the ordered checklist records started/completed state for every item.
set -euo pipefail
cd "$(dirname "$0")/.."

cmp -s docs/goal-checklist.md /root/goals/ap0dl-rust-rewrite.md
python3 - <<'PY'
import json
import pathlib
import re

path = pathlib.Path("docs/goal-checklist.md")
text = path.read_text()
assert "Status: complete" in text
assert "Goal revision: 3" in text

rows = []
for line in text.splitlines():
    match = re.match(r"^\|\s*(\d+)\s*\|", line)
    if not match:
        continue
    number = int(match.group(1))
    rows.append(number)
    assert 1 <= number <= 18, line
    assert re.search(r"\|\s*\[x\]\s*\|\s*\[x\]\s*\|", line), line
assert rows == list(range(1, 19)), rows
assert "| [ ] |" not in text and "| [~] |" not in text
assert "Final verification" in text
assert "Live smoke evidence" in text

for provider in ("reanime", "animegg", "anidb", "kisskh"):
    evidence = pathlib.Path(f"docs/evidence/provider-{provider}.txt")
    data = json.loads(evidence.read_text())
    assert data["provider"] == provider
    assert data["episode_count"] > 0
    assert data["required_stream"]["quality"] in (720, 1080)
    assert data["required_stream"]["kind"] in ("hls", "direct")

smoke = pathlib.Path("docs/evidence/live-smoke-2026-10-21.md").read_text()
for name in ("Re:Anime", "AnimeGG", "AniDB", "KissKH"):
    assert name in smoke, name
PY

echo "PASS: 18 checklist items have started/completed state and final evidence"