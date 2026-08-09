/**
 * The long-message clamp — threshold, measurement and count, as pure functions.
 *
 * Design of record: `docs/design/astryx-adoption/astryx-ui-adoption-design.md`,
 * "A long message — collapsed by default". Before this, one pasted stack trace
 * could push the reply that followed it off screen, and nothing truncated.
 *
 * Everything here is deliberately free of React and of the DOM: the threshold is
 * the part that has to be right, and a threshold you can only exercise by
 * rendering a component is a threshold nobody re-tests. The view (`UserMessage`)
 * owns the clipping, the fade and the control; it owns none of the arithmetic.
 *
 * The one rule that governs the whole file: **the clamp knows nothing about what
 * it clamps.** It fires on length alone. There is no paste-origin signal in this
 * app (`handlePaste` early-returns for text), and gating the clamp on one would
 * be wrong even if there were — a long message does not have to be a paste, it
 * can simply be long. Content shape is consulted for exactly one thing: which
 * *unit* the count is stated in.
 */

/**
 * Clamp above ten lines. Ten exactly does not clamp: the design's rule is
 * "clamp above the threshold … and never below it", and the failure it is
 * guarding against — "a three-line message that collapses is worse than no
 * collapse at all" — does not stop being a failure one line under the line.
 */
export const CLAMP_LINE_THRESHOLD = 10;

/**
 * …or above 600 characters, which catches the other shape entirely: a single
 * unbroken paragraph is one line and can still be a wall of text.
 */
export const CLAMP_CHAR_THRESHOLD = 600;

/**
 * The collapsed height of the bubble, in px, matching the design specimen
 * (`astryx-design-showcase.html`, `.umsg--clamped`).
 *
 * The arithmetic, because it is not arbitrary: the bubble is `border-box` with
 * 10px of vertical padding and a 20px line-height (`--text-body--line-height`),
 * so 200px is exactly 10 + 9×20 + 10 — nine lines of text, with the fade
 * covering the last two and a bit of them.
 *
 * Nine, and not ten to match `CLAMP_LINE_THRESHOLD`, on purpose. The shortest
 * message that clamps at all is eleven lines; showing nine of it folds away two,
 * where a 220px cap would fold away one, and a control that hides a single line
 * is pure noise. The gap between the cap and the threshold is what guarantees
 * the clamp always earns its own control.
 */
export const CLAMP_MAX_HEIGHT_PX = 200;

/**
 * How long the expansion takes. Must track `--dur-med` (300ms) in `main.css`,
 * the tier that stylesheet's own comment already reserved for "long-message
 * expand". This copy exists because the component has to know when the growth
 * has finished so it can hand the element back to automatic sizing; see
 * `UserMessage`. Collapsing does not use it — collapsing is instant, because you
 * are moving away from the message rather than into it.
 */
export const CLAMP_EXPAND_MS = 300;

/**
 * Mean characters per line at or below which a message is treated as structured
 * by its line breaks rather than as running prose.
 *
 * 80 is the terminal/source width: anything wrapped by a machine or typed as a
 * log, a traceback, a table or a bulleted list sits under it, while prose typed
 * into a composer runs to whatever length the thought takes (the design's own
 * prose specimen is ~800 characters on a single line). Misclassifying only ever
 * changes the *unit* of the count — never whether the message clamps, and never
 * how it is typeset.
 */
const LINE_STRUCTURED_MEAN_WIDTH = 80;

/**
 * Which unit the count is stated in. `lines` counts what a log is measured in
 * (lines and bytes); `prose` counts what prose is measured in (words).
 */
export type MessageShape = 'lines' | 'prose';

export interface MessageExtent {
  /** Visual lines. A single trailing newline does not add one. */
  lines: number;
  words: number;
  /** UTF-8 bytes — what the "8.4 KB" of a pasted log actually measures. */
  bytes: number;
  chars: number;
  shape: MessageShape;
  /** True above either threshold. This, and nothing else, decides the clamp. */
  shouldClamp: boolean;
}

function countLines(text: string): number {
  // Drop one trailing newline before splitting: "a\n" is one line that happens
  // to be terminated, not two lines the second of which is empty.
  return text.replace(/\n$/, '').split('\n').length;
}

function countWords(text: string): number {
  const trimmed = text.trim();
  return trimmed === '' ? 0 : trimmed.split(/\s+/).length;
}

function countBytes(text: string): number {
  // Bytes, not characters: a message of CJK or emoji is several times larger on
  // the wire than its `.length` suggests, and "8.4 KB" is a claim about size.
  return new TextEncoder().encode(text).length;
}

/** One decimal, with a bare `.0` dropped — `8.4 KB`, but `84 KB`, not `84.0 KB`. */
function oneDecimal(value: number): string {
  return value.toFixed(1).replace(/\.0$/, '');
}

/** `842 B` · `8.4 KB` · `1.2 MB`. KB here is 1024 bytes, as every file manager means it. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${oneDecimal(kb)} KB`;
  return `${oneDecimal(kb / 1024)} MB`;
}

function plural(count: number, noun: string): string {
  return count === 1 ? noun : `${noun}s`;
}

export function measureMessageExtent(text: string): MessageExtent {
  const chars = text.length;
  const lines = countLines(text);
  const meanLineWidth = chars / lines;

  return {
    lines,
    words: countWords(text),
    bytes: countBytes(text),
    chars,
    // A single line can never be "structured by its line breaks" — it has none.
    shape: lines > 1 && meanLineWidth <= LINE_STRUCTURED_MEAN_WIDTH ? 'lines' : 'prose',
    shouldClamp: lines > CLAMP_LINE_THRESHOLD || chars > CLAMP_CHAR_THRESHOLD,
  };
}

/**
 * The count that rides next to the control — `214 lines · 8.4 KB` for a log,
 * `128 words` for prose.
 *
 * The count is the whole value of the control: it is what tells you whether
 * expanding is worth it. A bare "Show more" does not.
 */
export function formatMessageExtent(extent: MessageExtent): string {
  if (extent.shape === 'lines') {
    return `${extent.lines} ${plural(extent.lines, 'line')} · ${formatBytes(extent.bytes)}`;
  }
  return `${extent.words} ${plural(extent.words, 'word')}`;
}

/** Convenience for the view: the one decision and the one string it needs. */
export function describeMessageLength(text: string): {
  shouldClamp: boolean;
  label: string;
  extent: MessageExtent;
} {
  const extent = measureMessageExtent(text);
  return { shouldClamp: extent.shouldClamp, label: formatMessageExtent(extent), extent };
}
