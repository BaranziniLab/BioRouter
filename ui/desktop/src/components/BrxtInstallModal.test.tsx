import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { BrxtManifest } from '../types/brxt';

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

const MANIFEST: BrxtManifest = {
  name: 'playwrightagent',
  display_name: 'Playwright Agent',
  description: 'Browser automation via Playwright.',
  version: '0.1.0',
  entry_point: 'main.py',
  repository: 'https://github.com/BaranziniLab/PlaywrightAgent',
  tools_count: 23,
  env_vars: [],
};

afterEach(() => {
  cleanup();
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
      screen.getByText(/including commercial models hosted outside your institution/i)
    ).toBeInTheDocument();
  });

  /**
   * ⚠ **This modal is not only the file-drop route.** `BrowseExtensionsModal`
   * downloads a marketplace `.brxt` and then renders THIS component — so the
   * browse row can show a Private badge, the install confirmation say "always
   * Public", and Settings badge it Private afterwards. Three screens, two
   * answers, one install.
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

  /**
   * Issue #116. The marketplace row's badge is the only tier available while
   * the bundle is still downloading, and the installer opens on top of that
   * row — so it must not answer the question differently from the screen the
   * user just clicked.
   */
  it('carries the marketplace row badge through the download, before any manifest exists', () => {
    render(
      <BrxtInstallModal
        onClose={() => {}}
        onInstalled={() => {}}
        origin={{
          kind: 'marketplace',
          registrySource: { registryId: 'ucsfomopagent-1.0.0' },
          entry: { name: 'UCSF OMOP', privacyTier: 'private' },
          downloading: true,
          onRetry: () => {},
        }}
      />
    );

    expect(screen.getByText(/only private models/i)).toBeInTheDocument();
    expect(screen.queryByText(/always Public/i)).toBeNull();
  });
});

/**
 * Issue #116. The local route is the half that MUST keep its file controls, and
 * these assertions are the reason the marketplace suite's absence-checks are
 * worth anything: the same strings are found here and refused there.
 */
describe('BrxtInstallModal — the local-file route (issue #116)', () => {
  it('keeps drag/drop and the file picker when no origin is given', () => {
    render(<BrxtInstallModal onClose={() => {}} onInstalled={() => {}} />);

    expect(screen.getByText('Add extension')).toBeInTheDocument();
    expect(screen.getByText('Drop your .brxt file here')).toBeInTheDocument();
    expect(screen.getByText('or click to browse')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Browse file…' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back to marketplace' })).toBeNull();
  });

  /**
   * ⚠ A preloaded path is NOT a marketplace install. Finder hands a
   * double-clicked `.brxt` to the app over IPC and `ExtensionsView` preloads
   * it, so deriving the mode from the path — the obvious shortcut — would strip
   * the file controls off the one route that needs them and leave the user with
   * no way to correct a mis-picked file.
   */
  it('stays on the local route for an IPC-preloaded file, offering to replace it', async () => {
    (window as unknown as { electron: unknown }).electron = {
      validateBrxtBundle: vi.fn(async () => ({ manifest: MANIFEST, skillsPreview: [] })),
    };

    render(
      <BrxtInstallModal
        onClose={() => {}}
        onInstalled={() => {}}
        preloadedFilePath="/tmp/playwright.brxt"
      />
    );

    expect(await screen.findByText('Detected from bundle')).toBeInTheDocument();
    expect(screen.getByText('Add extension')).toBeInTheDocument();
    expect(screen.getByText('Drop a different .brxt file here')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Browse file…' })).toBeInTheDocument();
  });

  it('labels the local primary action by what it will actually do', async () => {
    const install = vi.fn(async () => ({ installDir: '/ext/playwrightagent' }));
    (window as unknown as { electron: unknown }).electron = {
      validateBrxtBundle: vi.fn(async () => ({ manifest: MANIFEST, skillsPreview: [] })),
      installBrxtBundle: install,
    };

    render(
      <BrxtInstallModal
        onClose={() => {}}
        onInstalled={() => {}}
        preloadedFilePath="/tmp/playwright.brxt"
      />
    );

    // Zero env vars: the button installs, so it says so.
    const button = await screen.findByRole('button', { name: 'Install extension' });
    expect(screen.queryByRole('button', { name: /Next: configure/i })).toBeNull();

    await userEvent.setup().click(button);
    await waitFor(() => expect(install).toHaveBeenCalled());
    // The local route records no registry provenance (DR-23): there is none.
    expect(install).toHaveBeenCalledWith('/tmp/playwright.brxt', 'playwrightagent', undefined);
  });
});
