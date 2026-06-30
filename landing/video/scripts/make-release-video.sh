#!/usr/bin/env bash
#
# make-release-video.sh — generate a "What's new" release clip for the About page.
#
# Usage:
#   scripts/make-release-video.sh <version>
#   e.g.  scripts/make-release-video.sh 1.86.0
#
# Reads variables from  releases/v<version>.json  and renders release.html with
# them, then derives a WebM + poster JPG. Drop a new releases/v<X.Y.Z>.json
# (same keys as v1.86.0.json) and re-run for every release — no HTML editing.
#
set -euo pipefail

VER="${1:?usage: make-release-video.sh <version>   (e.g. 1.86.0)}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"          # the video/ project dir
cd "$HERE"

VARS="releases/v${VER}.json"
OUT_DIR="../assets/videos"
POSTER_DIR="${OUT_DIR}/posters"
BASE="biorouter-release-v${VER}"
MP4="${OUT_DIR}/${BASE}.mp4"
WEBM="${OUT_DIR}/${BASE}.webm"
POSTER="${POSTER_DIR}/${BASE}.jpg"

[ -f "$VARS" ] || { echo "✗ no variables file: $VARS"; echo "  create it from releases/v1.86.0.json"; exit 1; }
mkdir -p "$OUT_DIR" "$POSTER_DIR"

MASTER="$(mktemp -t biorouter-release-XXXX).mp4"
echo "▶ rendering release video for v${VER} from ${VARS} (4K master)"
npx --yes hyperframes render \
  --composition release.html \
  --resolution landscape-4k --quality high --fps 30 \
  --variables-file "$VARS" \
  --output "$MASTER"

echo "▶ downscaling 4K master to crisp 1440p delivery + WebM + poster"
ffmpeg -y -loglevel error -i "$MASTER" -vf "scale=2560:1440:flags=lanczos" \
  -c:v libx264 -crf 20 -preset slow -pix_fmt yuv420p -movflags +faststart -an "$MP4"
ffmpeg -y -loglevel error -i "$MASTER" -vf "scale=2560:1440:flags=lanczos" \
  -c:v libvpx-vp9 -b:v 0 -crf 31 -row-mt 1 -an "$WEBM"
ffmpeg -y -loglevel error -ss 1.2 -i "$MASTER" -vf "scale=1920:-1:flags=lanczos" -frames:v 1 -q:v 2 "$POSTER"
rm -f "$MASTER"

echo "✓ done:"
ls -la "$MP4" "$WEBM" "$POSTER"
