import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  REF_TAG_NAME,
  decodeRefValue,
  encodeRefValue,
  findRefTags,
  labelledRefTag,
  refTag,
  segmentRefTags,
} from './resourceRefs';

// The corpus is shared with the Rust implementation rather than restated here.
// A restated corpus is a second copy that drifts, and the drift is invisible:
// both sides keep passing their own tests while producing different bytes for
// the same name, so the reference silently resolves to nothing.
//
// vitest runs with `ui/desktop` as the cwd; `process.cwd()` + `..` is how the
// rest of this package reaches the repo root (see `src/biorouterd.ts`).
const corpusPath = path.join(
  process.cwd(),
  '..',
  '..',
  'crates/biorouter/src/agents/resource_ref_corpus.json'
);
const CORPUS: [string, string][] = JSON.parse(readFileSync(corpusPath, 'utf8')).pairs;

describe('encodeRefValue / decodeRefValue', () => {
  it('produces exactly the bytes the Rust encoder produces', () => {
    for (const [raw, encoded] of CORPUS) {
      expect(encodeRefValue(raw), `encoding of ${JSON.stringify(raw)}`).toBe(encoded);
    }
  });

  it('round-trips every hostile name', () => {
    for (const [raw] of CORPUS) {
      expect(decodeRefValue(encodeRefValue(raw)), `round trip of ${JSON.stringify(raw)}`).toBe(raw);
    }
  });

  // A chained-replacement encoder that escapes `&` last produces `&amp;lt;` for
  // `<`; a chained decoder that resolves `&amp;` first turns the encoding of the
  // literal text `&quot;` back into a bare `"`. Both are silent corruptions.
  it('escapes the ampersand first and decodes it last', () => {
    expect(encodeRefValue('<')).toBe('&lt;');
    expect(encodeRefValue('&')).toBe('&amp;');
    expect(encodeRefValue('&lt;')).toBe('&amp;lt;');
    expect(encodeRefValue('&quot;')).toBe('&amp;quot;');

    expect(decodeRefValue('&amp;lt;')).toBe('&lt;');
    expect(decodeRefValue('&amp;quot;')).toBe('&quot;');
    expect(decodeRefValue('&amp;amp;')).toBe('&amp;');
  });

  it('covers the whole table and nothing else', () => {
    expect(encodeRefValue('a&b"c<d>e\nf\rg')).toBe('a&amp;b&quot;c&lt;d&gt;e&#10;f&#13;g');
    // `'`, tab and non-ASCII are deliberately left alone.
    expect(encodeRefValue("it's\ta — b")).toBe("it's\ta — b");
  });

  // The table is closed: there is no general numeric-reference support and no
  // `&apos;`/`&nbsp;`. Anything outside it survives verbatim rather than being
  // dropped or guessed at, so a name that merely looks like markup is not
  // rewritten behind the user's back.
  it('leaves an unknown or malformed entity alone', () => {
    for (const input of [
      'caf&eacute;',
      '&#65;',
      '&#x41;',
      '&apos;',
      '&nbsp;',
      'AT&T',
      'a & b',
      '&amp',
      '&',
      '&;',
      '&quot',
    ]) {
      expect(decodeRefValue(input), `mangled ${input}`).toBe(input);
    }

    expect(decodeRefValue('&nbsp;&amp;&nbsp;')).toBe('&nbsp;&&nbsp;');
  });
});

describe('refTag / labelledRefTag', () => {
  it('escapes the values it embeds', () => {
    expect(refTag('skill', 'say "hi" & bye')).toBe(
      '<biorouter-ref type="skill" name="say &quot;hi&quot; &amp; bye">'
    );
    expect(refTag('extension', 'Chat Recall')).toBe(
      '<biorouter-ref type="extension" name="Chat Recall">'
    );
    expect(labelledRefTag('knowledge_base', 'soul', 'Soul & Body')).toBe(
      '<biorouter-ref type="knowledge_base" id="soul" label="Soul &amp; Body">'
    );
  });

  it('names knowledge bases by id and everything else by name', () => {
    expect(refTag('knowledge_base', 'soul')).toContain('id="soul"');
    expect(refTag('skill', 'rna-qc')).toContain('name="rna-qc"');
  });
});

describe('findRefTags', () => {
  it('reads back every hostile name a tag can carry', () => {
    for (const [raw] of CORPUS) {
      const found = findRefTags(`before ${refTag('skill', raw)} after`);
      if (raw.trim() === '') {
        // An empty value is not a reference — the backend drops it too.
        expect(found).toHaveLength(0);
        continue;
      }
      expect(found, `tag for ${JSON.stringify(raw)}`).toHaveLength(1);
      expect(found[0].kind).toBe('skill');
      expect(found[0].value).toBe(raw);
    }
  });

  it('accepts attributes in any order, extras, self-closing and newlines', () => {
    const found = findRefTags(
      `<biorouter-ref name="rna-qc" label="RNA QC" type="skill" />
       <biorouter-ref
          type="extension"
          data-chip
          name="Chat Recall">
       <biorouter-ref type="knowledge_base" id="soul" label="Soul &amp; Body">`
    );

    expect(found.map((ref) => [ref.kind, ref.value, ref.label])).toEqual([
      ['skill', 'rna-qc', 'RNA QC'],
      ['extension', 'Chat Recall', undefined],
      ['knowledge_base', 'soul', 'Soul & Body'],
    ]);
  });

  it('rejects a single-quoted value', () => {
    expect(findRefTags(`<biorouter-ref type='skill' name='rna-qc'>`)).toEqual([]);
  });

  it('does not match a longer element name', () => {
    expect(findRefTags(`<biorouter-reference type="skill" name="rna-qc">`)).toEqual([]);
  });

  it('ignores an unknown type', () => {
    expect(findRefTags(`<biorouter-ref type="workflow" name="nightly">`)).toEqual([]);
  });

  // The failure the old Rust parser had, one delimiter along: a scan to the
  // first `"` let a broken tag claim an unrelated quote later in the message and
  // consume the good tag that followed it.
  it('does not let a malformed tag swallow the next one', () => {
    const found = findRefTags(
      `<biorouter-ref type="skill" name="broken <biorouter-ref type="skill" name="good">`
    );
    expect(found.map((ref) => ref.value)).toEqual(['good']);
  });

  it('drops an unterminated tag without hanging', () => {
    expect(findRefTags(`<biorouter-ref type="skill" name="never closed`)).toEqual([]);
    expect(findRefTags(`<biorouter-ref`)).toEqual([]);
    expect(findRefTags(`<<<biorouter-ref type="skill"`)).toEqual([]);
  });

  it('keeps a good tag on either side of a broken one', () => {
    const found = findRefTags(
      `${refTag('skill', 'first')} <biorouter-ref type="skill" name="broken ${refTag('skill', 'second')}`
    );
    expect(found.map((ref) => ref.value)).toEqual(['first', 'second']);
  });

  it('reports the exact span it consumed', () => {
    const tag = refTag('skill', 'rna-qc');
    const text = `use ${tag} please`;
    const [found] = findRefTags(text);

    expect(text.slice(found.start, found.end)).toBe(tag);
    expect(found.raw).toBe(tag);
  });

  it('finds adjacent tags with no separator', () => {
    const found = findRefTags(`${refTag('skill', 'a')}${refTag('extension', 'b')}`);
    expect(found.map((ref) => ref.value)).toEqual(['a', 'b']);
  });

  it('exposes the element name it looks for', () => {
    expect(REF_TAG_NAME).toBe('biorouter-ref');
  });
});

describe('segmentRefTags', () => {
  it('interleaves text and references in source order', () => {
    const segments = segmentRefTags(
      `ask ${refTag('skill', 'rna qc')} about ${refTag('knowledge_base', 'soul')}`
    );

    expect(
      segments.map((segment) => (segment.type === 'text' ? segment.text : segment.ref.value))
    ).toEqual(['ask ', 'rna qc', ' about ', 'soul']);
  });

  it('returns a single text segment when there is no tag', () => {
    expect(segmentRefTags('plain message')).toEqual([{ type: 'text', text: 'plain message' }]);
  });

  it('returns nothing for an empty string', () => {
    expect(segmentRefTags('')).toEqual([]);
  });

  // A tag the parser refuses is left in the text verbatim, so the user sees
  // what they typed instead of a blank where a reference used to be.
  it('leaves an unparseable tag as text', () => {
    const broken = `<biorouter-ref type="skill" name="never closed`;
    expect(segmentRefTags(broken)).toEqual([{ type: 'text', text: broken }]);
  });
});
