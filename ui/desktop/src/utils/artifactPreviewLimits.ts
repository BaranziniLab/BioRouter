import AdmZip from 'adm-zip';
import { Buffer } from 'node:buffer';
import type { FileHandle } from 'node:fs/promises';

const OFFICE_MAX_ENTRIES = 5_000;
const OFFICE_MAX_ENTRY_BYTES = 16 * 1024 * 1024;
const OFFICE_MAX_EXPANDED_BYTES = 64 * 1024 * 1024;
export const IMAGE_MAX_DIMENSION = 8_192;
export const IMAGE_MAX_PIXELS = 32_000_000;

/**
 * Reads at most `maxBytes`, refusing rather than truncating.
 *
 * The refusal is the point: a truncated buffer is a *corrupt* artifact that the
 * renderer would try to decode, so an over-limit file has to fail loudly here
 * instead of arriving as half an image. Reading through the handle rather than
 * `fs.readFile` keeps the one-shot whole-file allocation — the thing that turns
 * an oversized file into a main-process OOM — out of reach entirely.
 */
export async function readFileHandleBounded(handle: FileHandle, maxBytes: number): Promise<Buffer> {
  const chunks: Buffer[] = [];
  let total = 0;
  while (total <= maxBytes) {
    const chunk = Buffer.allocUnsafe(Math.min(1024 * 1024, maxBytes + 1 - total));
    const { bytesRead } = await handle.read(chunk, 0, chunk.length, null);
    if (bytesRead === 0) break;
    chunks.push(chunk.subarray(0, bytesRead));
    total += bytesRead;
  }
  if (total > maxBytes) throw new Error('Artifact exceeds the preview size limit');
  return Buffer.concat(chunks, total);
}

function jpegDimensions(buffer: Buffer): { width: number; height: number } | null {
  if (buffer.length < 4 || buffer[0] !== 0xff || buffer[1] !== 0xd8) return null;
  let offset = 2;
  while (offset + 8 < buffer.length) {
    if (buffer[offset] !== 0xff) {
      offset += 1;
      continue;
    }
    const marker = buffer[offset + 1];
    if (marker === 0xd8 || marker === 0xd9) {
      offset += 2;
      continue;
    }
    const length = buffer.readUInt16BE(offset + 2);
    if (length < 2 || offset + 2 + length > buffer.length) return null;
    if (
      (marker >= 0xc0 && marker <= 0xc3) ||
      (marker >= 0xc5 && marker <= 0xc7) ||
      (marker >= 0xc9 && marker <= 0xcb) ||
      (marker >= 0xcd && marker <= 0xcf)
    ) {
      return {
        height: buffer.readUInt16BE(offset + 5),
        width: buffer.readUInt16BE(offset + 7),
      };
    }
    offset += 2 + length;
  }
  return null;
}

function avifDimensions(buffer: Buffer): { width: number; height: number } | null {
  for (let offset = 4; offset + 16 <= buffer.length; offset += 1) {
    if (buffer.toString('ascii', offset, offset + 4) !== 'ispe') continue;
    const boxStart = offset - 4;
    const boxSize = buffer.readUInt32BE(boxStart);
    if (boxSize < 20 || boxStart + boxSize > buffer.length) continue;
    return { width: buffer.readUInt32BE(offset + 8), height: buffer.readUInt32BE(offset + 12) };
  }
  return null;
}

function svgDimensions(buffer: Buffer): { width: number; height: number } | null {
  const source = buffer.subarray(0, 64 * 1024).toString('utf8');
  const svg = source.match(/<svg\b([^>]*)>/i)?.[1];
  if (!svg) return null;
  const dimension = (name: string) => {
    const value = svg.match(new RegExp(`\\b${name}\\s*=\\s*["']\\s*([0-9.]+)`, 'i'))?.[1];
    return value ? Number(value) : null;
  };
  const width = dimension('width');
  const height = dimension('height');
  if (width !== null && height !== null) return { width, height };
  const viewBox = svg
    .match(/\bviewBox\s*=\s*["']\s*[-0-9.]+[ ,]+[-0-9.]+[ ,]+([0-9.]+)[ ,]+([0-9.]+)/i)
    ?.slice(1)
    .map(Number);
  return viewBox ? { width: viewBox[0], height: viewBox[1] } : null;
}

function rasterImageDimensions(buffer: Buffer, mimeType: string) {
  if (
    (mimeType === 'image/png' || mimeType === 'image/apng') &&
    buffer.length >= 24 &&
    buffer.toString('ascii', 12, 16) === 'IHDR'
  ) {
    return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
  }
  if (mimeType === 'image/gif' && buffer.length >= 10) {
    return { width: buffer.readUInt16LE(6), height: buffer.readUInt16LE(8) };
  }
  if (mimeType === 'image/bmp' && buffer.length >= 26) {
    return { width: Math.abs(buffer.readInt32LE(18)), height: Math.abs(buffer.readInt32LE(22)) };
  }
  if (mimeType === 'image/jpeg') return jpegDimensions(buffer);
  if (mimeType === 'image/avif') return avifDimensions(buffer);
  if (mimeType === 'image/svg+xml') return svgDimensions(buffer);
  if (
    mimeType === 'image/webp' &&
    buffer.length >= 30 &&
    buffer.toString('ascii', 0, 4) === 'RIFF' &&
    buffer.toString('ascii', 8, 12) === 'WEBP'
  ) {
    const format = buffer.toString('ascii', 12, 16);
    if (format === 'VP8X') {
      return {
        width: 1 + buffer.readUIntLE(24, 3),
        height: 1 + buffer.readUIntLE(27, 3),
      };
    }
    if (format === 'VP8 ' && buffer.length >= 30) {
      return {
        width: buffer.readUInt16LE(26) & 0x3fff,
        height: buffer.readUInt16LE(28) & 0x3fff,
      };
    }
    if (format === 'VP8L' && buffer.length >= 25 && buffer[20] === 0x2f) {
      const bits = buffer.readUInt32LE(21);
      return { width: 1 + (bits & 0x3fff), height: 1 + ((bits >> 14) & 0x3fff) };
    }
  }
  return null;
}

export function assertSafeRasterImageDimensions(buffer: Buffer, mimeType: string): void {
  const dimensions = rasterImageDimensions(buffer, mimeType);
  if (!dimensions) return;
  // SVG carries CSS lengths, so an ordinary A4 page is `595.28 x 841.89` and a
  // fractional `viewBox` is routine. Rounding up keeps the comparison against
  // integer caps honest — and keeps a `Number.isSafeInteger` test from refusing
  // every well-formed vector document. NaN and Infinity survive `Math.ceil`
  // unchanged, so a garbage header is still rejected below.
  const width = Math.ceil(dimensions.width);
  const height = Math.ceil(dimensions.height);
  if (
    !Number.isSafeInteger(width) ||
    !Number.isSafeInteger(height) ||
    width <= 0 ||
    height <= 0 ||
    width > IMAGE_MAX_DIMENSION ||
    height > IMAGE_MAX_DIMENSION ||
    width * height > IMAGE_MAX_PIXELS
  ) {
    throw new Error('Image dimensions exceed the safe preview limit');
  }
}

/**
 * Which image container an archive member really is, read from its magic bytes
 * rather than its name.
 *
 * The name is not evidence: the preview renderers hand these entries to
 * Chromium as blobs and Chromium sniffs content, so an image named `.dat` still
 * decodes and an image named `.png` that is really a JPEG still decodes. Only
 * the bytes decide what gets parsed.
 */
function sniffImageMimeType(buffer: Buffer): string | null {
  if (buffer.length < 12) return null;
  if (buffer.toString('hex', 0, 8) === '89504e470d0a1a0a') return 'image/png';
  if (buffer.toString('ascii', 0, 3) === 'GIF') return 'image/gif';
  if (buffer.toString('ascii', 0, 2) === 'BM') return 'image/bmp';
  if (buffer[0] === 0xff && buffer[1] === 0xd8 && buffer[2] === 0xff) return 'image/jpeg';
  if (buffer.toString('ascii', 0, 4) === 'RIFF' && buffer.toString('ascii', 8, 12) === 'WEBP') {
    return 'image/webp';
  }
  // AVIF and HEIC are both ISO-BMFF and both carry their extent in an `ispe`
  // box, which is the box the AVIF parser looks for.
  if (buffer.toString('ascii', 4, 8) === 'ftyp') return 'image/avif';
  if (/<svg[\s>]/i.test(buffer.subarray(0, 1024).toString('utf8'))) return 'image/svg+xml';
  return null;
}

/** Where the Office renderers pull `<img>` sources from. */
function isEmbeddedMediaEntry(entryName: string): boolean {
  return /(?:^|\/)media\//i.test(entryName) || /^docProps\/thumbnail\./i.test(entryName);
}

/**
 * Rejects an archive carrying an image bomb.
 *
 * The byte, entry and ratio caps bound what an archive *weighs*; not one of
 * them bounds what it *decodes to*. A 1.6 MB PNG declaring 40000x40000 is
 * already deflated (so the ratio guard sees ~1), sits under the per-entry cap
 * and lands far under the aggregate — then `docx-preview` hands it to Chromium
 * as an `<img>` that decodes to ~6.4 GB in the renderer. The header states the
 * extent before a single pixel is decoded, so read the header.
 */
function assertSafeEmbeddedMedia(zip: AdmZip): void {
  for (const entry of zip.getEntries()) {
    if (entry.isDirectory || !isEmbeddedMediaEntry(entry.entryName)) continue;
    let data: Buffer;
    try {
      data = entry.getData();
    } catch {
      // An entry we cannot inflate is one the renderer cannot inflate either,
      // so a corrupt thumbnail must not cost the user the whole document.
      continue;
    }
    const mimeType = sniffImageMimeType(data);
    if (mimeType) assertSafeRasterImageDimensions(data, mimeType);
  }
}

export function validatedOfficeZip(buffer: Buffer): AdmZip {
  const zip = new AdmZip(buffer);
  const entries = zip.getEntries();
  if (entries.length > OFFICE_MAX_ENTRIES) throw new Error('Office archive has too many entries');
  let expandedBytes = 0;
  for (const entry of entries) {
    const size = entry.header.size;
    const compressed = Math.max(1, entry.header.compressedSize);
    if (size > OFFICE_MAX_ENTRY_BYTES || size / compressed > 200) {
      throw new Error('Office archive entry exceeds preview limits');
    }
    expandedBytes += size;
    if (expandedBytes > OFFICE_MAX_EXPANDED_BYTES) {
      throw new Error('Office archive expands past the preview limit');
    }
  }
  // Runs after the weight checks, so inflating the media entries is already
  // bounded by the per-entry and aggregate caps above.
  assertSafeEmbeddedMedia(zip);
  return zip;
}

function spreadsheetColumnNumber(reference: string): number {
  return [...reference.toUpperCase()].reduce(
    (value, character) => value * 26 + character.charCodeAt(0) - 64,
    0
  );
}

export function validateOfficeDocumentShape(zip: AdmZip, format: 'docx' | 'xlsx' | 'pptx'): void {
  if (format === 'pptx') {
    const slides = zip
      .getEntries()
      .filter((entry) => /^ppt\/slides\/slide\d+\.xml$/.test(entry.entryName)).length;
    if (slides > 500) throw new Error('Presentation has too many slides to preview safely');
    return;
  }
  if (format !== 'xlsx') return;

  const worksheets = zip
    .getEntries()
    .filter((candidate) => /^xl\/worksheets\/sheet\d+\.xml$/.test(candidate.entryName));
  if (worksheets.length > 50) {
    throw new Error('Spreadsheet has too many sheets to preview safely');
  }
  let aggregateUsedCells = 0;
  let aggregatePopulatedCells = 0;
  for (const entry of worksheets) {
    const xml = entry.getData().toString('utf8');
    const dimension = xml.match(/<dimension\b[^>]*\bref="(?:[A-Z]+\d+:)?([A-Z]+)(\d+)"/i);
    if (dimension) {
      const columns = spreadsheetColumnNumber(dimension[1]);
      const rows = Number(dimension[2]);
      const usedCells = columns * rows;
      aggregateUsedCells += usedCells;
      if (
        columns > 2_000 ||
        rows > 200_000 ||
        usedCells > 500_000 ||
        aggregateUsedCells > 500_000
      ) {
        throw new Error('Spreadsheet used range is too large to preview safely');
      }
    }
    const populatedCells = (xml.match(/<c\b/g) ?? []).length;
    aggregatePopulatedCells += populatedCells;
    if (populatedCells > 200_000 || aggregatePopulatedCells > 200_000) {
      throw new Error('Spreadsheet has too many populated cells to preview safely');
    }
  }
}
