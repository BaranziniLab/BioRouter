import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { GraphNode } from '../../../api/types.gen';
import { GRAPH_PALETTE, typeFill, typeShape } from '../../../styles/graphPalette';
import { NodePreview, nodeFacts, nodeSubtitle, nodeTitle } from './NodePreview';
import { credibilityKey, fillFor, shapeFor } from './nodeMark';
import { svgPathForShape } from './nodeShapes';

let pageContent = '# MYC\nA transcription factor.';

vi.mock('../hooks/usePagePreview', () => ({
  usePagePreview: () => ({ content: pageContent, loading: false, error: null }),
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

/** A source page, which is the only family §5.5 lets carry a credibility ring. */
const source = {
  id: 'hauser-2017',
  label: 'Hauser et al. 2017',
  identifier: 'Hauser et al. 2017',
  kind: 'source',
  node_type: 'Publication',
  credibility_tier: 'peer_reviewed',
  path: 'knowledge/hauser-2017.md',
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

describe('nodeFacts — §4.8 item 1', () => {
  it('gives draft a warning tone and deprecated a struck neutral one', () => {
    const draft = nodeFacts({ ...typed, status: 'draft' } as GraphNode);
    expect(draft.find((f) => f.key === 'status')).toMatchObject({
      text: 'draft',
      tone: 'warning',
    });

    const deprecated = nodeFacts({ ...typed, status: 'deprecated' } as GraphNode);
    expect(deprecated.find((f) => f.key === 'status')).toMatchObject({
      tone: 'neutral',
      strike: true,
    });
  });

  it('emits nothing at all for the ordinary status', () => {
    // §4.8: `stable` → nothing. Decorating the normal state spends attention on
    // the pages that need none and leaves the two that matter competing with it.
    expect(nodeFacts({ ...typed, status: 'stable' } as GraphNode)).toHaveLength(1);
  });

  it('still renders a status outside the vocabulary — there is no allowlist', () => {
    expect(nodeFacts({ ...typed, status: 'under-review' } as GraphNode)).toContainEqual(
      expect.objectContaining({ key: 'status', text: 'under-review' })
    );
  });

  it('carries subtype and stale as their own facts', () => {
    const facts = nodeFacts({ ...typed, subtype: 'biguanide', stale: true } as GraphNode);
    expect(facts).toContainEqual(expect.objectContaining({ key: 'subtype', text: 'biguanide' }));
    expect(facts).toContainEqual(
      expect.objectContaining({ key: 'stale', text: 'Stale', tone: 'warning' })
    );
  });
});

describe('NodePreview — credibility (§4.8 item 3)', () => {
  it('rings the tier and names it in a NEUTRAL badge', () => {
    render(<NodePreview kbId="soul" node={source} mode="light" onClose={() => undefined} />);
    const provenance = screen.getByRole('region', { name: 'Provenance' });

    expect(within(provenance).getByText('Peer reviewed')).toBeInTheDocument();
    // The ring is drawn in the tier hue, from the same palette the canvas rings
    // from — four arcs for `peer_reviewed`, not a hue-only encoding.
    const ring = provenance.querySelector('svg path, svg circle');
    expect(ring).toHaveAttribute('stroke', GRAPH_PALETTE.light.credibility.peer_reviewed);
    expect(credibilityKey(source)).toBe('peer_reviewed');
  });

  it('never paints the tier hue as a surface behind the word', () => {
    // The seven ring hues are solved for a 1.6px arc against the graph ground.
    // As a background under text they are neither legible nor a passing pair,
    // so the badge is app ink on an app surface and the hue stays a STROKE.
    const { container } = render(
      <NodePreview kbId="soul" node={source} mode="light" onClose={() => undefined} />
    );
    const hue = GRAPH_PALETTE.light.credibility.peer_reviewed;
    for (const el of Array.from(container.querySelectorAll<HTMLElement>('*'))) {
      expect(el.style.background).not.toContain(hue);
      expect(el.style.backgroundColor).not.toContain(hue);
      expect(el.getAttribute('fill')).not.toBe(hue);
    }
  });

  it('lets retraction override the tier, and says so in the danger tone', () => {
    const retracted = { ...source, retracted: true } as GraphNode;
    render(<NodePreview kbId="soul" node={retracted} mode="light" onClose={() => undefined} />);

    // Retraction is a flag rather than a rung and is the more important fact.
    expect(credibilityKey(retracted)).toBe('retracted');
    expect(screen.getByText('Retracted')).toBeInTheDocument();
  });

  it('shows no provenance section for a page that has no verdict', () => {
    render(<NodePreview kbId="soul" node={typed} mode="light" onClose={() => undefined} />);
    expect(screen.queryByRole('region', { name: 'Provenance' })).toBeNull();
  });
});

describe('NodePreview — frontmatter rows (§4.8 item 2)', () => {
  it('renders parsed rows rather than a raw YAML dump', () => {
    pageContent = '---\nidentifier: MS\nsynonyms:\n  - multiple sclerosis\n---\n# Heading\nbody';
    render(<NodePreview kbId="soul" node={typed} mode="light" onClose={() => undefined} />);

    expect(screen.getByText('identifier')).toBeInTheDocument();
    expect(screen.getByText('MS')).toBeInTheDocument();
    expect(screen.getByText('multiple sclerosis')).toBeInTheDocument();
    pageContent = '# MYC\nA transcription factor.';
  });

  it('falls back to the raw block when the YAML does not parse', () => {
    // A page whose frontmatter is malformed still has frontmatter the user
    // needs to SEE in order to fix it.
    pageContent = '---\na:\n  - [unclosed\n---\nbody';
    render(<NodePreview kbId="soul" node={typed} mode="light" onClose={() => undefined} />);
    expect(screen.getByText('Unparsed frontmatter')).toBeInTheDocument();
    pageContent = '# MYC\nA transcription factor.';
  });
});
