import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * The Knowledge section's three structural tokens, and the three registries each
 * of them has to join (knowledge-base/okf-migration/ui-spec.md §6.2).
 *
 * ⚠ **jsdom can see none of this.** It has no layout engine and never runs
 * Tailwind, so a component test renders identically whether `w-knowledge-rail-detail`
 * generated a rule or nothing at all. The only assertable thing is the
 * declaration and its registrations, so that is what is asserted, at the source
 * — the same argument `measures.test.ts` makes for the two reading measures.
 *
 * Why all three registries and not just the `:root` declaration:
 *
 * - **`@theme inline`** is what turns a token into a UTILITY. Without the
 *   `--spacing-*` mirror, `w-knowledge-rail-detail` is not generated, the class
 *   silently does nothing, and the element takes whatever width it would have
 *   taken anyway. That is the exact failure `check-token-mirrors.mjs` exists for
 *   on the colour side, and it has shipped three times there.
 * - **`src/utils.ts`'s `spacing` list** is what makes tailwind-merge treat the
 *   name as part of the `w`/`h`/`max-w` class group. Omit it and a call-site
 *   override stops conflicting with `w-64`, and source order decides which one
 *   paints.
 * - **`STRUCTURAL_TOKENS`** is what makes `validateMode` REJECT a theme family
 *   that tries to theme a rail width. Nothing else stops a fourth family
 *   declaring geometry of its own — the same "nothing enforces the sharing"
 *   hole §5.1 closes for the canvas ground.
 */
const CSS = readFileSync(join(__dirname, 'main.css'), 'utf8');
const UTILS = readFileSync(join(__dirname, '../utils.ts'), 'utf8');
const CONTRACT = readFileSync(join(__dirname, '../../scripts/lib/theme-contract.mjs'), 'utf8');
const READABLE = readFileSync(join(__dirname, '../components/Layout/ReadableContent.tsx'), 'utf8');

const TOKENS = ['measure-graph', 'knowledge-rail-sources', 'knowledge-rail-detail'] as const;

describe('the Knowledge section geometry is tokenised', () => {
  it.each(TOKENS)('--%s is declared once in the hand-authored :root block', (token) => {
    const declarations = CSS.match(new RegExp(`^\\s*--${token}:`, 'gm')) ?? [];
    expect(declarations).toHaveLength(1);
  });

  it.each(TOKENS)('--%s has a --spacing-* mirror in @theme inline', (token) => {
    expect(CSS).toContain(`--spacing-${token}: var(--${token});`);
  });

  it.each(TOKENS)('%s is registered with tailwind-merge', (token) => {
    expect(UTILS).toContain(`'${token}'`);
  });

  it.each(TOKENS)('%s is structural, so no theme family may re-point it', (token) => {
    expect(CONTRACT).toContain(`'${token}'`);
  });

  /**
   * `graph` is Knowledge's measure and it must read the TOKEN, not restate the
   * clamp. It is the widest column in the app, sitting beside two measures that
   * are already named; an un-tokenised literal there is the drift
   * `--measure-chat` had to be rescued from once already.
   */
  it('ReadableContent size="graph" reads the token', () => {
    expect(READABLE).toContain("graph: 'max-w-measure-graph'");
  });

  /**
   * `--dock-height` is REUSED by the knowledge facet strip and legend dock, not
   * forked. Its comment used to claim it was the terminal's; the value never
   * moved, only the name stopped lying.
   */
  it('--dock-height still names the general pane-docked strip', () => {
    const match = CSS.match(/--dock-height:\s*([^;]+);/);
    expect(match?.[1].trim()).toBe('36px');
    expect(CSS).toContain('A pane-docked control strip');
  });
});
