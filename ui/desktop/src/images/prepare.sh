#!/usr/bin/env sh

# This script generates all icon formats from icon.svg and glyph.svg
# For macOS: uses sips (built-in) and iconutil
# For Windows .ico: requires ImageMagick's convert command

# Detect if we're on macOS or have ImageMagick
if command -v sips > /dev/null 2>&1; then
    echo "Using macOS sips for icon generation..."

    # Create template icons for the menu bar
    sips -s format png -z 22 22 glyph.svg --out iconTemplate.png
    sips -s format png -z 44 44 glyph.svg --out iconTemplate@2x.png
    sips -s format png -z 22 22 glyph.svg --out iconTemplateUpdate.png
    sips -s format png -z 44 44 glyph.svg --out iconTemplateUpdate@2x.png

    # Create main application icons from icon.svg
    sips -s format png -z 1024 1024 icon.svg --out icon.png
    sips -s format png -z 2048 2048 icon.svg --out icon@2x.png

    # Create macOS icon set (icns)
    mkdir -p icon.iconset
    sips -s format png -z 16 16 icon.svg --out icon.iconset/icon_16x16.png
    sips -s format png -z 32 32 icon.svg --out icon.iconset/icon_16x16@2x.png
    sips -s format png -z 32 32 icon.svg --out icon.iconset/icon_32x32.png
    sips -s format png -z 64 64 icon.svg --out icon.iconset/icon_32x32@2x.png
    sips -s format png -z 128 128 icon.svg --out icon.iconset/icon_128x128.png
    sips -s format png -z 256 256 icon.svg --out icon.iconset/icon_128x128@2x.png
    sips -s format png -z 256 256 icon.svg --out icon.iconset/icon_256x256.png
    sips -s format png -z 512 512 icon.svg --out icon.iconset/icon_256x256@2x.png
    sips -s format png -z 512 512 icon.svg --out icon.iconset/icon_512x512.png
    sips -s format png -z 1024 1024 icon.svg --out icon.iconset/icon_512x512@2x.png
    iconutil -c icns icon.iconset
    rm -rf icon.iconset

    # Create light variants
    cp icon.svg icon-light.svg
    cp icon.icns icon-light.icns
    cp icon.png icon-light.png

    echo "✓ Icon generation complete (macOS formats)"
    echo "⚠ Note: Windows .ico generation requires ImageMagick"

elif command -v convert > /dev/null 2>&1; then
    echo "Using ImageMagick convert for icon generation..."

    # Create template icons for the menu bar
    convert -background none -resize 22x22 glyph.svg iconTemplate.png
    convert -background none -resize 44x44 glyph.svg iconTemplate@2x.png
    convert -background none -resize 22x22 glyph.svg iconTemplateUpdate.png
    convert -background none -resize 44x44 glyph.svg iconTemplateUpdate@2x.png

    # Create main application icons from icon.svg
    convert -background none -resize 1024x1024 icon.svg icon.png
    convert -background none -resize 2048x2048 icon.svg icon@2x.png

    # Create Windows icon (ico) with multiple sizes
    convert icon.svg -background none -define icon:auto-resize=256,128,64,48,32,16 icon.ico

    # Create macOS icon set (icns)
    mkdir -p icon.iconset
    convert -background none -resize 16x16 icon.svg icon.iconset/icon_16x16.png
    convert -background none -resize 32x32 icon.svg icon.iconset/icon_16x16@2x.png
    convert -background none -resize 32x32 icon.svg icon.iconset/icon_32x32.png
    convert -background none -resize 64x64 icon.svg icon.iconset/icon_32x32@2x.png
    convert -background none -resize 128x128 icon.svg icon.iconset/icon_128x128.png
    convert -background none -resize 256x256 icon.svg icon.iconset/icon_128x128@2x.png
    convert -background none -resize 256x256 icon.svg icon.iconset/icon_256x256.png
    convert -background none -resize 512x512 icon.svg icon.iconset/icon_256x256@2x.png
    convert -background none -resize 512x512 icon.svg icon.iconset/icon_512x512.png
    convert -background none -resize 1024x1024 icon.svg icon.iconset/icon_512x512@2x.png
    iconutil -c icns icon.iconset
    rm -rf icon.iconset

    # Create light variants
    cp icon.svg icon-light.svg
    cp icon.icns icon-light.icns
    cp icon.png icon-light.png

    echo "✓ Icon generation complete (all formats)"

else
    echo "Error: Neither sips (macOS) nor ImageMagick (convert) found"
    echo "Install ImageMagick: brew install imagemagick"
    exit 1
fi
