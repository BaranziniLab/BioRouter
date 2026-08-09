import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { AffiliationBadge } from './AffiliationBadge';
import type { ProviderAffiliation } from '../privacy/providerAffiliation';

afterEach(cleanup);

const ucsf: ProviderAffiliation = {
  kind: 'institutions',
  institutions: [{ id: 'ucsf', display_name: 'UCSF' }],
};
const local: ProviderAffiliation = { kind: 'local', institutions: [] };
const unstated: ProviderAffiliation = { kind: 'unstated', institutions: [] };

describe('AffiliationBadge', () => {
  /**
   * A public model has no affiliation, so the badge is not merely blank — it is
   * absent, and the surrounding row collapses. A blank chip would read as a
   * constraint on a model that has none.
   */
  it('renders nothing at all when there is no affiliation', () => {
    const { container } = render(<AffiliationBadge affiliation={null} />);
    expect(container).toBeEmptyDOMElement();
    cleanup();
    const undefinedCase = render(<AffiliationBadge affiliation={undefined} />);
    expect(undefinedCase.container).toBeEmptyDOMElement();
  });

  it('names the institution and marks the kind for a covered model', () => {
    render(<AffiliationBadge affiliation={ucsf} />);
    const badge = screen.getByTestId('affiliation-badge');
    expect(badge).toHaveAttribute('data-affiliation', 'institutions');
    expect(badge).toHaveTextContent('UCSF');
  });

  /**
   * ⚠ **The inversion, at the badge.** `local` must not render in the
   * institution's visual family: it is the MOST permissive affiliation, and a
   * landmark-marked chip beside `UCSF` invites "an institution, but narrower".
   * The three kinds are distinguishable by their `data-affiliation` marker AND
   * by carrying different glyph elements.
   */
  it('marks local as its own kind, distinct from an institution', () => {
    const { container: localMark } = render(<AffiliationBadge affiliation={local} />);
    const localBadge = screen.getByTestId('affiliation-badge');
    expect(localBadge).toHaveAttribute('data-affiliation', 'local');
    expect(localBadge).toHaveTextContent('On this machine');
    const localGlyph = localMark.querySelector('svg')?.getAttribute('class');
    cleanup();

    const { container: instMark } = render(<AffiliationBadge affiliation={ucsf} />);
    const instGlyph = instMark.querySelector('svg')?.getAttribute('class');

    expect(localGlyph).toBeTruthy();
    expect(instGlyph).toBeTruthy();
    expect(localGlyph).not.toEqual(instGlyph);
  });

  /**
   * ⚠ **No tone, weight or colour varies across the three kinds**, because any
   * variation is read as a ranking and the ranking people infer — "more
   * specific means more restricted" — is backwards. The chips must be the same
   * surface; only the glyph and the word differ.
   */
  it('gives the three affiliations the same chip surface, never a scale', () => {
    const classes = [local, ucsf, unstated].map((affiliation) => {
      cleanup();
      render(<AffiliationBadge affiliation={affiliation} />);
      return screen.getByTestId('affiliation-badge').getAttribute('class');
    });
    expect(new Set(classes).size).toBe(1);
  });

  /**
   * `unstated` is a fact, not an absence — it is the least-reaching private
   * model, and an empty-looking chip would show it as the least constrained.
   */
  it('states an unstated institution rather than rendering an empty chip', () => {
    render(<AffiliationBadge affiliation={unstated} />);
    const badge = screen.getByTestId('affiliation-badge');
    expect(badge).toHaveAttribute('data-affiliation', 'unstated');
    expect(badge.textContent?.trim()).toBe('No stated institution');
  });

  /**
   * The dense form is what the composer chip can fit. A glyph alone is silent to
   * a screen reader unless it is given a role that takes an accessible name —
   * the same trap `PrivacyBadge`'s dense dot documents.
   */
  it('keeps the dense glyph announced and per-kind', () => {
    render(<AffiliationBadge affiliation={ucsf} dense />);
    const badge = screen.getByTestId('affiliation-badge');
    expect(badge).toHaveAttribute('role', 'img');
    expect(badge.getAttribute('aria-label')).toContain('UCSF');
    expect(badge).toHaveAttribute('data-affiliation', 'institutions');
    // No visible text in the dense form; the words live in the label and title.
    expect(badge.textContent?.trim()).toBe('');
    expect(badge.getAttribute('title')).toContain('Compliance does not transfer');
  });
});
