/**
 * The renderer's half of the `<biorouter-ref …>` contract (issue #65).
 *
 * A reference to a skill, extension or knowledge base travels in the message
 * text as a tag:
 *
 * ```text
 * <biorouter-ref type="skill" name="single-cell RNA &quot;QC&quot;">
 * <biorouter-ref type="extension" name="Chat Recall">
 * <biorouter-ref type="knowledge_base" id="soul" label="Soul &amp; Body">
 * ```
 *
 * The compact `/skill:name` markers it replaces cannot carry a name containing
 * whitespace: the backend extractor for those splits the message on whitespace,
 * so `/skill:my skill` reaches the resolver as `my`. Dropping the space instead
 * — the fix that worked for `/ext:` in issue #60, where every consumer
 * re-normalises anyway — cannot transfer, because `loadSkill` looks a skill up
 * *exactly*: `myskill` is not `my skill`.
 *
 * ## The escaping contract
 *
 * This module is a port. `crates/biorouter/src/agents/resource_refs.rs` is the
 * source of truth and its module docs are the spec; the two are pinned together
 * by `crates/biorouter/src/agents/resource_ref_corpus.json`, which both test
 * suites read. The table is closed — six entries, no others:
 *
 * | character | escaped as |
 * |-----------|------------|
 * | `&`       | `&amp;`    |
 * | `"`       | `&quot;`   |
 * | `<`       | `&lt;`     |
 * | `>`       | `&gt;`     |
 * | `\n`      | `&#10;`    |
 * | `\r`      | `&#13;`    |
 *
 * There is no general numeric-reference support and no `&apos;`/`&nbsp;`:
 * `&#65;` decodes to the literal text `&#65;`, not to `A`. `'` is never escaped
 * and single-quoted attribute values are rejected, so there is exactly one
 * quoting style for the two implementations to agree on.
 *
 * Both functions map one character (or one whole entity) at a time rather than
 * sweeping the string once per rule, which makes them order-free. Written as
 * chained `String.replace` calls instead, encoding must escape `&` **first**
 * (or `<` becomes `&amp;lt;`) and decoding must resolve `&amp;` **last** (or a
 * name containing the literal text `&quot;` decodes to a bare `"`).
 */

/** The element name every reference tag uses. */
export const REF_TAG_NAME = 'biorouter-ref';

/** Which kind of resource a reference names. Mirrors Rust's `RefKind`. */
export type RefKind = 'skill' | 'extension' | 'knowledge_base';

/**
 * The attribute a kind's identity travels in. Knowledge bases are named by `id`
 * because that is what `kb_search` takes; skills and extensions by `name`.
 */
export const refValueAttr = (kind: RefKind): 'id' | 'name' =>
  kind === 'knowledge_base' ? 'id' : 'name';

const REF_ENTITIES: ReadonlyArray<readonly [string, string]> = [
  ['&', '&amp;'],
  ['"', '&quot;'],
  ['<', '&lt;'],
  ['>', '&gt;'],
  ['\n', '&#10;'],
  ['\r', '&#13;'],
];

/** Escape `value` for an attribute of a reference tag. */
export function encodeRefValue(value: string): string {
  let out = '';
  for (const char of value) {
    const entity = REF_ENTITIES.find(([raw]) => raw === char);
    out += entity ? entity[1] : char;
  }
  return out;
}

/**
 * Unescape an attribute value from a reference tag.
 *
 * Anything outside the table survives untouched — a lone `&`, `&nbsp;`,
 * `&#65;`. Dropping them would mangle names and guessing at them would make
 * this disagree with {@link encodeRefValue}.
 */
export function decodeRefValue(value: string): string {
  let out = '';
  let index = 0;

  while (index < value.length) {
    if (value[index] !== '&') {
      out += value[index];
      index += 1;
      continue;
    }

    const entity = REF_ENTITIES.find(([, escaped]) => value.startsWith(escaped, index));
    if (entity) {
      out += entity[0];
      index += entity[1].length;
    } else {
      // Not an entity we emit: keep the `&` verbatim and resume one character
      // on, so `&amp;` inside `&amp;amp;` still decodes exactly once.
      out += '&';
      index += 1;
    }
  }

  return out;
}

/** The canonical tag naming `value`. */
export function refTag(kind: RefKind, value: string): string {
  return `<${REF_TAG_NAME} type="${kind}" ${refValueAttr(kind)}="${encodeRefValue(value)}">`;
}

/**
 * The canonical tag naming `value`, carrying a display string for the chip.
 *
 * Only a knowledge base's label is read back by the backend (it has somewhere
 * to go); on the other kinds it is presentation-only and the parser ignores it.
 * That tolerance is what lets the composer add chip attributes without
 * coordinating with the backend.
 */
export function labelledRefTag(kind: RefKind, value: string, label: string): string {
  return `<${REF_TAG_NAME} type="${kind}" ${refValueAttr(kind)}="${encodeRefValue(
    value
  )}" label="${encodeRefValue(label)}">`;
}

/** A reference the parser recognised, with the span of source it consumed. */
export interface RefSpan {
  kind: RefKind;
  /** The decoded identity: a skill/extension name, or a knowledge-base id. */
  value: string;
  /** The decoded display string, when the tag carried one. */
  label?: string;
  /** Byte offset of the `<` this tag starts at. */
  start: number;
  /** Byte offset one past the closing `>`. */
  end: number;
  /** The exact source the tag occupied, for a verbatim fallback. */
  raw: string;
}

const REF_KINDS: RefKind[] = ['skill', 'extension', 'knowledge_base'];

const isRefKind = (value: string): value is RefKind => (REF_KINDS as string[]).includes(value);

const leadingWhitespace = (input: string): number => input.length - input.trimStart().length;

/**
 * Parse the attribute list following the tag name, up to and including the `>`.
 *
 * Deliberately permissive about shape — any attribute order, extra attributes,
 * a valueless attribute, `/>` or `>`, whitespace anywhere — because the
 * alternative is losing a reference the user explicitly attached over a
 * cosmetic difference in serialisation. Strict about exactly two things: the
 * value's quoting (double quotes, entity-escaped) and that the tag is closed.
 *
 * Returns `null` when the tag never closes. An unterminated tag is dropped
 * rather than half-honoured: a message truncated mid-attribute must not load a
 * resource nobody named.
 */
function parseTagAttrs(input: string): { attrs: [string, string][]; consumed: number } | null {
  const attrs: [string, string][] = [];
  let pos = 0;

  for (;;) {
    if (pos > input.length) return null;
    pos += leadingWhitespace(input.slice(pos));
    const rest = input.slice(pos);

    if (rest.startsWith('/>')) return { attrs, consumed: pos + 2 };
    if (rest.startsWith('>')) return { attrs, consumed: pos + 1 };
    if (rest === '') return null;

    const nameEnd = rest.search(/[\s=>/]/);
    const nameLen = nameEnd === -1 ? rest.length : nameEnd;
    // A stray `=` or `/` where an attribute name belongs: not a shape we
    // understand, so leave the whole tag alone.
    if (nameLen === 0) return null;
    const name = rest.slice(0, nameLen);
    pos += nameLen;

    pos += leadingWhitespace(input.slice(pos));
    if (input[pos] !== '=') {
      // A valueless attribute (`<biorouter-ref … data-chip>`). Record it empty
      // and keep going rather than discarding an otherwise good tag over a
      // decoration.
      attrs.push([name, '']);
      continue;
    }
    pos += 1;
    pos += leadingWhitespace(input.slice(pos));

    // Double quotes only. Accepting `'` too would need `'` in the escape table
    // to be safe, and one quoting style is one fewer way for an emitter to
    // drift from this parser.
    if (input[pos] !== '"') return null;
    pos += 1;
    // Scanning to the next raw `"` is correct *because* of the escaping: an
    // escaped quote is `&quot;`, which contains no quote at all, so the first
    // one found is always the closing one.
    const end = input.indexOf('"', pos);
    if (end === -1) return null;
    attrs.push([name, input.slice(pos, end)]);
    pos = end + 1;
  }
}

/**
 * Every reference tag in `text`, in source order.
 *
 * Mirrors the backend's `parse_ref_tags` so the chip a user sees and the
 * resource the agent loads can never disagree: a tag rendered as a chip here is
 * one the agent resolves, and a tag left as text here is one the agent ignores.
 *
 * Tags are honoured wherever they appear, including inside a code fence — the
 * backend does the same, and a chip silently dropped because it sits in a fence
 * is worse than a fenced example being announced in the reply.
 */
export function findRefTags(text: string): RefSpan[] {
  const found: RefSpan[] = [];
  let index = 0;

  for (;;) {
    const start = text.indexOf('<', index);
    if (start === -1) break;
    // Advance past this `<` before anything else. Every failure below resumes
    // from here, which is what stops a malformed tag from either spinning
    // forever or eating the tags that follow it.
    index = start + 1;

    if (!text.startsWith(REF_TAG_NAME, index)) continue;
    const afterName = text.slice(index + REF_TAG_NAME.length);
    // `<biorouter-reference …>` is a different element: the name has to end
    // here, not merely start here.
    if (!/^[\s>/]/.test(afterName)) continue;

    // A tag never spans a raw `<`. A correctly encoded value carries `&lt;`
    // instead, so a raw one means this tag is malformed and another is starting.
    const nextTag = afterName.indexOf('<');
    const bounded = nextTag === -1 ? afterName : afterName.slice(0, nextTag);

    const parsed = parseTagAttrs(bounded);
    if (!parsed) continue;

    const attr = (key: string): string | undefined => {
      const hit = parsed.attrs.find(([name]) => name === key);
      // The raw value is trimmed *before* decoding, so slop in a hand-written
      // tag is forgiven while an encoded `&#10;` at either end survives.
      return hit ? hit[1].trim() : undefined;
    };

    const end = index + REF_TAG_NAME.length + parsed.consumed;
    index = end;

    // `type` is compared raw: a closed keyword set never needs escaping, and
    // decoding it would only invent a second spelling per type. An unknown type
    // is ignored rather than guessed at, so a newer composer can add one
    // without this build mis-filing it.
    const kind = attr('type') ?? '';
    if (!isRefKind(kind)) continue;

    const value = decodeRefValue(attr(refValueAttr(kind)) ?? '');
    if (value === '') continue;

    const label = attr('label') ? decodeRefValue(attr('label') as string) : undefined;

    found.push({
      kind,
      value,
      label: label || undefined,
      start,
      end,
      raw: text.slice(start, end),
    });
  }

  return found;
}

/** A run of plain text, or one reference, from {@link segmentRefTags}. */
export type RefSegment = { type: 'text'; text: string } | { type: 'ref'; ref: RefSpan };

/**
 * Split `text` into the runs of prose between its reference tags and the
 * references themselves, so a renderer can draw a chip in place of each tag.
 *
 * Text the parser did not claim comes back verbatim — including a malformed
 * tag, which the user then sees exactly as they typed it rather than as a blank
 * where a reference used to be.
 */
export function segmentRefTags(text: string): RefSegment[] {
  const segments: RefSegment[] = [];
  let cursor = 0;

  for (const ref of findRefTags(text)) {
    if (ref.start > cursor) segments.push({ type: 'text', text: text.slice(cursor, ref.start) });
    segments.push({ type: 'ref', ref });
    cursor = ref.end;
  }

  if (cursor < text.length) segments.push({ type: 'text', text: text.slice(cursor) });
  return segments;
}
