import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { GraphNode } from '../../../api/types.gen';
import { NodePreview } from './NodePreview';

vi.mock('../hooks/usePagePreview', () => ({
  usePagePreview: () => ({
    content: '# MYC\nA transcription factor.',
    loading: false,
    error: null,
  }),
}));

const node = {
  id: 'myc',
  label: 'MYC',
  kind: 'entity',
  path: 'entities/MYC.md',
} as GraphNode;

describe('NodePreview', () => {
  it('has dialog semantics and dismisses with Escape', () => {
    const onClose = vi.fn();
    render(<NodePreview kbId="soul" node={node} onClose={onClose} />);

    expect(screen.getByRole('dialog', { name: /Preview MYC/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Close preview' })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('dismisses when another control is clicked without swallowing that click', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const outside = vi.fn();
    render(
      <>
        <NodePreview kbId="soul" node={node} onClose={onClose} />
        <button type="button" onClick={outside}>
          Outside action
        </button>
      </>
    );

    await user.click(screen.getByRole('button', { name: 'Outside action' }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(outside).toHaveBeenCalledTimes(1);
  });
});
