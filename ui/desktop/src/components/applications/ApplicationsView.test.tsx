import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import ApplicationsView, { ApplicationItem } from './ApplicationsView';

vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock('../../toasts', () => ({
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
}));

const app = {
  id: 'cohort-explorer',
  title: 'Cohort Explorer',
  description: 'Explore a paired cohort.',
  kind: 'agentic' as const,
  session_id: 'session-1',
};

describe('ApplicationItem', () => {
  it('keeps every action keyboard-accessible when hover controls are visually collapsed', async () => {
    const user = userEvent.setup();
    const onLaunch = vi.fn();

    render(
      <ApplicationItem
        app={app}
        onLaunch={onLaunch}
        onOpenConversation={vi.fn()}
        onExport={vi.fn()}
        onDelete={vi.fn()}
      />
    );

    const launch = screen.getByRole('button', { name: 'Launch Cohort Explorer in browser' });
    expect(
      screen.getByRole('button', {
        name: 'Open the conversation where Cohort Explorer was built',
      })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Export Cohort Explorer to a folder' })
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete Cohort Explorer' })).toBeInTheDocument();
    expect(launch.parentElement).toHaveClass('sm:group-focus-within:opacity-100');

    await user.tab();
    expect(launch).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(onLaunch).toHaveBeenCalledTimes(1);
  });

  it('disables duplicate launch and export actions while they are in flight', () => {
    const { container } = render(
      <ApplicationItem
        app={app}
        onLaunch={vi.fn()}
        onOpenConversation={vi.fn()}
        onExport={vi.fn()}
        onDelete={vi.fn()}
        isLaunching
        isExporting
      />
    );

    expect(
      screen.getByRole('button', { name: 'Launch Cohort Explorer in browser' })
    ).toBeDisabled();
    expect(
      screen.getByRole('button', { name: 'Export Cohort Explorer to a folder' })
    ).toBeDisabled();
    expect(container.firstElementChild).toHaveAttribute('aria-busy', 'true');
  });

  it('shows theme-pack swatch and surface badges for a v2 manifest', () => {
    render(
      <ApplicationItem
        app={{
          ...app,
          theme: { pack: 'midnight' },
          surface: {
            actions: [{ name: 'focus_node' }, { name: 'reset_view' }],
            signals: [{ name: 'node_selected' }],
          },
        }}
        onLaunch={vi.fn()}
        onOpenConversation={vi.fn()}
        onExport={vi.fn()}
        onDelete={vi.fn()}
      />
    );

    const themeBadge = screen.getByText('midnight');
    expect(themeBadge).toBeInTheDocument();
    expect(themeBadge.querySelector('[aria-hidden="true"]')).toHaveClass(
      'app-theme-swatch--midnight'
    );
    expect(screen.getByText('2 actions · 1 signal')).toBeInTheDocument();
  });

  it('renders no v2 badges for a plain v1 manifest (or the default theme pack)', () => {
    render(
      <ApplicationItem
        app={{ ...app, theme: { pack: 'biorouter' } }}
        onLaunch={vi.fn()}
        onOpenConversation={vi.fn()}
        onExport={vi.fn()}
        onDelete={vi.fn()}
      />
    );

    expect(screen.queryByText('biorouter')).toBeNull();
    expect(screen.queryByText(/\d+ actions?/)).toBeNull();
    expect(screen.queryByText(/\d+ signals?/)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Export dialog (Apps SDK v2, design §3.9)
// ---------------------------------------------------------------------------

const exportableApp = {
  ...app,
  created_at: 1750000000,
  updated_at: 1750000000,
  agent: {
    model: { provider: 'anthropic', model: 'claude-sonnet-4-5' },
    extensions: ['spokeagent'],
    skills: ['ggplot-visualization'],
    knowledge_base: 'ms-cohort',
  },
};

const okJson = (body: unknown) =>
  ({ ok: true, status: 200, json: async () => body, text: async () => '' }) as Response;

describe('ApplicationsView export dialog', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let directoryChooser: ReturnType<typeof vi.fn>;
  let writeFile: ReturnType<typeof vi.fn>;

  const exportCalls = () =>
    fetchMock.mock.calls.map(([url]) => String(url)).filter((url) => url.includes('/export'));

  beforeEach(() => {
    vi.clearAllMocks();
    fetchMock = vi.fn(async (url: string) => {
      if (String(url).includes('/export')) {
        return okJson({ id: exportableApp.id, files: { 'run.sh': '#!/bin/sh' } });
      }
      return okJson([exportableApp]);
    });
    vi.stubGlobal('fetch', fetchMock);
    directoryChooser = vi.fn().mockResolvedValue({ canceled: false, filePaths: ['/tmp/dest'] });
    writeFile = vi.fn().mockResolvedValue(true);
    window.electron = {
      platform: 'darwin',
      getSecretKey: vi.fn().mockResolvedValue('test-secret'),
      openExternal: vi.fn().mockResolvedValue(undefined),
      directoryChooser,
      writeFile,
    } as unknown as typeof window.electron;
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  async function openExportDialog() {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <ApplicationsView />
      </MemoryRouter>
    );
    await user.click(
      await screen.findByRole('button', { name: 'Export Cohort Explorer to a folder' })
    );
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    return user;
  }

  it('opens a dialog on Export with launcher mode selected and the credentials note', async () => {
    await openExportDialog();

    expect(screen.getByRole('radio', { name: /Launcher \(app \+ scripts\)/ })).toBeChecked();
    expect(
      screen.getByRole('radio', { name: /Full \(bundle server-side content\)/ })
    ).not.toBeChecked();
    expect(screen.getByText(/Credentials never travel/)).toBeInTheDocument();
    // No request happens just from opening the dialog.
    expect(exportCalls()).toHaveLength(0);
  });

  it('launcher mode requests mode=launcher with no include and keeps the download behavior', async () => {
    const user = await openExportDialog();

    await user.click(screen.getByRole('button', { name: 'Export' }));

    await waitFor(() => expect(exportCalls()).toHaveLength(1));
    const url = new URL(exportCalls()[0]);
    expect(url.pathname).toBe('/apps/cohort-explorer/export');
    expect(url.searchParams.get('mode')).toBe('launcher');
    expect(url.searchParams.get('bundle_daemon')).toBe('none');
    expect(url.searchParams.has('include')).toBe(false);

    // Today's save flow is preserved: pick a directory, write the scaffold.
    await waitFor(() => expect(writeFile).toHaveBeenCalled());
    expect(directoryChooser).toHaveBeenCalledTimes(1);
    expect(writeFile).toHaveBeenCalledWith('/tmp/dest/cohort-explorer/run.sh', '#!/bin/sh');
  });

  it('full mode pre-checks manifest grants and carries mode=full plus the include JSON', async () => {
    const user = await openExportDialog();

    await user.click(screen.getByRole('radio', { name: /Full \(bundle server-side content\)/ }));

    // Pre-checked from the app's agent config.
    expect(screen.getByRole('checkbox', { name: /Knowledge base: ms-cohort/ })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: /Skill: ggplot-visualization/ })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: /Extension: spokeagent/ })).toBeChecked();
    const daemon = screen.getByRole('checkbox', { name: /Bundle daemon binary/ });
    expect(daemon).not.toBeChecked();
    expect(screen.getByText(/knowledge base carries its sources/)).toBeInTheDocument();

    await user.click(daemon);
    await user.click(screen.getByRole('button', { name: 'Export' }));

    await waitFor(() => expect(exportCalls()).toHaveLength(1));
    const url = new URL(exportCalls()[0]);
    expect(url.searchParams.get('mode')).toBe('full');
    expect(url.searchParams.get('bundle_daemon')).toBe('current');
    expect(JSON.parse(url.searchParams.get('include') ?? '{}')).toEqual({
      knowledge_bases: ['ms-cohort'],
      skills: ['ggplot-visualization'],
      extensions: ['spokeagent'],
    });
  });

  it('unchecking an include item drops it from the include JSON', async () => {
    const user = await openExportDialog();

    await user.click(screen.getByRole('radio', { name: /Full \(bundle server-side content\)/ }));
    await user.click(screen.getByRole('checkbox', { name: /Knowledge base: ms-cohort/ }));
    await user.click(screen.getByRole('button', { name: 'Export' }));

    await waitFor(() => expect(exportCalls()).toHaveLength(1));
    const url = new URL(exportCalls()[0]);
    expect(JSON.parse(url.searchParams.get('include') ?? '{}')).toEqual({
      skills: ['ggplot-visualization'],
      extensions: ['spokeagent'],
    });
  });

  it('cancel closes the dialog and sends nothing', async () => {
    const user = await openExportDialog();

    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(exportCalls()).toHaveLength(0);
    expect(directoryChooser).not.toHaveBeenCalled();
  });
});
