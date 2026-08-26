import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import WorkflowsView from './WorkflowsView';

const mocks = vi.hoisted(() => ({
  listSavedWorkflows: vi.fn(),
  refreshConfig: vi.fn(),
  setWorkflowSlashCommand: vi.fn(),
  startAgent: vi.fn(),
  setView: vi.fn(),
  userActionHeaders: vi.fn(),
}));

vi.mock('../../workflow/workflow_management', () => ({
  listSavedWorkflows: mocks.listSavedWorkflows,
  convertToLocaleDateString: () => 'Jul 11, 2026',
}));

vi.mock('../../hooks/useNavigation', () => ({ useNavigation: () => mocks.setView }));
vi.mock('../../api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api')>()),
  setWorkflowSlashCommand: mocks.setWorkflowSlashCommand,
  startAgent: mocks.startAgent,
}));
vi.mock('../../utils/userAction', () => ({ userActionHeaders: mocks.userActionHeaders }));
vi.mock('../../utils/workingDir', () => ({ getInitialWorkingDir: () => '/tmp/workspace' }));
vi.mock('../ConfigContext', () => ({
  useConfig: () => ({ refreshConfig: mocks.refreshConfig }),
}));
vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => children,
}));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.refreshConfig.mockResolvedValue(undefined);
  mocks.setWorkflowSlashCommand.mockResolvedValue({ data: undefined });
  mocks.userActionHeaders.mockResolvedValue({ 'X-User-Action': 'proof-of-user' });
});

describe('WorkflowsView loading transition', () => {
  it('shows its skeleton immediately and reveals loaded workflows without a blank timer gap', async () => {
    let finishLoad: ((value: unknown[]) => void) | undefined;
    mocks.listSavedWorkflows.mockReturnValueOnce(
      new Promise<unknown[]>((resolve) => {
        finishLoad = resolve;
      })
    );

    const { container } = render(
      <MemoryRouter>
        <WorkflowsView />
      </MemoryRouter>
    );

    expect(container.querySelectorAll('[data-slot="skeleton"]').length).toBeGreaterThan(0);
    finishLoad?.([
      {
        id: 'workflow-1',
        file_path: '/tmp/workflow.yaml',
        last_modified: '2026-07-11',
        workflow: { title: 'Cohort Review', description: 'Review cohort results' },
      },
    ]);

    expect(await screen.findByText('Cohort Review')).toBeInTheDocument();
    await waitFor(() => {
      expect(container.querySelector('[data-slot="skeleton"]')).not.toBeInTheDocument();
    });
  });

  it('refreshes the config cache after writing a workflow slash command', async () => {
    mocks.listSavedWorkflows.mockResolvedValue([
      {
        id: 'workflow-1',
        file_path: '/tmp/workflow.yaml',
        last_modified: '2026-07-11',
        workflow: { title: 'Cohort Review', description: 'Review cohort results' },
      },
    ]);

    render(
      <MemoryRouter>
        <WorkflowsView />
      </MemoryRouter>
    );

    fireEvent.click(await screen.findByTitle('Add slash command'));
    fireEvent.change(screen.getByPlaceholderText('command-name'), {
      target: { value: 'cohort-review' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(mocks.setWorkflowSlashCommand).toHaveBeenCalledOnce());
    expect(mocks.refreshConfig).toHaveBeenCalledOnce();
  });

  it('presents an accessible empty state with create and import actions', async () => {
    mocks.listSavedWorkflows.mockResolvedValueOnce([]);

    render(
      <MemoryRouter>
        <WorkflowsView />
      </MemoryRouter>
    );

    const title = await screen.findByRole('heading', { name: 'No workflows yet' });
    const emptyState = title.closest('section');

    expect(emptyState).toHaveAccessibleDescription(
      'Create a reusable workflow here, save one from a chat, or import an existing workflow.'
    );
    expect(screen.getByRole('button', { name: 'Create workflow' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Import workflow' })).toBeInTheDocument();
  });

  it('proves that starting a workflow came from the renderer user', async () => {
    const workflow = { title: 'Cohort Review', description: 'Review cohort results' };
    mocks.listSavedWorkflows.mockResolvedValueOnce([
      {
        id: 'workflow-1',
        file_path: '/tmp/workflow.yaml',
        last_modified: '2026-07-11',
        workflow,
      },
    ]);
    mocks.startAgent.mockResolvedValueOnce({ data: { id: 'workflow-session' } });

    render(
      <MemoryRouter>
        <WorkflowsView />
      </MemoryRouter>
    );

    fireEvent.click(await screen.findByTitle('Use workflow'));

    await waitFor(() => {
      expect(mocks.startAgent).toHaveBeenCalledWith({
        body: { working_dir: '/tmp/workspace', workflow },
        headers: { 'X-User-Action': 'proof-of-user' },
        throwOnError: true,
      });
    });
    expect(mocks.setView).toHaveBeenCalledWith('pair', {
      disableAnimation: true,
      resumeSessionId: 'workflow-session',
    });
  });
});
