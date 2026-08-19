/**
 * "Install all" must run installers ONE AT A TIME.
 *
 * `dep:install` resolves as soon as the child is spawned, not when it finishes,
 * so the obvious `await window.electron.installDependency(...)` in a loop starts
 * every installer at once — two package managers fighting over one lock, and
 * interleaved output in the panes. The batch instead waits on the terminal push
 * event for each dependency.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { DependencyEvent, DependencyInfo } from '../utils/dependencyChecker';

vi.mock('./ModalShell', () => ({
  ModalShell: ({ children, footer }: { children: React.ReactNode; footer?: React.ReactNode }) => (
    <div>
      {children}
      {footer}
    </div>
  ),
}));

import DependencySetupModal from './DependencySetupModal';

const installDependency = vi.fn();
let emit: (e: DependencyEvent) => void = () => {};

function dep(name: string): DependencyInfo {
  return {
    name,
    displayName: name,
    version: null,
    installed: false,
    installCmd: `install ${name}`,
    requiresSudo: false,
    downloadUrl: '',
    required: true,
  };
}

beforeEach(() => {
  installDependency.mockReset().mockResolvedValue({ started: true });
  // A partial stub — only what this modal touches. Cast rather than
  // `@ts-expect-error`, which suppresses the NEXT LINE only and so cannot cover
  // a multi-line object literal.
  window.electron = {
    on: (_channel: string, handler: (e: unknown, ...args: unknown[]) => void) => {
      emit = (payload: DependencyEvent) => handler({}, payload);
      return () => {};
    },
    cliStatus: async () => null,
    installDependency,
    openExternal: vi.fn(),
    createChatWindow: vi.fn(),
    dependencyEnvironment: async () => ({}),
  } as unknown as typeof window.electron;
});

describe('Install all', () => {
  it('starts the next installer only after the previous one finishes', async () => {
    render(<DependencySetupModal />);
    emit({ type: 'check-results', deps: [dep('git'), dep('uv'), dep('npm')] });

    const button = await screen.findByRole('button', { name: /install all \(3\)/i });
    await userEvent.click(button);

    // One installer running, not three.
    await waitFor(() => expect(installDependency).toHaveBeenCalledTimes(1));
    expect(installDependency).toHaveBeenLastCalledWith('git');

    // Nothing else starts while the first is still going.
    await new Promise((r) => setTimeout(r, 20));
    expect(installDependency).toHaveBeenCalledTimes(1);

    emit({ type: 'install-done', dep: 'git', installed: true, version: '2.50' });
    await waitFor(() => expect(installDependency).toHaveBeenCalledTimes(2));
    expect(installDependency).toHaveBeenLastCalledWith('uv');

    // A FAILED install must not stall the rest of the batch.
    emit({ type: 'install-error', dep: 'uv', error: 'no network' });
    await waitFor(() => expect(installDependency).toHaveBeenCalledTimes(3));
    expect(installDependency).toHaveBeenLastCalledWith('npm');
  });

  it('offers the debug session on the one that failed', async () => {
    render(<DependencySetupModal />);
    emit({ type: 'check-results', deps: [dep('git'), dep('uv')] });
    await screen.findByText('uv');

    emit({ type: 'install-error', dep: 'uv', error: 'could not resolve host' });

    await screen.findByText('could not resolve host');
    const debugButtons = await screen.findAllByRole('button', { name: /debug with biorouter/i });
    // Exactly one — the failure, not every row.
    expect(debugButtons).toHaveLength(1);
  });

  it('counts only dependencies that have an automated installer', async () => {
    render(<DependencySetupModal />);
    emit({
      type: 'check-results',
      // `rust` here has no install command — a "use your package manager"
      // placeholder — so batching it would click a button that does nothing.
      deps: [dep('git'), dep('uv'), { ...dep('rust'), installCmd: '' }],
    });
    expect(await screen.findByRole('button', { name: /install all \(2\)/i })).toBeInTheDocument();
  });

  it('does not offer a batch for a single dependency', async () => {
    render(<DependencySetupModal />);
    emit({ type: 'check-results', deps: [dep('git')] });
    await screen.findByText('git');
    // The row's own Install button is the whole batch; a second control saying
    // "Install all (1)" beside it is noise.
    expect(screen.queryByRole('button', { name: /install all/i })).toBeNull();
  });
});
