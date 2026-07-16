import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import glyphUrl from '../../images/glyph.svg';
import { BioRouter } from './BioRouter';

describe('BioRouter mark', () => {
  it('renders the canonical Biorouter glyph asset', () => {
    render(<BioRouter className="size-8" />);

    const mark = screen.getByRole('img', { name: 'Biorouter logo' });
    expect(mark).toHaveClass('size-8');
    expect(mark.style.getPropertyValue('--biorouter-glyph')).toContain(glyphUrl);
    expect(mark).toHaveStyle({ display: 'block' });
  });
});
