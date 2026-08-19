import { describe, expect, it } from 'vitest';
import { frontmatterRows, splitFrontmatter, xrefHref } from './frontmatter';

describe('splitFrontmatter', () => {
  it('splits a delimited block off the top', () => {
    const { frontmatter, body } = splitFrontmatter('---\nidentifier: MYC\n---\n# MYC\ntext');
    expect(frontmatter).toBe('identifier: MYC');
    expect(body).toBe('# MYC\ntext');
  });

  it('leaves a page with no frontmatter entirely alone', () => {
    expect(splitFrontmatter('# MYC')).toEqual({ frontmatter: null, body: '# MYC' });
  });

  it('treats an unterminated block as body, not as frontmatter', () => {
    // Otherwise a page that merely opens with `---` loses everything below it.
    expect(splitFrontmatter('---\nidentifier: MYC').frontmatter).toBeNull();
  });
});

describe('xrefHref', () => {
  it('resolves each prefix the spec names', () => {
    expect(xrefHref('DOI:10.1056/x')).toBe('https://doi.org/10.1056/x');
    expect(xrefHref('PMID:12345')).toBe('https://pubmed.ncbi.nlm.nih.gov/12345/');
    expect(xrefHref('MONDO:0005301')).toBe('https://purl.obolibrary.org/obo/MONDO_0005301');
    expect(xrefHref('UniProtKB:P05231')).toBe('https://www.uniprot.org/uniprotkb/P05231');
  });

  it('is case-insensitive, because a model writes the prefix either way', () => {
    expect(xrefHref('pmid:99')).toBe(xrefHref('PMID:99'));
  });

  it('returns null for an unrecognised prefix rather than guessing a URL', () => {
    // A guessed link is a link to a 404 wearing the authority of a real one.
    expect(xrefHref('WIDGET:1')).toBeNull();
    expect(xrefHref('no-colon')).toBeNull();
    expect(xrefHref('PMID:')).toBeNull();
  });
});

describe('frontmatterRows', () => {
  it('renders a scalar as text and a sequence as chips', () => {
    const rows = frontmatterRows('identifier: MS\nsynonyms:\n  - multiple sclerosis\n  - MS')!;
    expect(rows[0]).toEqual({
      key: 'identifier',
      label: 'identifier',
      value: { kind: 'text', text: 'MS' },
    });
    expect(rows[1].value).toEqual({
      kind: 'chips',
      items: [
        { text: 'multiple sclerosis', href: null },
        { text: 'MS', href: null },
      ],
    });
  });

  it('links a resolvable token inside a sequence', () => {
    const rows = frontmatterRows('xref:\n  - MONDO:0005301')!;
    expect(rows[0].value).toEqual({
      kind: 'chips',
      items: [{ text: 'MONDO:0005301', href: 'https://purl.obolibrary.org/obo/MONDO_0005301' }],
    });
  });

  it('renders a sequence of mappings as sub-rows, keyed on SHAPE not on the name', () => {
    // Nothing is special-cased on the literal key `sources`, so a base that
    // calls its list something else still renders as rows.
    const rows = frontmatterRows('citations:\n  - title: A paper\n    year: 2017')!;
    expect(rows[0].value).toEqual({
      kind: 'entries',
      entries: [
        [
          { label: 'title', text: 'A paper' },
          { label: 'year', text: '2017' },
        ],
      ],
    });
  });

  it('renders an unknown key with the same treatment — there is no allowlist', () => {
    const rows = frontmatterRows('brand_new_key: 7')!;
    expect(rows[0]).toEqual({
      key: 'brand_new_key',
      label: 'brand new key',
      value: { kind: 'text', text: '7' },
    });
  });

  it('returns null on malformed YAML so the caller can show the raw block', () => {
    // Frontmatter that fails to parse is still frontmatter the user needs to
    // SEE in order to fix it; swallowing it hides a real defect.
    expect(frontmatterRows('a:\n  - [unclosed')).toBeNull();
    expect(frontmatterRows('just a scalar')).toBeNull();
  });

  it('parses with the safe schema — no code-construction tag survives', () => {
    // js-yaml v4's `load` IS the safe schema; `!!js/function` is not in it.
    expect(frontmatterRows('fn: !!js/function "function(){}"')).toBeNull();
  });
});
