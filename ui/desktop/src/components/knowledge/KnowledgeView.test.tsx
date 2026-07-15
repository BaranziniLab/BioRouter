import { fireEvent, render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import KnowledgeView from './KnowledgeView';

const mocks = vi.hoisted(() => ({
  refresh: vi.fn(),
}));

vi.mock('./KnowledgeContext', () => ({
  useKnowledge: () => ({ refresh: mocks.refresh }),
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
