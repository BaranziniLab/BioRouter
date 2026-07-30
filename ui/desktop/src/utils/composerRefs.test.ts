import { describe, expect, it } from 'vitest';
import {
  appendComposerRef,
  removeComposerRefAt,
  joinComposerText,
  splitComposerText,
} from './composerRefs';
import { refTag } from './resourceRefs';

const BODIES = [
  '',
  'hi',
  'hi ',
  '  ',
  'a message that ends in a newline\n',
  'two\n\nparagraphs',
  'trailing punctuation!',
  'a <not a tag> b',
];

describe('splitComposerText / joinComposerText', () => {
  // The property the composer rests on: whatever the user typed comes back
  // character for character. Anything less and a phantom space appears in the
  // textarea after every keystroke — and because React reassigns a controlled
  // value that disagrees with the DOM, the caret lands after it and the next
  // character is typed in the wrong place.
  it('round-trips every body exactly', () => {
    for (const body of BODIES) {
      for (const refs of [[], ['rna-qc'], ['rna-qc', 'my skill']]) {
        const text = refs.reduce((acc, name) => appendComposerRef(acc, 'skill', name), body);
        expect(
          splitComposerText(text).body,
          `body ${JSON.stringify(body)} with ${refs.length} refs`
        ).toBe(body);
      }
    }
  });

  it('is idempotent', () => {
    const once = appendComposerRef('hi', 'skill', 'my skill');
    const { body, refs } = splitComposerText(once);
    expect(joinComposerText(body, refs)).toBe(once);
  });

  it('leaves text with no reference completely alone', () => {
    for (const body of BODIES) {
      expect(splitComposerText(body)).toEqual({ body, refs: [] });
      expect(joinComposerText(body, [])).toBe(body);
    }
  });

  it('reads the references out in order', () => {
    const text = appendComposerRef(
      appendComposerRef('please', 'skill', 'my skill'),
      'knowledge_base',
      'soul'
    );

    expect(splitComposerText(text).refs.map((ref) => [ref.kind, ref.value])).toEqual([
      ['skill', 'my skill'],
      ['knowledge_base', 'soul'],
    ]);
  });

  // A draft restored from storage, a `?prompt=` deep link or a queued message
  // being edited can carry a tag anywhere in the text. Reference extraction is
  // position-independent on the backend, so moving it to the end costs nothing
  // and buys a body/suffix split with no per-character offset mapping.
  it('normalises a tag that arrives mid-message to the end', () => {
    const { body, refs } = splitComposerText(`use ${refTag('skill', 'rna-qc')} now`);

    expect(body).toBe('use now');
    expect(refs.map((ref) => ref.value)).toEqual(['rna-qc']);
    expect(joinComposerText(body, refs)).toBe(`use now ${refTag('skill', 'rna-qc')}`);
  });

  it('keeps a tag it cannot parse in the body, where the user can see it', () => {
    const broken = `<biorouter-ref type="skill" name="never closed`;
    expect(splitComposerText(broken)).toEqual({ body: broken, refs: [] });
  });
});

describe('appendComposerRef', () => {
  it('carries a name the compact marker would truncate', () => {
    const text = appendComposerRef('', 'skill', 'my skill');
    expect(splitComposerText(text).refs[0].value).toBe('my skill');
  });

  it('carries a name full of characters that would break the markup', () => {
    const hostile = 'single-cell "QC" & prep <v2>';
    const text = appendComposerRef('run it', 'skill', hostile);

    expect(splitComposerText(text).refs[0].value).toBe(hostile);
    expect(splitComposerText(text).body).toBe('run it');
  });

  it('records a display label without changing the identity', () => {
    const text = appendComposerRef('', 'knowledge_base', 'soul', 'Soul & Body');
    const [ref] = splitComposerText(text).refs;

    expect(ref.value).toBe('soul');
    expect(ref.label).toBe('Soul & Body');
  });

  // Picking the same skill twice is a slip, not a request for two copies of it,
  // and the backend dedups anyway — so the composer must not show two chips for
  // one resource.
  it('ignores a reference that is already attached', () => {
    const once = appendComposerRef('go', 'skill', 'my skill');
    expect(appendComposerRef(once, 'skill', 'my skill')).toBe(once);
  });

  it('distinguishes the same name across kinds', () => {
    const text = appendComposerRef(appendComposerRef('', 'skill', 'memory'), 'extension', 'memory');
    expect(splitComposerText(text).refs).toHaveLength(2);
  });
});

describe('removeComposerRefAt', () => {
  it('drops one reference and keeps the rest and the body', () => {
    const text = appendComposerRef(
      appendComposerRef('please run', 'skill', 'first'),
      'skill',
      'second'
    );

    const after = removeComposerRefAt(text, 0);
    expect(splitComposerText(after).body).toBe('please run');
    expect(splitComposerText(after).refs.map((ref) => ref.value)).toEqual(['second']);
  });

  it('leaves nothing behind when the last reference goes', () => {
    const text = appendComposerRef('please run', 'skill', 'only');
    expect(removeComposerRefAt(text, 0)).toBe('please run');
  });

  it('ignores an index that names no reference', () => {
    const text = appendComposerRef('go', 'skill', 'only');
    expect(removeComposerRefAt(text, 5)).toBe(text);
    expect(removeComposerRefAt(text, -1)).toBe(text);
  });
});
