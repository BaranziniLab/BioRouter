import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { PrivacyBadge } from './PrivacyBadge';

afterEach(cleanup);

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
});
