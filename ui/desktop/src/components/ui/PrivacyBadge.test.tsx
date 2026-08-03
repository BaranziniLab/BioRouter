import { readFileSync } from 'node:fs';
import path from 'node:path';
import type { ReactElement } from 'react';
import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { PrivacyBadge } from './PrivacyBadge';
import type { SessionClassification } from '../../api';

afterEach(cleanup);

/** vitest runs with `ui/desktop` as its root — same idiom as MentionPopover.test.tsx. */
const read = (...p: string[]) => readFileSync(path.join(process.cwd(), ...p), 'utf8');

/**
 * Every custom property `main.css` declares, anywhere in the file.
 *
 * This is the join `check-contrast.mjs` cannot make on its own: it reads
 * `main.css` and never opens this component, so a label written the way the
 * design's §14.1 specified it — an arbitrary value naming a token that exists
 * in no theme — passes every one of its assertions, raises no error, and
 * silently inherits whatever colour it lands on. That spelling is deliberately
 * not repeated here: `src/components` is grepped for it and owes zero hits, and
 * a comment is not a use. It is in the commit that added these assertions.
 */
const DECLARED_TOKENS = new Set(
  [...read('src', 'styles', 'main.css').matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1])
);

/**
 * The tokens the guard's four `privacy …` assertions actually measure.
 *
 * Hard-coding the expected pairs in the guard proves the *theme* holds them at
 * ratio; it does not prove this component paints them. Deriving the set from
 * the guard's own source and checking the rendered classes against it closes
 * that loop in the one direction that was open: a badge repainted in a token
 * nobody measured now fails here, not in a screenshot review six months on.
 */
const PRIVACY_ASSERTION_LINES = read('scripts', 'check-contrast.mjs')
  .split('\n')
  .filter((l) => l.includes('privacy '));
const MEASURED_TOKENS = new Set(
  PRIVACY_ASSERTION_LINES.flatMap((l) => [...l.matchAll(/'(--[\w-]+)'/g)].map((m) => m[1]))
);

/**
 * The colour tokens a rendered `className` names.
 *
 * Tailwind colour utilities here are `<prefix>-<token>`, where `<token>` is the
 * custom property minus its `--` (`bg-background-muted` → `--background-muted`,
 * `bg-text-default` → `--text-default`). Arbitrary values name their token
 * inside a `var()`, which is scanned separately — that form is exactly how the
 * design's non-existent token would have reached the DOM. `text-[11px]` is a
 * font size, not a colour, and is correctly ignored: its remainder names no
 * colour family.
 */
const COLOUR_FAMILY = /^(?:text|background|border|sidebar|ring)(?:-|$)/;
function colourTokensOf(className: string): string[] {
  const out: string[] = [];
  for (const cls of className.split(/\s+/)) {
    const m = /^(?:bg|text|border|ring|fill|stroke)-(.+)$/.exec(cls);
    if (!m) continue;
    const token = m[1].replace(/\/\d+$/, '');
    if (COLOUR_FAMILY.test(token)) out.push(`--${token}`);
  }
  for (const m of className.matchAll(/var\((--[\w-]+)\)/g)) out.push(m[1]);
  return [...new Set(out)];
}

const badgeOf = (ui: ReactElement) =>
  render(ui).container.querySelector('[data-testid="privacy-badge"]');

/** Every state that renders something. Public + dense renders nothing by design. */
const VISIBLE_STATES: { name: string; tier: SessionClassification; dense: boolean }[] = [
  { name: 'private pill', tier: 'private', dense: false },
  { name: 'public pill', tier: 'public', dense: false },
  { name: 'private dot', tier: 'private', dense: true },
];

describe('PrivacyBadge', () => {
  it('renders through the app badge primitive and adds no geometry of its own', () => {
    const { container } = render(<PrivacyBadge tier="private" />);
    const el = container.querySelector('[data-testid="privacy-badge"]')!;
    expect(el).toHaveTextContent('Private');
    expect(el.className).toContain('rounded-sm'); // from Badge, not hand-rolled
    expect(el.querySelector('svg')).not.toBeNull(); // never colour alone: shape + glyph + word
  });

  it('renders nothing in dense mode for a public session', () => {
    const { queryByTestId } = render(<PrivacyBadge tier="public" dense />);
    expect(queryByTestId('privacy-badge')).toBeNull(); // no dot means public
  });

  // ── The styling itself, not just the text ──
  //
  // The two assertions above pass unchanged against a Public pill painted the
  // design's way — `border-border-subtle bg-transparent text-text-subtle` —
  // which measures 1.00:1 against its own ground in parchment:dark (the
  // hairline and the surface are literally the same colour) and drops the
  // label under AA on three of six scopes' hover rows. So does the whole
  // contrast guard, which never opens this file. These do not.

  it('paints Private as the marked state: the fill, the strong ink, the glyph, the word', () => {
    const el = badgeOf(<PrivacyBadge tier="private" />)!;
    expect(el.getAttribute('data-privacy')).toBe('private');
    expect(el).toHaveTextContent('Private');
    expect(el.className).toContain('bg-background-muted');
    expect(el.className).toContain('text-text-default');
    expect(el.className).not.toContain('text-text-muted');
    expect(el.querySelector('svg')).not.toBeNull();
  });

  it('paints Public as the quiet state: same fill, quiet ink, no glyph, no outline', () => {
    const el = badgeOf(<PrivacyBadge tier="public" />)!;
    expect(el.getAttribute('data-privacy')).toBe('public');
    expect(el).toHaveTextContent('Public');
    expect(el.className).toContain('bg-background-muted');
    expect(el.className).toContain('text-text-muted');
    expect(el.className).not.toContain('text-text-default');
    // Quiet, not absent: a badge on everything trains people to stop seeing
    // badges, but Public still names itself in words.
    expect(el.querySelector('svg')).toBeNull();
    // And it is a FILL, never an outline. No neutral resting border token in
    // this system clears 1.6:1 against either badge ground, so the design's
    // hairline pill would have shipped invisible.
    const classes = el.className.split(/\s+/);
    expect(classes.filter((c) => c.startsWith('border'))).toEqual([]);
    expect(classes).not.toContain('bg-transparent');
  });

  it('keeps the two tiers visually distinct, so the badge is not decoration', () => {
    const priv = badgeOf(<PrivacyBadge tier="private" />)!;
    const pub = badgeOf(<PrivacyBadge tier="public" />)!;
    expect(priv.className).not.toEqual(pub.className);
    expect(priv.textContent).not.toEqual(pub.textContent);
  });

  it('rides Badge for the pill geometry in both tiers, restating none of it', () => {
    const source = read('src', 'components', 'ui', 'PrivacyBadge.tsx');
    for (const el of [
      badgeOf(<PrivacyBadge tier="private" />)!,
      badgeOf(<PrivacyBadge tier="public" />)!,
    ]) {
      for (const geometry of ['rounded-sm', 'px-1.5', 'py-0.5', 'text-[11px]', 'inline-flex']) {
        expect(el.className, `the pill lost Badge's ${geometry}`).toContain(geometry);
      }
    }
    // …and none of it is written down a second time here, which is the drift
    // badge.tsx's own doc-comment exists to prevent. `rounded-full` on the dot
    // is not Badge geometry and is expected.
    expect(source).toMatch(/from '\.\/badge'/);
    expect(source).not.toMatch(/rounded-sm|px-1\.5|py-0\.5|text-\[11px\]/);
  });

  // ── The tokens, checked against the stylesheet and against the guard ──

  it('names only colour tokens main.css actually declares', () => {
    for (const { name, tier, dense } of VISIBLE_STATES) {
      const tokens = colourTokensOf(badgeOf(<PrivacyBadge tier={tier} dense={dense} />)!.className);
      expect(tokens.length, `${name} names no colour token at all`).toBeGreaterThan(0);
      for (const t of tokens) {
        expect(DECLARED_TOKENS.has(t), `${name} names ${t}, which main.css never declares`).toBe(
          true
        );
      }
    }
  });

  it('names only colour tokens check-contrast.mjs measures for this badge', () => {
    expect(
      PRIVACY_ASSERTION_LINES.length,
      'the guard no longer carries assertions labelled `privacy …`'
    ).toBeGreaterThanOrEqual(4);
    for (const { name, tier, dense } of VISIBLE_STATES) {
      for (const t of colourTokensOf(
        badgeOf(<PrivacyBadge tier={tier} dense={dense} />)!.className
      )) {
        expect(
          MEASURED_TOKENS.has(t),
          `${name} paints ${t}, which no privacy assertion in check-contrast.mjs measures`
        ).toBe(true);
      }
    }
  });

  // ── The dense dot ──

  it('gives the dense dot a box, so it cannot render at zero size', () => {
    const classes = badgeOf(<PrivacyBadge tier="private" dense />)!.className.split(/\s+/);
    // `width` and `height` do not apply to a non-replaced INLINE element, and a
    // `<span>` is inline by default. `h-1.5 w-1.5` on its own therefore paints
    // nothing at all unless whichever parent mounts it happens to be a flex or
    // grid container that blockifies it. That is this component's own failure
    // mode — an indicator that is silently invisible, passing every screenshot
    // review — handed to its callers to get right.
    expect(classes).toContain('inline-block');
    expect(classes).toContain('h-1.5');
    expect(classes).toContain('w-1.5');
    // Dense surfaces are tight by definition — that is why they are dense. A
    // flex child with no shrink guard is the first thing squeezed to nothing,
    // and these are exactly the rows the dot was made for.
    expect(classes).toContain('shrink-0');
  });

  it('marks a dense Private row with a dot that carries its own name', () => {
    const dot = badgeOf(<PrivacyBadge tier="private" dense />)!;
    expect(dot.getAttribute('data-privacy')).toBe('private');
    expect(dot.textContent).toBe('');
    expect(dot.className).toContain('rounded-full');
    expect(dot.className).toContain('bg-text-default');
    // Colour and shape alone carry the meaning at this size, so the dot owes an
    // accessible name. A bare <span> maps to role `generic`, which takes none —
    // `title` by itself would leave it both invisible and unannounced.
    expect(dot.getAttribute('role')).toBe('img');
    expect(dot.getAttribute('aria-label')).toBe('Private chat');
  });

  it('passes a caller className through in both dense and full mode', () => {
    expect(badgeOf(<PrivacyBadge tier="public" className="ml-2" />)!.className).toContain('ml-2');
    expect(badgeOf(<PrivacyBadge tier="private" dense className="ml-2" />)!.className).toContain(
      'ml-2'
    );
  });
});
