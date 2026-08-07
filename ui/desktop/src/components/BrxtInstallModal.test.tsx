import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

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

afterEach(() => {
  delete (window as unknown as { electron?: unknown }).electron;
});

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

  /**
   * ⚠ **This modal is not only the file-drop route.** `BrowseExtensionsModal`
   * downloads a marketplace `.brxt` and then renders THIS component with
   * `preloadedFilePath` — so the browse row can show a Private badge, the
   * install confirmation say "always Public", and Settings badge it Private
   * afterwards. Three screens, two answers, one install.
   *
   * The task's own Step 3 names the other half of the same contradiction: a
   * hand-installed extension NAMED `ucsfomopagent` inherits the private badge,
   * "fail-closed, and fine" — which is only fine if the modal does not promise
   * the opposite first.
   *
   * The disclosure is therefore about the RESULT and not about the route, and
   * once a manifest is in hand the result is knowable: `classifyExtension` is
   * the same union every other privacy surface in the app reads.
   */
  it('says Private when that is the badge the install will actually produce', async () => {
    (window as unknown as { electron: unknown }).electron = {
      validateBrxtBundle: vi.fn(async () => ({
        manifest: {
          name: 'ucsfomopagent',
          display_name: 'UCSF OMOP',
          description: 'OMOP clinical database',
          version: '1.0.0',
          entry_point: 'main.py',
          repository: 'https://github.com/BaranziniLab/UCSFOMOPAgent',
          env_vars: [],
        },
        skillsPreview: [],
      })),
    };

    render(
      <BrxtInstallModal
        onClose={() => {}}
        onInstalled={() => {}}
        preloadedFilePath="/tmp/ucsfomopagent.brxt"
      />
    );

    expect(await screen.findByText(/only private models/i)).toBeInTheDocument();
    expect(screen.queryByText(/always Public/i)).toBeNull();
  });
});
