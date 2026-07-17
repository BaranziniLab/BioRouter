import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { BioRouter } from './BioRouter';

describe('BioRouter mark', () => {
  it('renders the canonical Biorouter glyph without an external mask', () => {
    render(<BioRouter className="size-8" />);

    const mark = screen.getByRole('img', { name: 'Biorouter logo' });
    expect(mark).toHaveClass('size-8');
    expect(mark.tagName).toBe('svg');
    expect(mark).toHaveAttribute('viewBox', '110 55 380 330');
    expect(mark).toHaveAttribute('fill', 'currentColor');
    expect(mark.style.backgroundColor).toBe('');
    expect(mark.style.maskImage).toBe('');
    expect(mark.querySelectorAll('path')).toHaveLength(7);
    expect(mark.querySelectorAll('ellipse')).toHaveLength(3);
  });
});
