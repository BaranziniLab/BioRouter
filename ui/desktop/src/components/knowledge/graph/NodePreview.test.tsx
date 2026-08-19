import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { GraphNode } from '../../../api/types.gen';
import { typeFill, typeShape } from '../../../styles/graphPalette';
import { NodePreview, nodeSubtitle, nodeTitle } from './NodePreview';
import { fillFor, shapeFor } from './nodeMark';
import { svgPathForShape } from './nodeShapes';

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

/**
 * A TYPED page, exactly as the deriver emits one: `kind` is `hub` for every
 * typed page, so anything the inspector reads off `kind` says "hub" for the
 * whole base while the canvas correctly draws a Drug.
 */
const typed = {
  id: 'metformin',
  label: 'metformin',
  identifier: 'Metformin',
  kind: 'hub',
  node_type: 'Drug',
  path: 'knowledge/metformin.md',
} as GraphNode;

describe('NodePreview', () => {
  it('has dialog semantics and dismisses with Escape', () => {
    const onClose = vi.fn();
    render(<NodePreview kbId="soul" node={node} mode="light" onClose={onClose} />);

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
        <NodePreview kbId="soul" node={node} mode="light" onClose={onClose} />
        <button type="button" onClick={outside}>
          Outside action
        </button>
      </>
    );

    await user.click(screen.getByRole('button', { name: 'Outside action' }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(outside).toHaveBeenCalledTimes(1);
  });

  it('titles a typed page with its identifier, not its slug', () => {
    expect(nodeTitle(typed)).toBe('Metformin');
    // The frontmatter block below the title has always shown `identifier`; a
    // title showing the slug made the panel contradict itself.
    expect(nodeTitle(typed)).not.toBe(typed.label);
  });

  it('subtitles a typed page with its node_type, never the legacy kind', () => {
    expect(nodeSubtitle(typed)).toBe('Drug');
    expect(nodeSubtitle(typed)).not.toContain('hub');
  });

  it('names an untyped page as untyped rather than inventing a type', () => {
    expect(nodeSubtitle(node)).toBe('Untyped · entity');
  });

  it('draws the same mark the canvas draws for the same node', () => {
    const { container } = render(
      <NodePreview kbId="soul" node={typed} mode="light" onClose={() => undefined} />
    );
    const path = container.querySelector('svg path');

    // Not "a colour and a shape" — the CANVAS's colour and shape, resolved
    // through the one function both surfaces call.
    expect(path).toHaveAttribute('fill', fillFor(typed, 'light'));
    expect(path).toHaveAttribute('d', svgPathForShape(shapeFor(typed, 'light')));
    // And that function keys on `node_type`, so it agrees with the palette.
    expect(fillFor(typed, 'light')).toBe(typeFill('Drug', 'light'));
    expect(shapeFor(typed, 'light')).toBe(typeShape('Drug', 'light'));
  });

  it('resolves the mark per mode, so the dark inspector is not the light one', () => {
    expect(fillFor(typed, 'dark')).not.toBe(fillFor(typed, 'light'));
  });
});
