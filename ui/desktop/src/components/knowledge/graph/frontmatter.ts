// ui/desktop/src/components/knowledge/graph/frontmatter.ts
import { load } from 'js-yaml';

/**
 * A page's frontmatter, turned into the labelled rows §4.8 item 2 specifies.
 *
 * ⚠ **This is the inspector's "single biggest fix", and it needed a real
 * parser.** The panel used to `indexOf('\n---\n')` the block out and dump the
 * raw string into a `<pre>` — so `synonyms: [MS, disseminated sclerosis]` was
 * shown to the reader as those literal characters, and a `sources[]` list of
 * objects arrived as YAML indentation. Rendering arrays as chips and object
 * lists as rows cannot be done by string surgery.
 *
 * ⚠ **`js-yaml@4`'s `load` IS the safe schema.** v3's `load` was the unsafe one
 * and `safeLoad` the guarded alias; v4 removed `safeLoad` and made `load`
 * default to `DEFAULT_SCHEMA`, which has no `!!js/function`, `!!js/regexp` or
 * `!!js/undefined` type. So this parses a KB page — a file an LLM wrote — with
 * no code-construction path.
 *
 * ⚠ **And it is CSP-clean, which is why it is this parser and not a faster
 * one.** The renderer runs under `script-src 'self'` and `worker-src 'self'`
 * (`src/main.ts`), so `new Function` throws `EvalError` and a Blob-URL worker
 * never starts — and both fail at RUNTIME only, after typechecking, bundling
 * and passing every jsdom test. js-yaml 4.3.0 contains no `eval`, no
 * `new Function`, no `Blob` and no `Worker`; that was measured over
 * `dist/js-yaml.mjs` and `lib/*.js`, not taken on trust.
 */

/** Splits a `---`-delimited frontmatter block off the top of a page. */
export function splitFrontmatter(content: string): { frontmatter: string | null; body: string } {
  if (!content.startsWith('---\n')) {
    return { frontmatter: null, body: content };
  }

  const end = content.indexOf('\n---\n', 4);
  if (end === -1) {
    return { frontmatter: null, body: content };
  }

  return {
    frontmatter: content.slice(4, end).trim(),
    body: content.slice(end + 5).trimStart(),
  };
}

/**
 * The `xref` prefixes §4.8 names, and where each resolves.
 *
 * Lower-cased keys because a KB page is written by a model and `pmid:`,
 * `PMID:` and `Pmid:` all occur. An unrecognised prefix is not an error — it
 * renders as text, which is the same "no allowlist" rule the rows follow.
 */
const XREF_RESOLVERS: Partial<Record<string, (accession: string) => string>> = {
  doi: (a) => `https://doi.org/${a}`,
  pmid: (a) => `https://pubmed.ncbi.nlm.nih.gov/${encodeURIComponent(a)}/`,
  pmcid: (a) => `https://www.ncbi.nlm.nih.gov/pmc/articles/${encodeURIComponent(a)}/`,
  arxiv: (a) => `https://arxiv.org/abs/${encodeURIComponent(a)}`,
  uniprotkb: (a) => `https://www.uniprot.org/uniprotkb/${encodeURIComponent(a)}`,
  hgnc: (a) =>
    `https://www.genenames.org/data/gene-symbol-report/#!/hgnc_id/HGNC:${encodeURIComponent(a)}`,
  mondo: (a) => `https://purl.obolibrary.org/obo/MONDO_${encodeURIComponent(a)}`,
  hp: (a) => `https://hpo.jax.org/app/browse/term/HP:${encodeURIComponent(a)}`,
  hpo: (a) => `https://hpo.jax.org/app/browse/term/HP:${encodeURIComponent(a)}`,
};

/**
 * The external URL for a `PREFIX:accession` token, or `null` when the prefix is
 * not one of the eight §4.8 recognises.
 *
 * Returning `null` rather than guessing is the point: a token that is not a
 * resolvable identifier must render as the text it is, not as a link to a 404.
 */
export function xrefHref(token: string): string | null {
  const at = token.indexOf(':');
  if (at <= 0) return null;
  const resolve = XREF_RESOLVERS[token.slice(0, at).toLowerCase()];
  const accession = token.slice(at + 1).trim();
  return resolve && accession.length > 0 ? resolve(accession) : null;
}

export type FrontmatterValue =
  | { kind: 'text'; text: string }
  /** A YAML sequence of scalars — chips, with any resolvable token linked. */
  | { kind: 'chips'; items: { text: string; href: string | null }[] }
  /** A sequence of mappings (`sources[]`) — one sub-row block per entry. */
  | { kind: 'entries'; entries: { label: string; text: string }[][] };

export interface FrontmatterRow {
  key: string;
  /** The key with underscores opened up, for display. */
  label: string;
  value: FrontmatterValue;
}

function scalarText(value: unknown): string {
  if (value == null) return '';
  if (value instanceof Date) return value.toISOString();
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Parse a frontmatter block into rows.
 *
 * Returns `null` when the block is not a YAML mapping — malformed YAML, or a
 * bare scalar. The caller falls back to showing the raw text, because a page
 * whose frontmatter does not parse still has frontmatter the user may need to
 * SEE in order to fix it. Swallowing it would turn a visible defect into an
 * invisible one.
 */
export function frontmatterRows(raw: string): FrontmatterRow[] | null {
  let parsed: unknown;
  try {
    parsed = load(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed)) return null;

  return Object.entries(parsed).map(([key, value]) => {
    const label = key.replace(/_/g, ' ');

    if (Array.isArray(value)) {
      // A sequence of mappings is `sources[]`-shaped and becomes sub-rows; a
      // sequence of scalars becomes chips. Nothing is keyed on the NAME
      // `sources`, so a base that calls the list something else still renders.
      if (value.length > 0 && value.every(isRecord)) {
        return {
          key,
          label,
          value: {
            kind: 'entries',
            entries: (value as Record<string, unknown>[]).map((entry) =>
              Object.entries(entry).map(([k, v]) => ({
                label: k.replace(/_/g, ' '),
                text: scalarText(v),
              }))
            ),
          },
        };
      }
      return {
        key,
        label,
        value: {
          kind: 'chips',
          items: value.map((item) => {
            const text = scalarText(item);
            return { text, href: xrefHref(text) };
          }),
        },
      };
    }

    if (isRecord(value)) {
      return {
        key,
        label,
        value: {
          kind: 'entries',
          entries: [
            Object.entries(value).map(([k, v]) => ({
              label: k.replace(/_/g, ' '),
              text: scalarText(v),
            })),
          ],
        },
      };
    }

    return { key, label, value: { kind: 'text', text: scalarText(value) } };
  });
}
