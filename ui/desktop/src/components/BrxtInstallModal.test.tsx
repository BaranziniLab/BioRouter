import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

/**
 * `BrxtInstallModal` reaches for the live config on mount; nothing here installs
 * anything, so the context is stubbed down to the one member the component
 * closes over.
 */
vi.mock('./ConfigContext', () => ({
  useConfig: () => ({ addExtension: vi.fn() }),
  // `PrivacyBadge` reads the master switch off this same context and fails loudly
  // if the mock omits it — see the warning in `ui/PrivacyBadge.tsx`.
  usePrivacyTiersEnabled: () => true,
}));

import { BrxtInstallModal } from './BrxtInstallModal';

describe('BrxtInstallModal — issue #56 §13.5', () => {
  it('the brxt install modal says the resulting badge out loud', () => {
    // The plan wrote `render(<BrxtInstallModal manifest={{ name: 'anything' }} />)`.
    // There is no `manifest` prop — the component derives the manifest from the
    // dropped file — so the real props are passed instead. The assertions are
    // the plan's, unchanged: the disclosure has to be on screen BEFORE a file is
    // chosen, because "always Public" is a property of the install route, not of
    // whatever bundle the user is about to pick.
    render(<BrxtInstallModal onClose={() => {}} onInstalled={() => {}} />);
    expect(screen.getByText(/always Public/i)).toBeInTheDocument();
    expect(
      screen.getByText(/including commercial models hosted outside UCSF/i)
    ).toBeInTheDocument();
  });
});
