import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { looksLikePreviewableFile } from '../components/artifacts/artifactUtils';
import {
  DECODABLE_IMAGE_EXTENSIONS,
  IMAGE_EXTENSIONS,
  IMAGE_MIME_TYPES,
  NATIVE_IMAGE_EXTENSIONS,
  imageExtensionAlternation,
  isImageExtension,
  isNativelyDecodableImage,
} from './imageFormats';

/**
 * The image list used to exist four times over and the copies disagreed.
 *
 * These tests exist to make that impossible again. Most of them read the *real
 * source* of each consumer rather than importing a value, because the failure
 * being guarded against is precisely a consumer that stops sharing the list and
 * goes back to a hard-coded array — which no amount of importing would catch.
 */

const SRC = join(__dirname, '..');
const read = (relative: string) => readFileSync(join(SRC, relative), 'utf8');

describe('the canonical image list', () => {
  it('gives every supported extension a MIME type', () => {
    for (const extension of IMAGE_EXTENSIONS) {
      expect(IMAGE_MIME_TYPES[extension], `no MIME type for .${extension}`).toMatch(/^image\//);
    }
  });

  it('is exactly the union of the native and decodable sets', () => {
    expect([...IMAGE_EXTENSIONS].sort()).toEqual(
      [...NATIVE_IMAGE_EXTENSIONS, ...DECODABLE_IMAGE_EXTENSIONS].sort()
    );
  });

  // The regression this whole change exists to fix: Chromium has decoded all
  // three of these for years and the panel refused them anyway.
  it.each(['avif', 'bmp', 'ico'])('treats %s as natively decodable', (extension) => {
    expect(isNativelyDecodableImage(extension)).toBe(true);
    expect(isImageExtension(extension)).toBe(true);
  });

  // Measured, not assumed: Blink has a jxl decoder directory, but JPEG XL is
  // flag-gated from Chrome 145 and Electron 39 ships Chromium 142. Claiming it
  // would render a broken image rather than a picture.
  it('does not claim JPEG XL, which this Chromium cannot decode', () => {
    expect(isImageExtension('jxl')).toBe(false);
  });

  // These are listed so the panel can name the format it is refusing, but they
  // must never be reported as natively renderable — Blink has no decoder for
  // either, on any platform, and macOS does not help.
  it.each(['heic', 'heif', 'tiff', 'tif'])('knows %s needs a decoder', (extension) => {
    expect(isImageExtension(extension)).toBe(true);
    expect(isNativelyDecodableImage(extension)).toBe(false);
  });

  it('orders the regex alternation longest-first so jpg cannot shadow jpeg', () => {
    const parts = imageExtensionAlternation().split('|');
    expect(parts.indexOf('jpeg')).toBeLessThan(parts.indexOf('jpg'));
    for (let i = 1; i < parts.length; i += 1) {
      expect(parts[i - 1].length).toBeGreaterThanOrEqual(parts[i].length);
    }
  });
});

describe('every consumer shares the one list', () => {
  // The decisive one. `mimeTypeForArtifactPath` is what assigns `kind: 'image'`,
  // so a format missing from it is a format the panel shows as an opaque binary
  // no matter what the other three lists say.
  it('the main process builds its MIME map from the shared table', () => {
    const main = read('main.ts');
    expect(main).toContain("from './utils/imageFormats'");
    expect(main).toContain('...Object.fromEntries(');
    expect(main).toContain('Object.entries(IMAGE_MIME_TYPES)');
    // No hand-written image rows may survive beside the spread.
    expect(main).not.toMatch(/'\.png':\s*'image\/png'/);
    expect(main).not.toMatch(/'\.webp':\s*'image\/webp'/);
  });

  it("the panel's previewable gate uses the shared set", () => {
    const utils = read('components/artifacts/artifactUtils.ts');
    expect(utils).toContain("from '../../utils/imageFormats'");
    expect(utils).not.toMatch(/const IMAGE_EXTENSIONS\s*=\s*new Set/);
  });

  it.each([...IMAGE_EXTENSIONS])('accepts an absolute .%s path as previewable', (extension) => {
    expect(looksLikePreviewableFile(`/tmp/figure.${extension}`)).toBe(true);
  });

  it('prose discovery is generated from the shared list', () => {
    const baseChat = read('components/BaseChat.tsx');
    expect(baseChat).toContain('imageExtensionAlternation()');
    // The old literal alternation must be gone, or adding a format silently
    // leaves discovery behind again.
    expect(baseChat).not.toContain('png|jpe?g|gif|webp|svg');
  });

  it('the tab icon asks the shared predicate rather than its own array', () => {
    const viewer = read('components/artifacts/ArtifactViewer.tsx');
    expect(viewer).toContain('isImageExtension(ext)');
    expect(viewer).not.toMatch(/\['png',\s*'jpg',\s*'jpeg',\s*'gif',\s*'webp',\s*'svg'\]/);
  });

  // These two are not the panel, but they *offer* files to it. Before this
  // change both listed bmp/tiff/ico that the panel could only render as "this
  // file can't be previewed here".
  it('the file icon shares the list', () => {
    const icon = read('components/ItemIcon.tsx');
    expect(icon).toContain('isImageExtension(ext)');
    expect(icon).not.toMatch(/'bmp',\s*'tiff',\s*'tif'/);
  });

  it('the mention popover shares the list', () => {
    const popover = read('components/MentionPopover.tsx');
    expect(popover).toContain('...IMAGE_EXTENSIONS');
    expect(popover).not.toMatch(/'bmp',\n\s*'tiff',/);
  });
});

describe('prose discovery actually matches the formats it advertises', () => {
  // Rebuilt exactly as BaseChat builds it, so this asserts the shape of the
  // real regex rather than a convenient approximation of it.
  const proseRe = () =>
    new RegExp(
      String.raw`(?<![\w:/\\@])(?:file://|~[\\/]|\.{1,2}[\\/]|[a-z]:[\\/]|/|\\\\)[^\s)\]}\x60"'<>]+\.(?:` +
        `html?|${imageExtensionAlternation()}|` +
        String.raw`pdf|docx)(?:[?#][^\s)\]}\x60"'<>]*)?(?![\w./\\])`,
      'gi'
    );

  it.each([...IMAGE_EXTENSIONS])('finds /tmp/plot.%s in prose', (extension) => {
    const matches = `see /tmp/plot.${extension} for the result`.match(proseRe());
    expect(matches).toEqual([`/tmp/plot.${extension}`]);
  });

  it('still does not match a bare relative path', () => {
    expect('see results/plot.png'.match(proseRe())).toBeNull();
  });
});
