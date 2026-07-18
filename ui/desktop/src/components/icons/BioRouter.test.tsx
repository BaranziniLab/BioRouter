import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import glyphUrl from '../../images/glyph.svg';
import { BioRouter } from './BioRouter';

describe('BioRouter mark', () => {
  it('renders the canonical Biorouter glyph asset', () => {
    render(<BioRouter className="size-8" />);

    const mark = screen.getByRole('img', { name: 'Biorouter logo' });
    expect(mark).toHaveClass('size-8');
    expect(mark).toHaveStyle({ display: 'block' });
    expect(mark.style.maskImage).toContain(glyphUrl);
  });

  // The regression guard. The mark paints `currentColor` and relies on the mask
  // to cut its shape, so a dropped mask-image renders a SOLID BLOCK of the
  // accent colour — a failure that looks deliberate rather than broken.
  //
  // Vite inlines glyph.svg as a `data:` URI whose markup carries single quotes
  // and parentheses (`version='1.0'`, `transform='rotate(-90 434.5 177.25)'`).
  // An UNQUOTED url() token may not contain `(`, `'` or whitespace: the CSS
  // tokenizer emits a bad-url-token and the browser throws the whole
  // declaration away. jsdom's CSS parser is lenient and accepts it, so this
  // cannot be caught by asserting "the property is set" — the previous test
  // asserted exactly that against the old `--biorouter-glyph` custom property,
  // passed, and the logo was a solid square in the real app the whole time.
  // Assert the QUOTING, which is the thing Chrome actually requires.
  it('quotes the mask url so the data URI survives the CSS tokenizer', () => {
    render(<BioRouter />);

    const mark = screen.getByRole('img', { name: 'Biorouter logo' });
    expect(mark.style.maskImage).toMatch(/^url\("/);
    expect(mark.style.maskImage).toMatch(/"\)$/);
  });

  it('inlines the mask rather than routing it through a custom property', () => {
    render(<BioRouter />);

    const mark = screen.getByRole('img', { name: 'Biorouter logo' });
    // Nothing ever read this variable; the indirection only added a second way
    // for the declaration to be invalidated.
    expect(mark.style.getPropertyValue('--biorouter-glyph')).toBe('');
  });
});
