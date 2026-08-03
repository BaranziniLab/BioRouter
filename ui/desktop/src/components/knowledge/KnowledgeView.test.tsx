import { fireEvent, render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import KnowledgeView from './KnowledgeView';

const mocks = vi.hoisted(() => ({
  refresh: vi.fn(),
}));

const state = vi.hoisted(() => ({
  primaryKb: null as { id: string; name: string; tier?: string } | null,
}));

vi.mock('./KnowledgeContext', () => ({
  useKnowledge: () => ({ refresh: mocks.refresh, primaryKb: state.primaryKb }),
}));

vi.mock('./KbTierControl', () => ({
  KbTierPanel: ({ kb }: { kb: { id: string } }) => <div>tier control for {kb.id}</div>,
}));

vi.mock('../Layout/MainPanelLayout', () => ({
  MainPanelLayout: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock('../Layout/ReadableContent', () => ({
  ReadableContent: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock('./KBSelector/KBSelectorTrigger', () => ({
  KBSelectorTrigger: () => <button type="button">Knowledge base</button>,
}));

vi.mock('./IngestPanel/IngestPanel', () => ({
  IngestPanel: () => <div>Digest workspace</div>,
}));

vi.mock('./graph/KnowledgeGraphPanel', () => ({
  KnowledgeGraphPanel: () => <div>Graph workspace</div>,
}));

vi.mock('./changelog/ChangeLogDrawer', () => ({
  ChangeLogDrawer: () => null,
}));

beforeEach(() => {
  vi.clearAllMocks();
  state.primaryKb = null;
});

describe('KnowledgeView compact workspace', () => {
  it('lets compact windows give the digest and graph their own workspace', () => {
    render(<KnowledgeView />);

    const digestTab = screen.getByRole('tab', { name: 'Digest' });
    const graphTab = screen.getByRole('tab', { name: 'Graph' });
    const digestPanel = screen.getByTestId('knowledge-digest-panel');
    const graphPanel = screen.getByTestId('knowledge-graph-panel');

    expect(graphTab).toHaveAttribute('aria-selected', 'true');
    expect(graphPanel).toHaveClass('flex');
    expect(digestPanel).toHaveClass('hidden');

    fireEvent.click(digestTab);

    expect(digestTab).toHaveAttribute('aria-selected', 'true');
    expect(digestPanel).toHaveClass('flex');
    expect(graphPanel).toHaveClass('hidden');

    fireEvent.click(graphTab);

    expect(graphTab).toHaveAttribute('aria-selected', 'true');
    expect(graphPanel).toHaveClass('flex');
    expect(digestPanel).toHaveClass('hidden');
  });
});

// Issue #56 DR-18. The tier control lives beside the base it acts on, in the
// KB header — not in a settings page, where a user reading a private base would
// never meet it.
describe('KnowledgeView tier control', () => {
  it('offers the tier control for the base the view is showing', () => {
    state.primaryKb = { id: 'omop', name: 'OMOP', tier: 'private' };
    render(<KnowledgeView />);
    expect(screen.getByText(/tier control for omop/)).toBeInTheDocument();
  });

  it('offers nothing when there is no base to act on', () => {
    render(<KnowledgeView />);
    expect(screen.queryByText(/tier control for/)).toBeNull();
  });
});
