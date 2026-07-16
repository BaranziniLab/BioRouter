#!/usr/bin/env bash

set -euo pipefail
cd "$(dirname "$0")/.."

canonical_icon="ui/desktop/src/images/icon.svg"
canonical_glyph="ui/desktop/src/images/glyph.svg"

for copy in \
  ui/desktop/src/images/icon-light.svg \
  landing/icon.svg \
  landing/video/icon.svg \
  landing/video/assets/icon.svg \
  landing/video/reel/icon.svg; do
  cmp -s "$canonical_icon" "$copy" || {
    echo "Brand asset drift: $copy differs from $canonical_icon" >&2
    exit 1
  }
done

for copy in \
  crates/biorouter-cli/static/img/logo_dark.png \
  crates/biorouter-cli/static/img/logo_light.png; do
  cmp -s landing/icon.png "$copy" || {
    echo "Brand asset drift: $copy differs from landing/icon.png" >&2
    exit 1
  }
done

grep -q 'M 125 220' "$canonical_glyph"
grep -q 'M 125 220' "$canonical_icon"
grep -q '"productName": "Biorouter"' ui/desktop/package.json
grep -q "glyph.svg" ui/desktop/src/components/icons/BioRouter.tsx

for page in docs/agentic-system.html docs/design-system.html docs/theme-system.html; do
  grep -q 'M 125 220' "$page" || {
    echo "Brand asset drift: $page does not contain the canonical glyph" >&2
    exit 1
  }
done

echo "Biorouter name and canonical logo assets are consistent"
