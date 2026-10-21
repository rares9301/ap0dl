#!/usr/bin/env bash
# Prove the download engine handles direct MP4 and HLS/AES media into playable files.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo test --workspace hls
cargo test --workspace decrypts_aes128_segments

mp4='/tmp/ap0dl-mp4/One Piece/E01.mp4'
ts='/tmp/ap0dl-dl/Bocchi the Rock!/E01.ts'
for file in "$mp4" "$ts"; do
    test -s "$file"
    codec="$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 "$file" | head -1)"
    test "$codec" = "h264"
done

echo "PASS: direct MP4 and HLS/AES downloads are playable (h264)"