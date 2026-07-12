import { render, screen, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import WorkflowsView from './WorkflowsView';

const mocks = vi.hoisted(() => ({
  listSavedWorkflows: vi.fn(),
}));

vi.mock('../../workflow/workflow_management', () => ({
  listSavedWorkflows: mocks.listSavedWorkflows,
  convertToLocaleDateString: () => 'Jul 11, 2026',
}));

vi.mock('../../hooks/useNavigation', () => ({ useNavigation: () => vi.fn() }));
vi.mock('../../contexts/DashboardContext', () => ({
  useDashboard: () => ({ spawnWindow: vi.fn() }),
}));
vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => children,
}));

beforeEach(() => {
  vi.clearAllMocks();
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
      'Create a reusable workflow here, save one from a conversation, or import an existing workflow.'
    );
    expect(screen.getByRole('button', { name: 'Create workflow' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Import workflow' })).toBeInTheDocument();
  });
});
