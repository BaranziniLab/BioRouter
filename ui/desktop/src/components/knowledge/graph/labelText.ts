// ui/desktop/src/components/knowledge/graph/labelText.ts
//
// Helpers that keep node labels human-readable in the force graph:
//   - `prettyLabel` rescues labels that are really machine identifiers
//     (UUIDs, content hashes, `<uuid>.pdf` filenames) so the canvas shows a
//     natural-language name instead of gibberish.
//   - `wrapLabel` folds a long label across multiple lines so a single node
//     never paints one very long horizontal string.

const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
const LONG_HEX_RE = /\b[0-9a-f]{16,}\b/gi;
const TRAILING_SHORT_HASH_RE = /[-_][0-9a-f]{6,8}$/i;

/// Returns true when `s` looks like a machine identifier rather than a
/// human-authored title: a bare UUID, a long hex/base-something hash, or one of
/// those with a file extension (e.g. `a64e171e-…-pdf-48f040`).
export function looksMachineGenerated(s: string): boolean {
  const t = s.trim();
  if (!t) return true;
  const stripped = t.replace(/\.(pdf|docx?|html?|txt|md|csv|pptx?)$/i, '');
  // Mostly hex + separators, with no run of letters that reads as a word.
  const compact = stripped.replace(/[-_\s.]/g, '');
  if (compact.length >= 12 && /^[0-9a-f]+$/i.test(compact)) return true;
  if (UUID_RE.test(t)) {
    UUID_RE.lastIndex = 0;
    // A UUID plus a couple of "words" (e.g. "pdf") is still gibberish.
    const remainder = t
      .replace(UUID_RE, ' ')
      .replace(/\b(pdf|docx?|html?|txt|md|csv|pptx?)\b/gi, ' ')
      .replace(/[-_\d]+/g, ' ')
      .trim();
    UUID_RE.lastIndex = 0;
    return remainder.length < 3;
  }
  return false;
}

/// Produce a display-safe label. If the raw label reads as a machine
/// identifier, strip the hash-like tokens and fall back to a generic but
/// honest name derived from the node kind.
export function prettyLabel(label: string, kind?: string): string {
  const raw = (label ?? '').trim();
  if (!looksMachineGenerated(raw)) {
    // Still drop a trailing short hash suffix if a readable stem precedes it.
    const stem = raw.replace(TRAILING_SHORT_HASH_RE, '');
    return stem.length >= 3 ? stem : raw;
  }
  const cleaned = raw
    .replace(UUID_RE, ' ')
    .replace(LONG_HEX_RE, ' ')
    .replace(/\.(pdf|docx?|html?|txt|md|csv|pptx?)$/i, ' ')
    .replace(/[-_]+/g, ' ')
    .split(/\s+/)
    // Drop file-type tokens and standalone hex fragments (hash leftovers).
    .filter(
      (w) =>
        w.length > 0 &&
        !/^(pdf|docx?|html?|txt|md|csv|pptx?)$/i.test(w) &&
        !(/^[0-9a-f]+$/i.test(w) && w.length >= 4)
    )
    .join(' ')
    .trim();
  if (cleaned.length >= 3) return cleaned;
  const noun = kind === 'source' ? 'source' : kind ? kind : 'item';
  return `Untitled ${noun}`;
}

/// Greedily wrap `label` into at most `maxLines` lines, each no wider than
/// `maxWidth` device-independent pixels under the current canvas font. The last
/// line is ellipsised if content remains. `measure` is the canvas
/// `ctx.measureText(...).width` callback (already accounts for font + scale).
export function wrapLabel(
  label: string,
  maxWidth: number,
  maxLines: number,
  measure: (s: string) => number
): string[] {
  const words = label.split(/\s+/).filter(Boolean);
  if (words.length === 0) return [label];
  const lines: string[] = [];
  let current = '';

  const pushWord = (word: string) => {
    const candidate = current ? `${current} ${word}` : word;
    if (measure(candidate) <= maxWidth || current === '') {
      current = candidate;
    } else {
      lines.push(current);
      current = word;
    }
  };

  for (const word of words) {
    if (lines.length >= maxLines) break;
    pushWord(word);
  }
  if (current && lines.length < maxLines) lines.push(current);

  // If we ran out of lines with words remaining, ellipsise the final line.
  const consumed = lines.join(' ').split(/\s+/).filter(Boolean).length;
  if (consumed < words.length && lines.length > 0) {
    let last = lines[lines.length - 1];
    while (last.length > 1 && measure(`${last}…`) > maxWidth) {
      last = last.slice(0, -1).trimEnd();
    }
    lines[lines.length - 1] = `${last}…`;
  }
  return lines;
}
