import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FixedExtensionEntry } from '../../../ConfigContext';
import ExtensionItem from './ExtensionItem';

describe('ExtensionItem', () => {
  it('shows an optimistic disabled state while the toggle request is pending', () => {
    const pending = new Promise<boolean>(() => undefined);
    const extension = {
      type: 'builtin',
      name: 'example',
      display_name: 'Example',
      description: 'Example extension',
      enabled: true,
    } as FixedExtensionEntry;

    render(<ExtensionItem extension={extension} onToggle={vi.fn(() => pending)} />);
    const toggle = screen.getByRole('switch', { name: 'Toggle Example extension' });

    fireEvent.click(toggle);

    expect(toggle).toHaveAttribute('aria-checked', 'false');
    expect(toggle).toBeDisabled();
  });
});
