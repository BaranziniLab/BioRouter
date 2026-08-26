// @vitest-environment node
//
// Node, not the project default of jsdom, and the reason is load-bearing: this
// module runs in the Electron main process, and under jsdom `Buffer` and
// `Uint8Array` come from different realms, so `adm-zip` fails the
// `instanceof Uint8Array` test on a real Buffer, mistakes the archive for an
// options object and hands back an empty zip — silently. Every assertion about
// what an archive contains would pass vacuously.
import AdmZip from 'adm-zip';
import { Buffer } from 'node:buffer';
import { describe, expect, it } from 'vitest';
import {
  assertSafeRasterImageDimensions,
  validateOfficeDocumentShape,
  validatedOfficeZip,
} from './artifactPreviewLimits';

function png(width: number, height: number) {
  const value = Buffer.alloc(24);
  // The real 8-byte signature, because an entry inside an Office archive is
  // recognised by its magic bytes rather than by its filename.
  value.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  value.write('IHDR', 12, 'ascii');
  value.writeUInt32BE(width, 16);
  value.writeUInt32BE(height, 20);
  return value;
}

/** A minimal .docx carrying one image at `word/media/`. */
function documentWithMedia(media: Buffer, name = 'word/media/image1.png') {
  const archive = new AdmZip();
  archive.addFile('word/document.xml', Buffer.from('<w:document><w:body/></w:document>'));
  archive.addFile(name, media);
  return archive.toBuffer();
}

function gif(width: number, height: number) {
  const value = Buffer.alloc(10);
  value.write('GIF89a', 0, 'ascii');
  value.writeUInt16LE(width, 6);
  value.writeUInt16LE(height, 8);
  return value;
}

function bmp(width: number, height: number) {
  const value = Buffer.alloc(26);
  value.write('BM', 0, 'ascii');
  value.writeInt32LE(width, 18);
  value.writeInt32LE(height, 22);
  return value;
}

function jpeg(width: number, height: number) {
  const value = Buffer.alloc(14);
  value.set([0xff, 0xd8, 0xff, 0xc0, 0x00, 0x0a, 0x08]);
  value.writeUInt16BE(height, 7);
  value.writeUInt16BE(width, 9);
  return value;
}

function webp(width: number, height: number) {
  const value = Buffer.alloc(30);
  value.write('RIFF', 0, 'ascii');
  value.write('WEBP', 8, 'ascii');
  value.write('VP8X', 12, 'ascii');
  value.writeUIntLE(width - 1, 24, 3);
  value.writeUIntLE(height - 1, 27, 3);
  return value;
}

function avif(width: number, height: number) {
  const value = Buffer.alloc(20);
  value.writeUInt32BE(20, 0);
  value.write('ispe', 4, 'ascii');
  value.writeUInt32BE(width, 12);
  value.writeUInt32BE(height, 16);
  return value;
}

describe('artifact preview resource limits', () => {
  it.each([
    ['image/png', png(9_000, 100)],
    ['image/apng', png(9_000, 100)],
    ['image/gif', gif(9_000, 100)],
    ['image/bmp', bmp(9_000, 100)],
    ['image/jpeg', jpeg(9_000, 100)],
    ['image/webp', webp(9_000, 100)],
    ['image/avif', avif(9_000, 100)],
    ['image/svg+xml', Buffer.from('<svg width="9000" height="100"></svg>')],
  ])('rejects oversized %s dimensions before renderer decode', (mimeType, image) => {
    expect(() => assertSafeRasterImageDimensions(image, mimeType)).toThrow(
      'Image dimensions exceed the safe preview limit'
    );
  });

  it('allows a bounded raster image', () => {
    expect(() => assertSafeRasterImageDimensions(png(1_024, 768), 'image/png')).not.toThrow();
  });

  // SVG dimensions are CSS lengths, so an A4 page is `595.28 x 841.89` and a
  // fractional `viewBox` is ordinary. Rejecting those turned every vector
  // document in the panel into "Image dimensions exceed the safe preview
  // limit".
  it.each([
    ['explicit width and height', '<svg width="595.28" height="841.89"></svg>'],
    ['a fractional viewBox', '<svg viewBox="0 0 595.28 841.89"></svg>'],
  ])('accepts an A4 vector page declared with %s', (_case, markup) => {
    expect(() =>
      assertSafeRasterImageDimensions(Buffer.from(markup), 'image/svg+xml')
    ).not.toThrow();
  });

  // The byte, entry and ratio caps see nothing wrong with this archive, and
  // that is the whole point: a deflated 40000x40000 PNG is small on disk and
  // ~6.4 GB once Chromium decodes the `<img>` a preview renderer emits for it.
  it('rejects an Office archive whose embedded image is a decompression bomb', () => {
    expect(() => validatedOfficeZip(documentWithMedia(png(40_000, 40_000)))).toThrow(
      'Image dimensions exceed the safe preview limit'
    );
  });

  it('reads the embedded image by its magic bytes, not its extension', () => {
    expect(() =>
      validatedOfficeZip(documentWithMedia(png(40_000, 40_000), 'ppt/media/image1.dat'))
    ).toThrow('Image dimensions exceed the safe preview limit');
  });

  it('allows an Office archive whose embedded image is bounded', () => {
    expect(() => validatedOfficeZip(documentWithMedia(png(1_024, 768)))).not.toThrow();
  });

  it('leaves a non-image archive member alone', () => {
    expect(() =>
      validatedOfficeZip(
        documentWithMedia(Buffer.from('not an image at all'), 'word/media/note.txt')
      )
    ).not.toThrow();
  });

  it('rejects a workbook with too many sheets', () => {
    const archive = new AdmZip();
    for (let sheet = 1; sheet <= 51; sheet += 1) {
      archive.addFile(
        `xl/worksheets/sheet${sheet}.xml`,
        Buffer.from('<worksheet><dimension ref="A1"/></worksheet>')
      );
    }
    expect(() => validateOfficeDocumentShape(archive, 'xlsx')).toThrow(
      'Spreadsheet has too many sheets'
    );
  });

  it('rejects aggregate workbook ranges even when each sheet is individually bounded', () => {
    const archive = new AdmZip();
    for (let sheet = 1; sheet <= 2; sheet += 1) {
      archive.addFile(
        `xl/worksheets/sheet${sheet}.xml`,
        Buffer.from('<worksheet><dimension ref="A1:CV3000"/></worksheet>')
      );
    }
    expect(() => validateOfficeDocumentShape(archive, 'xlsx')).toThrow(
      'Spreadsheet used range is too large'
    );
  });
});
