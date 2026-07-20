#!/usr/bin/env sh

# Regenerate every shipped icon raster for the BioRouter "BR" mark.
#
# ============================================================================
# WHY THIS SCRIPT RESIZES PNG MASTERS, NOT SVGs (do not "simplify" this back):
#   The BR mark is heavy weight-800 letters. `sips` renders an SVG <text> node
#   but FLATTENS font-weight - a weight-800 heavy B comes out medium - and
#   ImageMagick's SVG path is font-availability dependent too. So we do NOT
#   rasterize icon.svg / glyph.svg here. Instead we resize two committed,
#   browser-rendered PNG masters (rendered through Chromium, which honours
#   weight 800):
#     br-icon-beige.png  (1024) -> app icon  (icon.png/@2x/.icns/.ico + landing/CLI)
#     br-glyph-mono.png  (1024) -> menu-bar template (iconTemplate*.png)
#   Resizing a PNG->PNG (sips or magick/convert) is faithful - it only flattens
#   when RASTERIZING SVG text. The .svg files are kept as the editable,
#   browser-faithful source-of-truth; regenerate the masters through the
#   studio (docs/design/branding/logo-icon-studio.html) if the design changes, not here.
# ============================================================================

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../../.." && pwd)
cd "$SCRIPT_DIR" || exit 1

MASTER="br-icon-beige.png"       # app icon master (cream plate, heavy BR)
GLYPH_MASTER="br-glyph-mono.png" # menu-bar template master (solid black BR, transparent)

if [ ! -f "$MASTER" ] || [ ! -f "$GLYPH_MASTER" ]; then
    echo "Error: missing PNG master(s): $MASTER / $GLYPH_MASTER"
    echo "These are browser-rendered from docs/design/branding/logo-icon-studio.html; do not delete them."
    exit 1
fi

# resize_png <in.png> <out.png> <size> : faithful PNG->PNG downscale.
if command -v sips > /dev/null 2>&1; then
    echo "Using macOS sips for PNG resizing..."
    resize_png() { sips -s format png -z "$3" "$3" "$1" --out "$2" > /dev/null; }
elif command -v magick > /dev/null 2>&1; then
    echo "Using ImageMagick magick for PNG resizing..."
    resize_png() { magick "$1" -resize "$3x$3" "$2"; }
elif command -v convert > /dev/null 2>&1; then
    echo "Using ImageMagick convert for PNG resizing..."
    resize_png() { convert "$1" -resize "$3x$3" "$2"; }
else
    echo "Error: neither sips (macOS) nor ImageMagick (magick/convert) found"
    echo "Install ImageMagick: brew install imagemagick"
    exit 1
fi

# --- macOS menu-bar template icons (monochrome BR, from the mono master) ---
resize_png "$GLYPH_MASTER" iconTemplate.png            22
resize_png "$GLYPH_MASTER" iconTemplate@2x.png         44
resize_png "$GLYPH_MASTER" iconTemplateUpdate.png      22
resize_png "$GLYPH_MASTER" iconTemplateUpdate@2x.png   44

# --- main application icons (heavy BR on cream plate, from the beige master) ---
# icon.png is the 1024 master verbatim; @2x is a 2x upscale (acceptable - the
# master carries all the detail the icon pipeline needs).
cp "$MASTER" icon.png
resize_png "$MASTER" icon@2x.png 2048

# --- macOS icon set (.icns), each size resized from the master ---
mkdir -p icon.iconset
resize_png "$MASTER" icon.iconset/icon_16x16.png        16
resize_png "$MASTER" icon.iconset/icon_16x16@2x.png     32
resize_png "$MASTER" icon.iconset/icon_32x32.png        32
resize_png "$MASTER" icon.iconset/icon_32x32@2x.png     64
resize_png "$MASTER" icon.iconset/icon_128x128.png     128
resize_png "$MASTER" icon.iconset/icon_128x128@2x.png  256
resize_png "$MASTER" icon.iconset/icon_256x256.png     256
resize_png "$MASTER" icon.iconset/icon_256x256@2x.png  512
resize_png "$MASTER" icon.iconset/icon_512x512.png     512
resize_png "$MASTER" icon.iconset/icon_512x512@2x.png 1024
if command -v iconutil > /dev/null 2>&1; then
    iconutil -c icns icon.iconset
else
    echo "Warning: iconutil (macOS) not found - icon.icns not refreshed"
fi
rm -rf icon.iconset

# --- Windows icon (.ico), multi-size, from the master via ImageMagick ---
if command -v magick > /dev/null 2>&1; then
    magick "$MASTER" -background none -define icon:auto-resize=256,128,64,48,32,16 icon.ico
elif command -v convert > /dev/null 2>&1; then
    convert "$MASTER" -background none -define icon:auto-resize=256,128,64,48,32,16 icon.ico
else
    echo "Warning: ImageMagick (magick/convert) not found - skipping icon.ico refresh"
fi

# --- light variants are byte-for-byte copies of the primaries ---
cp icon.svg  icon-light.svg
cp icon.png  icon-light.png
[ -f icon.icns ] && cp icon.icns icon-light.icns

echo "✓ Desktop icon generation complete"

# --- keep every other shipped logo surface tied to the same reviewed mark ---
# SVG sources (editable) are copied as-is; PNGs are resized from the beige
# master (never sips-rasterized from the text SVG - see the caveat above).
cp icon.svg "$ROOT/landing/icon.svg"
cp icon.svg "$ROOT/landing/video/icon.svg"
cp icon.svg "$ROOT/landing/video/assets/icon.svg"
cp icon.svg "$ROOT/landing/video/reel/icon.svg"

resize_png "$MASTER" "$ROOT/landing/icon.png" 512
cp "$ROOT/landing/icon.png" "$ROOT/crates/biorouter-cli/static/img/logo_dark.png"
cp "$ROOT/landing/icon.png" "$ROOT/crates/biorouter-cli/static/img/logo_light.png"

echo "✓ Shared desktop, CLI, and landing assets synchronized"
