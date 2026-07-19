import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { ThemeProvider, THEME_FAMILIES } from '../../contexts/ThemeContext';
import ThemeFamilySelector from './ThemeFamilySelector';

/**
 * The selector is the only way a user reaches a theme family, so a family that
 * exists in `THEME_FAMILIES` but is missing here is invisible and effectively
 * unshipped. These tests derive from the registry rather than hardcoding names,
 * so adding a family without adding its button fails here.
 */
function renderSelector() {
  return render(
    <ThemeProvider>
      <ThemeFamilySelector />
    </ThemeProvider>
  );
}

describe('ThemeFamilySelector', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
  });

  it('renders a button for every registered theme family', () => {
    renderSelector();
    for (const family of THEME_FAMILIES) {
      expect(
        screen.getByTestId(`theme-family-${family}-button`),
        `no button for "${family}" — the family is unreachable from the UI`
      ).toBeInTheDocument();
    }
    // and nothing beyond the registry
    expect(screen.getAllByTestId(/^theme-family-.*-button$/)).toHaveLength(THEME_FAMILIES.length);
  });

  it('includes Roche Limit and labels it', () => {
    renderSelector();
    expect(screen.getByTestId('theme-family-roche-limit-button')).toHaveTextContent('Roche Limit');
  });

  it.each(THEME_FAMILIES)('selecting %s writes data-theme and persists it', (family) => {
    renderSelector();
    fireEvent.click(screen.getByTestId(`theme-family-${family}-button`));
    expect(document.documentElement.getAttribute('data-theme')).toBe(family);
    expect(localStorage.getItem('theme_family')).toBe(family);
  });

  // The grid is sized in a hardcoded Tailwind class, so it does not grow with
  // the registry. If a family is added and the column count is not bumped, the
  // buttons wrap into a ragged extra row.
  it('sizes the grid to the number of families', () => {
    const { container } = renderSelector();
    const grid = container.querySelector('[class*="grid-cols-"]');
    expect(grid, 'expected a grid container').not.toBeNull();
    expect(grid?.className).toContain(`grid-cols-${THEME_FAMILIES.length}`);
  });
});
