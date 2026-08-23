
/**
 * Why the panel cannot show a particular file.
 *
 * The panel used to answer every one of these with the same sentence — "This
 * file can't be previewed here." — which is true and useless. A researcher who
 * opens a `.doc` and a `.heic` and a `.key` gets one message and no way to tell
 * whether the file is broken, the app is broken, or the format was never
 * supported. Naming the format and saying what to do instead costs a table.
 */
export type FormatSupportNote = {
  /** How a person refers to the format, not the extension. */
  label: string;
  /** Why this file cannot be rendered. One sentence, no apology. */
  reason: string;
  /** What the user can actually do about it, when there is something. */
  suggestion?: string;
};

/**
 * Formats the panel deliberately declines, with the reason.
 *
 * Each entry is a decision, not an oversight, and the reason is recorded so the
 * next person does not have to re-derive it:
 *
 * - The **legacy Office binaries** (`.doc`, `.xls`, `.ppt`) are a completely
 *   different container from the OOXML formats the panel renders — they are not
 *   "an older version" of them. No permissively-licensed browser renderer
 *   exists for `.doc` or `.ppt` at all.
 * - **OpenDocument** has the same problem in the other direction: parsers exist,
 *   faithful renderers do not.
 * - **Apple iWork** files are opaque bundles with no published format.
 */
const DOCUMENT_FORMAT_NOTES: Readonly<Record<string, FormatSupportNote>> = {
  doc: {
    label: 'Legacy Word document',
    reason:
      'This is the pre-2007 binary Word format, which is a different container from the .docx files this panel renders.',
    suggestion: 'Save it as .docx to preview it here.',
  },
  xls: {
    label: 'Legacy Excel workbook',
    reason:
      'This is the pre-2007 binary Excel format, which is a different container from the .xlsx files this panel renders.',
    suggestion: 'Save it as .xlsx to preview it here.',
  },
  ppt: {
    label: 'Legacy PowerPoint presentation',
    reason:
      'This is the pre-2007 binary PowerPoint format, which is a different container from the .pptx files this panel renders.',
    suggestion: 'Save it as .pptx to preview it here.',
  },
  odt: {
    label: 'OpenDocument text',
    reason: 'This panel has no OpenDocument renderer.',
    suggestion: 'Export it as .docx or PDF to preview it here.',
  },
  ods: {
    label: 'OpenDocument spreadsheet',
    reason: 'This panel has no OpenDocument renderer.',
    suggestion: 'Export it as .xlsx or PDF to preview it here.',
  },
  odp: {
    label: 'OpenDocument presentation',
    reason: 'This panel has no OpenDocument renderer.',
    suggestion: 'Export it as .pptx or PDF to preview it here.',
  },
  rtf: {
    label: 'Rich Text Format document',
    reason: 'This panel has no RTF renderer.',
    suggestion: 'Save it as .docx or PDF to preview it here.',
  },
  pages: {
    label: 'Apple Pages document',
    reason: 'Pages files are an undocumented bundle format with no open renderer.',
    suggestion: 'Export it as .docx or PDF to preview it here.',
  },
  numbers: {
    label: 'Apple Numbers spreadsheet',
    reason: 'Numbers files are an undocumented bundle format with no open renderer.',
    suggestion: 'Export it as .xlsx or PDF to preview it here.',
  },
  key: {
    label: 'Apple Keynote presentation',
    reason: 'Keynote files are an undocumented bundle format with no open renderer.',
    suggestion: 'Export it as .pptx or PDF to preview it here.',
  },
};

/**
 * Image formats the panel still cannot show, and why.
 *
 * TIFF is deliberately absent: it is decoded in the renderer now, so a TIFF
 * that fails is a *broken file*, not an unsupported format, and the image
 * preview says so itself. HEIC remains here because its decoder is the
 * operating system's — available on macOS, and nowhere else without taking on
 * a licence and patent decision that is not a code change.
 */
const IMAGE_FORMAT_LABELS: Readonly<Record<string, string>> = {
  heic: 'HEIC image',
  heif: 'HEIF image',
};

/**
 * What to tell the user about a file the panel will not render, or `null` if
 * the panel has no specific complaint and the generic message is honest.
 */
export function describeUnsupportedFormat(
  extension: string | undefined | null
): FormatSupportNote | null {
  if (!extension) return null;
  const ext = extension.toLowerCase();

  const document = DOCUMENT_FORMAT_NOTES[ext];
  if (document) return document;

  const imageLabel = IMAGE_FORMAT_LABELS[ext];
  if (imageLabel) {
    return {
      label: imageLabel,
      reason:
        'No browser can decode this format, and this system has no converter for it either.',
      suggestion: 'Convert it to PNG or JPEG to preview it here.',
    };
  }

  return null;
}

/**
 * The honest ceiling on what the bundled renderers reproduce, surfaced in the
 * UI rather than left for the user to discover.
 *
 * Every non-commercial renderer for these formats shares these limits. Saying so
 * beats a researcher concluding their file is corrupt because a slide's
 * animation or a document's table of contents did not survive.
 */
export const DOCUMENT_FIDELITY_NOTES: Readonly<Record<string, string>> = {
  docx: 'Tables, images and styling render; a table of contents and exact page breaks do not.',
  pptx: 'Text, tables, charts and shapes render; animations, transitions, 3D effects, equations and speaker notes do not.',
  xlsx: 'Values and cell styling render; formulas show their last computed value, and charts do not render.',
  pdf: 'Pages render as in the source document.',
};
