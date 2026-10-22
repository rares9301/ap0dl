#!/usr/bin/env bash
# Prove the download engine handles HLS and direct media into playable files.
# Pass one or more media files produced by `ap0dl download`; the script checks
# container/codec dimensions with ffprobe. It is intentionally separate from
# verify-goal.sh because the sample files are hundreds of megabytes.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ "$#" -eq 0 ]; then
    cat >&2 <<'EOF'
usage: verify-downloads.sh <downloaded-media> [...]
example:
  ap0dl download 'animeparadise:cyberpunk edgerunners' --range 1 --quality 1080 -d /tmp/ap-dl2
  ap0dl download 'animetoki:cyberpunk edgerunners' --range 1 --quality 1080 -d /tmp/toki-dl
  ./scripts/verify-downloads.sh "/tmp/ap-dl2/Cyberpunk - Edgerunners/E01.mkv" \
    "/tmp/toki-dl/Cyberpunk - Edgerunners (Season 1) Dual Audio HEVC [1080p]/E01.mkv"
EOF
    exit 2
fi

cargo test --workspace hls
cargo test --workspace decrypts_aes128_segments

for file in "$@"; do
    test -s "$file"
    info="$(ffprobe -v error -select_streams v:0 \
        -show_entries stream=codec_name,width,height -of json "$file")"
    jq -e '.streams | length == 1
      and (.streams[0].codec_name == "h264" or .streams[0].codec_name == "hevc")
      and .streams[0].width >= 1280 and .streams[0].height >= 720' \
        <<<"$info" >/dev/null
    printf 'playable: %s (%sx%s)\n' "$file" \
        "$(jq -r '.streams[0].width' <<<"$info")" \
        "$(jq -r '.streams[0].height' <<<"$info")"
done

echo "PASS: direct/HLS downloads are playable video files"