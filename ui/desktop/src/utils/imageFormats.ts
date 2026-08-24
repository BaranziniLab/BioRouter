// The one list of image formats the preview panel understands.
//
// ⚠ **This file exists because there were four of these lists and they were
// allowed to disagree.** A format has to be present in the main process's MIME
// map (which is what actually assigns `kind: 'image'`), in the renderer's
// "is this previewable" gate, in the prose-discovery regex, and in the tab-icon
// check. Miss one and the format half-works in a way that reads as a rendering
// bug rather than a missing list entry — and two *adjacent* surfaces
// (`ItemIcon`, `MentionPopover`) were already offering `bmp`/`tiff`/`ico` that
// the panel could only render as "this file can't be previewed here".
//
// Everything below is derived from one set, and `imageFormats.test.ts` asserts
// that every consumer still agrees with it.

/**
 * Formats Chromium decodes natively in an `<img>` tag, so supporting them costs
 * a list entry and nothing else.
 *
 * Measured 2026-08-23 by loading a real file of each format, not by reading
 * docs: Blink ships exactly eight raster decoders (`avif`, `bmp`, `gif`, `ico`,
 * `jpeg`, `jxl`, `png`, `webp`) plus a separate SVG document path.
 *
 * Deliberately absent:
 * - `jxl` — Blink has the decoder directory, but JPEG XL is flag-gated from
 *   Chrome 145 and Electron 39 ships Chromium 142. Adding the extension would
 *   produce a broken image, not a picture.
 * - `heic`/`heif`, `tiff`/`tif` — no decoder in Blink at all, on any platform.
 *   macOS does not help: Blink uses its own decoders rather than ImageIO. These
 *   need a real decoder; see `DECODABLE_IMAGE_EXTENSIONS`.
 */
export const NATIVE_IMAGE_EXTENSIONS = [
  'apng',
  'avif',
  'bmp',
  'cur',
  'gif',
  'ico',
  'jfif',
  'jpeg',
  'jpg',
  'pjpeg',
  'png',
  'svg',
  'webp',
] as const;

/**
 * Formats that need a decoder before Chromium can show them. Listed here so the
 * panel can say *which* format it is refusing and why, rather than falling
 * through to a generic "can't be previewed" card.
 */
export const DECODABLE_IMAGE_EXTENSIONS = ['heic', 'heif', 'tif', 'tiff'] as const;

/** Every extension the panel will attempt to show as an image. */
export const IMAGE_EXTENSIONS: ReadonlySet<string> = new Set<string>([
  ...NATIVE_IMAGE_EXTENSIONS,
  ...DECODABLE_IMAGE_EXTENSIONS,
]);

/**
 * Extension → MIME type, for the formats above.
 *
 * `ico`/`cur` use `image/x-icon` and `image/vnd.microsoft.icon`: Chromium reads
 * the payload rather than trusting the type, so either works, but the registered
 * type is the honest one to put on a data URL.
 */
export const IMAGE_MIME_TYPES: Readonly<Record<string, string>> = {
  apng: 'image/apng',
  avif: 'image/avif',
  bmp: 'image/bmp',
  cur: 'image/vnd.microsoft.icon',
  gif: 'image/gif',
  heic: 'image/heic',
  heif: 'image/heif',
  ico: 'image/x-icon',
  jfif: 'image/jpeg',
  jpeg: 'image/jpeg',
  jpg: 'image/jpeg',
  pjpeg: 'image/jpeg',
  png: 'image/png',
  svg: 'image/svg+xml',
  tif: 'image/tiff',
  tiff: 'image/tiff',
  webp: 'image/webp',
};

/**
 * The alternation for a regular expression that discovers image paths in prose.
 * Sorted longest-first so `jpeg` cannot be shadowed by `jpg`, and every literal
 * is a bare extension, so the caller supplies its own delimiters.
 */
export function imageExtensionAlternation(): string {
  return [...IMAGE_EXTENSIONS].sort((a, b) => b.length - a.length || a.localeCompare(b)).join('|');
}

/** Whether the panel will try to render this extension as an image. */
export function isImageExtension(extension: string | undefined | null): boolean {
  return extension ? IMAGE_EXTENSIONS.has(extension.toLowerCase()) : false;
}

/** Whether Chromium can decode this extension without help. */
export function isNativelyDecodableImage(extension: string | undefined | null): boolean {
  return extension
    ? (NATIVE_IMAGE_EXTENSIONS as readonly string[]).includes(extension.toLowerCase())
    : false;
}

/**
 * Chromium refuses to raster an image whose larger dimension exceeds this, and
 * it fails by rendering *nothing* rather than by throwing — which reads as a
 * blank panel. A converted whole-slide microscopy image reaches it easily.
 */
export const MAX_RENDERABLE_IMAGE_DIMENSION = 32_767;

/**
 * Above this, an image is handed to the renderer as a `blob:` URL instead of a
 * `data:` URL.
 *
 * A base64 data URL costs about 4/3 of the file size as a JS string and pays
 * that twice — once crossing the IPC boundary as a structured clone, once as a
 * DOM attribute. Chromium also degrades badly on multi-megabyte URLs. Blob URLs
 * have neither problem and handle 100 MB comfortably. Small images stay on data
 * URLs because a blob has to be revoked and that bookkeeping is not worth it for
 * a 20 KB icon.
 */
export const IMAGE_BLOB_URL_THRESHOLD_BYTES = 512 * 1024;
